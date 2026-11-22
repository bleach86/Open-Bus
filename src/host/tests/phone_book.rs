//! Zimodem's phone book: entries kept by number, in order, one replacing
//! another of its number, deleted; saved to its file in Zimodem's way, and
//! read back from it, a real card's file (its numbers with an `l` after)
//! too.

#[cfg(test)]
mod tests {
    use crate::host::files;
    use crate::host::phone_book::{Entry, PhoneBook};

    fn entry(number: u32, place: &str, modifiers: &str, note: &str) -> Entry {
        Entry {
            number,
            place: place.to_string(),
            modifiers: modifiers.to_string(),
            note: note.to_string(),
        }
    }

    #[test]
    fn entries_are_kept_by_number_saved_and_read_back() {
        let file =
            std::env::temp_dir().join(format!("cpu_emu_{}_zphonebook.txt", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let mut book = PhoneBook::default();
        book.keep_in(&file);
        assert!(book.entries().is_empty(), "no file yet: none");
        book.add(entry(64, "bbs.example.org:6400", "t", "PETSCII, a note"));
        book.add(entry(5, "open.bus:8888", "t", "Open Bus"));
        book.add(entry(64, "bbs.example.org:6401", "", ""));
        files::all_saved();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "5,open.bus:8888,t,Open Bus\n64,bbs.example.org:6401,,\n",
            "by number; 64 replaced"
        );
        assert!(book.delete(64));
        assert!(!book.delete(64), "gone already");
        files::all_saved();

        let mut again = PhoneBook::default();
        again.keep_in(&file);
        assert_eq!(
            again.entries(),
            [entry(5, "open.bus:8888", "t", "Open Bus")]
        );

        std::fs::write(&file, "7l,bbs.example.org:23,t,a note, with a comma\n").unwrap();
        let mut zimodems = PhoneBook::default();
        zimodems.keep_in(&file);
        assert_eq!(
            zimodems.find(7),
            Some(&entry(7, "bbs.example.org:23", "t", "a note, with a comma"))
        );
        std::fs::remove_file(&file).unwrap();
    }
}
