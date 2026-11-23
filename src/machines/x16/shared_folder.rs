//! One folder on the host, the X16's files, on every card Open Bus makes as
//! its FILES folder: what a program saves there (a terminal's downloads)
//! there to be opened on the host, and what's put there on the card of
//! whatever runs next. As a real X16's one SD card has them for all its
//! programs; their own saves stay on their own cards.
//!
//! As the X16 starts, the card's FILES is made the folder's (the card
//! written again only if it's not that already). As it stops, what the
//! program wrote in FILES or deleted from it while it ran is written to,
//! or deleted from, the folder: what was put in the folder meanwhile, or
//! changed there, is left be. A card Open Bus didn't make isn't shared.

use super::card::{self, CardFile, Made};
use crate::host::{clock, files, zip};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Where the folder is on a card.
pub const ON_CARD: &str = "FILES";

/// A file as it was put on the card: its path in the folder, as it's
/// spelt there, and its length and CRC, to know it again.
type Put = (String, usize, u32);

/// A card being shared, for its files to be brought back when the X16
/// stops.
pub struct Shared {
    card: PathBuf,
    folder: PathBuf,
    /// What was put in FILES, by its path in it, lower case (FAT's blind to
    /// case).
    put: BTreeMap<String, Put>,
}

fn known(bytes: &[u8]) -> (usize, u32) {
    (bytes.len(), zip::crc32(bytes))
}

/// A path on the card, in FILES: its path in the folder.
fn in_files(path: &str) -> Option<&str> {
    let (first, rest) = path.split_once('/')?;
    (first.eq_ignore_ascii_case(ON_CARD) && !rest.is_empty()).then_some(rest)
}

/// The folder's files, its own folders' too, by their paths (folders by
/// `/`), but hidden ones (a dot first).
fn folder_files(folder: &Path) -> Result<Vec<CardFile>, String> {
    let mut found = Vec::new();
    let mut to_look = vec![(folder.to_path_buf(), String::new())];
    while let Some((at, path)) = to_look.pop() {
        let entries = fs::read_dir(&at).map_err(|e| format!("{}: {e}", at.display()))?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let inside = format!("{path}{name}");
            let full = entry.path();
            if full.is_dir() {
                to_look.push((full, format!("{inside}/")));
            } else if let Ok(bytes) = fs::read(&full) {
                found.push((inside, bytes));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}

fn write_file(folder: &Path, path: &str, bytes: &[u8]) {
    let to = folder.join(path);
    if let Some(parent) = to.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::write(&to, bytes) {
        eprintln!("{}: {e}", to.display());
    }
}

/// The card at `card` given the folder `folder`'s files, in FILES, as the
/// X16 starts (the folder made, if it isn't there); `None` if it's not
/// shared (not Open Bus's, or not to be read).
pub fn share_in(card: &Path, folder: &Path) -> Option<Shared> {
    let shown = card.display();
    let image = files::read(card).ok()?;
    let fingerprint = match card::made(&image) {
        Made::Game(fingerprint) => Some(fingerprint),
        Made::Before => None,
        Made::Other => return None,
    };
    let on_card = card::files_of(&image)
        .map_err(|e| eprintln!("{shown}: {e}; the X16's files aren't on it"))
        .ok()?;
    if let Err(e) = fs::create_dir_all(folder) {
        eprintln!("{}: {e}", folder.display());
        return None;
    }
    let in_folder = folder_files(folder).map_err(|e| eprintln!("{e}")).ok()?;
    let put: BTreeMap<String, Put> = in_folder
        .iter()
        .map(|(path, bytes)| {
            let (length, crc) = known(bytes);
            (path.to_lowercase(), (path.clone(), length, crc))
        })
        .collect();

    let shared: Vec<(&str, &Vec<u8>)> = on_card
        .iter()
        .filter_map(|(path, bytes)| in_files(path).map(|path| (path, bytes)))
        .collect();
    let same = shared.len() == put.len()
        && shared.iter().all(|(path, bytes)| {
            put.get(&path.to_lowercase())
                .is_some_and(|(_, length, crc)| (*length, *crc) == known(bytes))
        });
    if !same {
        // The card made again: its own files, and the folder's in FILES.
        let mut files: Vec<CardFile> = on_card
            .into_iter()
            .filter(|(path, _)| in_files(path).is_none())
            .collect();
        files.extend(
            in_folder
                .into_iter()
                .map(|(path, bytes)| (format!("{ON_CARD}/{path}"), bytes)),
        );
        match card::card(&files, clock::now()) {
            Ok(mut image) => {
                if let Some(fingerprint) = fingerprint {
                    card::stamp(&mut image, fingerprint);
                }
                if let Err(e) = files::save(card, &image) {
                    eprintln!("{shown}: {e}");
                    return None;
                }
            }
            Err(e) => {
                eprintln!("{shown}: {e}; the X16's files aren't on it");
                return None;
            }
        }
    }
    Some(Shared {
        card: card.to_path_buf(),
        folder: folder.to_path_buf(),
        put,
    })
}

/// The X16 stopped: what its program wrote in FILES written to the folder,
/// and what it deleted deleted from it, if the folder's copy is still as
/// it was put.
pub fn share_out(shared: &Shared) {
    let Ok(image) = files::read(&shared.card) else {
        return;
    };
    let Ok(on_card) = card::files_of(&image) else {
        return;
    };
    let mut now = HashSet::new();
    for (path, bytes) in &on_card {
        let Some(path) = in_files(path) else {
            continue;
        };
        let key = path.to_lowercase();
        match shared.put.get(&key) {
            Some((_, length, crc)) if (*length, *crc) == known(bytes) => {}
            // New, or changed: as the folder spells it, if it's there.
            Some((spelt, _, _)) => write_file(&shared.folder, spelt, bytes),
            None => write_file(&shared.folder, path, bytes),
        }
        now.insert(key);
    }
    for (key, (spelt, length, crc)) in &shared.put {
        if now.contains(key) {
            continue;
        }
        let file = shared.folder.join(spelt);
        let unchanged = fs::read(&file).is_ok_and(|bytes| known(&bytes) == (*length, *crc));
        if unchanged {
            let _ = fs::remove_file(&file);
        }
    }
}
