//! Zimodem's phone book: places kept by a number of up to 9 digits, each
//! with the modifiers it's dialled with (`t`, telnet's) and a note, as
//! `ATP"5=bbs.example.org:23,a note"` keeps one and `ATD5` dials it. Kept
//! in a file as Zimodem keeps its in the ESP32's flash (`zphonebook.txt`):
//! a line an entry, its number, place, modifiers and note, by commas;
//! saved as it changes.

use crate::host::files;
use std::path::{Path, PathBuf};

/// The most digits a number has.
pub const DIGITS: usize = 9;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub number: u32,
    /// Its host and port, `bbs.example.org:23`.
    pub place: String,
    /// Zimodem's dial modifiers, in lower case, as it keeps them: `t`.
    pub modifiers: String,
    pub note: String,
}

#[derive(Default)]
pub struct PhoneBook {
    /// By number.
    entries: Vec<Entry>,
    file: Option<PathBuf>,
}

impl PhoneBook {
    /// Kept in the file at `path`: read from it, if it's there, and saved
    /// to it as it changes.
    pub fn keep_in(&mut self, path: &Path) {
        if let Ok(bytes) = files::read(path) {
            self.entries = String::from_utf8_lossy(&bytes)
                .lines()
                .filter_map(entry)
                .collect();
            self.entries.sort_by_key(|entry| entry.number);
            self.entries.dedup_by_key(|entry| entry.number);
        }
        self.file = Some(path.to_path_buf());
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn find(&self, number: u32) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.number == number)
    }

    /// `entry` kept, in place of any of its number.
    pub fn add(&mut self, entry: Entry) {
        match self
            .entries
            .binary_search_by_key(&entry.number, |kept| kept.number)
        {
            Ok(at) => self.entries[at] = entry,
            Err(at) => self.entries.insert(at, entry),
        }
        self.save();
    }

    /// The entry of `number` gone: whether there was one.
    pub fn delete(&mut self, number: u32) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.number != number);
        let gone = self.entries.len() < before;
        if gone {
            self.save();
        }
        gone
    }

    fn save(&self) {
        let Some(path) = &self.file else { return };
        let text: String = self
            .entries
            .iter()
            .map(|entry| {
                format!(
                    "{},{},{},{}\n",
                    entry.number, entry.place, entry.modifiers, entry.note
                )
            })
            .collect();
        files::save_later(path, text.into_bytes());
    }
}

/// An entry, from its line in the file. Its number is the digits it
/// starts with, as Zimodem reads it: Zimodem writes an `l` after them.
fn entry(line: &str) -> Option<Entry> {
    let mut parts = line.splitn(4, ',');
    let number = parts.next()?;
    let digits = number.len()
        - number
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .len();
    let number = number[..digits].parse().ok()?;
    let place = parts.next()?.to_string();
    let modifiers = parts.next().unwrap_or("").to_string();
    let note = parts.next().unwrap_or("").to_string();
    Some(Entry {
        number,
        place,
        modifiers,
        note,
    })
}
