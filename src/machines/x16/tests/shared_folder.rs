//! A card's files as a folder beside it: made from the card the first
//! time, the program's own left out; what's put in the folder on the card
//! as the X16 starts, what's deleted from it gone; what the program wrote
//! brought back as it stops, what it deleted deleted, and what was put in
//! the folder or changed there meanwhile left be.

#[cfg(test)]
mod tests {
    use crate::host::clock::DateTime;
    use crate::machines::x16::card::{self, CardFile};
    use crate::machines::x16::shared_folder::{folder_of, share_in, share_out};
    use crate::machines::x16::{self};
    use std::fs;
    use std::path::Path;

    const WHEN: DateTime = DateTime {
        year: 2026,
        month: 11,
        day: 22,
        weekday: 0,
        hour: 12,
        minute: 0,
        second: 0,
    };

    /// The card's files, sorted.
    fn on(card: &Path) -> Vec<CardFile> {
        let mut files = card::files_of(&fs::read(card).unwrap()).unwrap();
        files.sort();
        files
    }

    /// The program writes `path` on its card (or, with no bytes, deletes
    /// it), as its DOS would: the boot sector, and its mark, as they were.
    fn program_writes(card: &Path, path: &str, bytes: Option<&[u8]>) {
        let image = fs::read(card).unwrap();
        let mut files = card::files_of(&image).unwrap();
        files.retain(|(name, _)| name != path);
        if let Some(bytes) = bytes {
            files.push((path.to_string(), bytes.to_vec()));
        }
        let mut again = card::card(&files, WHEN).unwrap();
        let start = 2048 * 512;
        again[start..start + 512].copy_from_slice(&image[start..start + 512]);
        fs::write(card, again).unwrap();
    }

    fn read(path: &Path) -> String {
        String::from_utf8(fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn a_cards_files_are_a_folder_beside_it_both_ways() {
        let root = std::env::temp_dir().join(format!("cpu_emu_{}_x16_shared", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let game = root.join("Term");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("TERM.PRG"), [0x01, 0x08, 1]).unwrap();
        let card = root.join("Term.img");
        x16::make_card(&game, &card).unwrap();
        let folder = folder_of(&card);
        assert_eq!(folder, root.join("Term files"));

        // Saved on it before there were folders: the folder made from it.
        program_writes(&card, "TERM.CFG", Some(b"start=modem"));
        let shared = share_in(Some(&game), &card).unwrap();
        assert_eq!(read(&folder.join("TERM.CFG")), "start=modem");
        assert!(!folder.join("TERM.PRG").exists(), "the program's own");
        assert!(!folder.join("AUTOBOOT.X16").exists());

        // As it runs: a download saved; a file put in the folder.
        program_writes(&card, "DOWNLOADS/GAME.ZIP", Some(b"zipped"));
        fs::write(folder.join("UPLOAD.TXT"), "from the PC").unwrap();
        share_out(&shared);
        assert_eq!(read(&folder.join("DOWNLOADS/GAME.ZIP")), "zipped");
        assert_eq!(read(&folder.join("UPLOAD.TXT")), "from the PC", "left be");

        // The next time: the folder's file on the card, one deleted from
        // the folder gone from it, the program's still there, its mark too.
        fs::remove_file(folder.join("TERM.CFG")).unwrap();
        let shared = share_in(Some(&game), &card).unwrap();
        let names: Vec<String> = on(&card).into_iter().map(|(path, _)| path).collect();
        assert_eq!(
            names,
            [
                "AUTOBOOT.X16",
                "DOWNLOADS/GAME.ZIP",
                "TERM.PRG",
                "UPLOAD.TXT"
            ]
        );
        assert!(matches!(
            card::made(&fs::read(&card).unwrap()),
            card::Made::Game(_)
        ));
        // Unchanged: not written again.
        let before = fs::read(&card).unwrap();
        share_out(&shared);
        let shared = share_in(Some(&game), &card).unwrap();
        assert!(
            fs::read(&card).unwrap() == before,
            "the same: left as it is"
        );

        // As it runs: the program deletes the download; the PC adds a
        // file.
        program_writes(&card, "DOWNLOADS/GAME.ZIP", None);
        fs::write(folder.join("NEW.TXT"), "added").unwrap();
        share_out(&shared);
        assert!(!folder.join("DOWNLOADS/GAME.ZIP").exists(), "deleted");
        assert_eq!(read(&folder.join("NEW.TXT")), "added");
        assert_eq!(read(&folder.join("UPLOAD.TXT")), "from the PC");

        // A file the PC changed while it ran isn't deleted by the program's
        // deleting it.
        let shared = share_in(Some(&game), &card).unwrap();
        fs::write(folder.join("NEW.TXT"), "changed on the PC").unwrap();
        program_writes(&card, "NEW.TXT", None);
        share_out(&shared);
        assert_eq!(read(&folder.join("NEW.TXT")), "changed on the PC");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_made_empty_before_its_first_run_is_given_the_cards_files() {
        let root = std::env::temp_dir().join(format!("cpu_emu_{}_x16_empty", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let game = root.join("Term");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("TERM.PRG"), [0x01, 0x08, 1]).unwrap();
        let card = root.join("Term.img");
        x16::make_card(&game, &card).unwrap();
        program_writes(&card, "TERM.DIR", Some(b"boards"));
        // The app made it, to open it, and something was put in it.
        let folder = folder_of(&card);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("PUT.TXT"), "put").unwrap();
        share_in(Some(&game), &card).unwrap();
        assert_eq!(read(&folder.join("TERM.DIR")), "boards", "not lost");
        let names: Vec<String> = on(&card).into_iter().map(|(path, _)| path).collect();
        assert_eq!(names, ["AUTOBOOT.X16", "PUT.TXT", "TERM.DIR", "TERM.PRG"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_card_open_bus_didnt_make_isnt_shared() {
        let root = std::env::temp_dir().join(format!("cpu_emu_{}_x16_other", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let card = root.join("theirs.img");
        fs::write(&card, vec![0; 1 << 20]).unwrap();
        assert!(share_in(None, &card).is_none());
        assert!(!folder_of(&card).exists());
        let _ = fs::remove_dir_all(&root);
    }
}
