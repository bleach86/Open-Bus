//! The X16's files, one folder of the host's, on every card as FILES:
//! put there as the X16 starts, what's deleted from the folder gone; what
//! the program wrote in FILES brought back as it stops, what it deleted
//! deleted, and what was put in the folder or changed there meanwhile left
//! be; a program's own saves left on its own card; one program's file
//! there for the next.

#[cfg(test)]
mod tests {
    use crate::host::clock::DateTime;
    use crate::machines::x16::card::{self, CardFile};
    use crate::machines::x16::shared_folder::{share_in, share_out};
    use crate::machines::x16::{self};
    use std::fs;
    use std::path::{Path, PathBuf};

    const WHEN: DateTime = DateTime {
        year: 2026,
        month: 11,
        day: 22,
        weekday: 0,
        hour: 12,
        minute: 0,
        second: 0,
    };

    fn names(card: &Path) -> Vec<String> {
        let mut names: Vec<String> = card::files_of(&fs::read(card).unwrap())
            .unwrap()
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        names.sort();
        names
    }

    /// The program writes `path` on its card (or, with no bytes, deletes
    /// it), as its DOS would: the boot sector, and its mark, as they were.
    fn program_writes(card: &Path, path: &str, bytes: Option<&[u8]>) {
        let image = fs::read(card).unwrap();
        let mut files: Vec<CardFile> = card::files_of(&image).unwrap();
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

    /// A game, `name`, its card made, in `root`.
    fn game_card(root: &Path, name: &str) -> PathBuf {
        let game = root.join(name);
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join(format!("{name}.PRG")), [0x01, 0x08, 1]).unwrap();
        let card = root.join(format!("{name}.img"));
        x16::make_card(&game, &card).unwrap();
        card
    }

    #[test]
    fn the_x16s_files_are_one_folder_on_every_card_both_ways() {
        let root = std::env::temp_dir().join(format!("cpu_emu_{}_x16_files", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let folder = root.join("X16 Files");
        let card = game_card(&root, "TERM");

        // The folder made, empty; a file put in it, on the card in FILES.
        let shared = share_in(&card, &folder).unwrap();
        assert!(folder.is_dir());
        share_out(&shared);
        fs::write(folder.join("UPLOAD.TXT"), "from the PC").unwrap();
        let shared = share_in(&card, &folder).unwrap();
        assert_eq!(
            names(&card),
            ["AUTOBOOT.X16", "FILES/UPLOAD.TXT", "TERM.PRG"]
        );
        assert!(matches!(
            card::made(&fs::read(&card).unwrap()),
            card::Made::Game(_)
        ));

        // As it runs: a download in FILES, its own settings beside it; and
        // a file put in the folder.
        program_writes(&card, "FILES/DL/GAME.ZIP", Some(b"zipped"));
        program_writes(&card, "TERM.CFG", Some(b"start=modem"));
        fs::write(folder.join("NEW.TXT"), "added").unwrap();
        share_out(&shared);
        assert_eq!(read(&folder.join("DL/GAME.ZIP")), "zipped");
        assert!(!folder.join("TERM.CFG").exists(), "its own, on its card");
        assert_eq!(read(&folder.join("NEW.TXT")), "added", "left be");

        // The next time: what's deleted from the folder gone from FILES,
        // its own settings still there; the same again: not written again.
        fs::remove_file(folder.join("UPLOAD.TXT")).unwrap();
        let shared = share_in(&card, &folder).unwrap();
        assert_eq!(
            names(&card),
            [
                "AUTOBOOT.X16",
                "FILES/DL/GAME.ZIP",
                "FILES/NEW.TXT",
                "TERM.CFG",
                "TERM.PRG"
            ]
        );
        share_out(&shared);
        let before = fs::read(&card).unwrap();
        let shared = share_in(&card, &folder).unwrap();
        assert!(
            fs::read(&card).unwrap() == before,
            "the same: left as it is"
        );

        // It deletes the download; the PC changes NEW.TXT, which it
        // deletes too: the PC's kept.
        program_writes(&card, "FILES/DL/GAME.ZIP", None);
        program_writes(&card, "FILES/NEW.TXT", None);
        fs::write(folder.join("NEW.TXT"), "changed on the PC").unwrap();
        share_out(&shared);
        assert!(!folder.join("DL/GAME.ZIP").exists(), "deleted");
        assert_eq!(read(&folder.join("NEW.TXT")), "changed on the PC");

        // Another program's card: the same files.
        let other = game_card(&root, "BASIC");
        share_in(&other, &folder).unwrap();
        assert!(names(&other).contains(&"FILES/NEW.TXT".to_string()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_card_open_bus_didnt_make_isnt_shared() {
        let root = std::env::temp_dir().join(format!("cpu_emu_{}_x16_other", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let card = root.join("theirs.img");
        fs::write(&card, vec![0; 1 << 20]).unwrap();
        assert!(share_in(&card, &root.join("X16 Files")).is_none());
        assert!(!root.join("X16 Files").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
