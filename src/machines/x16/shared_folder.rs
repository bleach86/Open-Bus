//! An X16 card's files, but its program's own, as a folder on the host
//! beside it, named for it ("busterm files" for busterm.img): what a
//! program saves on its card there to be opened, and what's put there on
//! the card the next time it runs. A terminal's downloads, a game's saves,
//! BASIC's programs.
//!
//! As the X16 starts, the card's made to have the folder's files (written
//! again only if they're not what's on it); the first time (no folder yet,
//! or one with no mark of its own), the card's files are put in the folder
//! first. As it stops, what the program
//! wrote or deleted on the card while it ran is written to, or deleted
//! from, the folder: what was put in the folder meanwhile, or changed in
//! it, is left be. The program's own files (its game's, and AUTOBOOT.X16)
//! aren't in it, and a card Open Bus didn't make isn't shared at all.

use super::card::{self, CardFile, Made};
use crate::host::{clock, files, zip};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The folder's mark, hidden, once it's had the card's files put in it:
/// from then on, it's what the card has.
const MARK: &str = ".open-bus-card";
const MARK_SAYS: &str = "Open Bus keeps this folder and the X16 card beside it the same.\n";

/// A file as it was put on the card: its name as spelt in the folder, and
/// its length and CRC, to know it again.
type Put = (String, usize, u32);

/// A card being shared, for its files to be brought back when the X16
/// stops.
pub struct Shared {
    card: PathBuf,
    folder: PathBuf,
    /// The program's own files, lower case: not shared.
    own: HashSet<String>,
    /// What was put on the card, by its path, lower case (FAT's blind to
    /// case).
    put: BTreeMap<String, Put>,
}

/// The folder beside the card `card`: its name and " files".
pub fn folder_of(card: &Path) -> PathBuf {
    let stem = card
        .file_stem()
        .map_or("card".into(), |stem| stem.to_string_lossy().into_owned());
    card.with_file_name(format!("{stem} files"))
}

fn known(bytes: &[u8]) -> (usize, u32) {
    (bytes.len(), zip::crc32(bytes))
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

/// The card at `card`, for the game at `game` (or none: the X16's own),
/// given the folder's files as it starts; `None` if it's not shared (not
/// Open Bus's, or not to be read).
pub fn share_in(game: Option<&Path>, card: &Path) -> Option<Shared> {
    let shown = card.display();
    let image = files::read(card).ok()?;
    let fingerprint = match card::made(&image) {
        Made::Game(fingerprint) => Some(fingerprint),
        Made::Before => None,
        Made::Other => return None,
    };
    let on_card = card::files_of(&image)
        .map_err(|e| eprintln!("{shown}: {e}; its files aren't shared"))
        .ok()?;
    let own: HashSet<String> = match game {
        Some(game) => super::game_files(&super::game_of(game))
            .and_then(|files| card::game(&super::game_name(game), files))
            .map(|made| {
                made.files
                    .iter()
                    .map(|(path, _)| path.to_lowercase())
                    .collect()
            })
            .unwrap_or_default(),
        None => HashSet::new(),
    };
    let is_own = |path: &str| own.contains(&path.to_lowercase());
    let folder = folder_of(card);
    let theirs: Vec<&CardFile> = on_card.iter().filter(|(path, _)| !is_own(path)).collect();

    let mark = folder.join(MARK);
    if !files::exists(&mark) {
        // The first time (the folder not there yet, or made by the app to
        // be opened, empty): what's on the card put in it, but over
        // nothing that's there already.
        let _ = fs::create_dir_all(&folder);
        for (path, bytes) in &theirs {
            if !folder.join(path).exists() {
                write_file(&folder, path, bytes);
            }
        }
        if let Err(e) = fs::write(&mark, MARK_SAYS) {
            eprintln!("{}: {e}", mark.display());
            return None;
        }
    }
    let in_folder: Vec<CardFile> = folder_files(&folder)
        .map_err(|e| eprintln!("{e}"))
        .ok()?
        .into_iter()
        .filter(|(path, _)| !is_own(path))
        .collect();

    let put: BTreeMap<String, Put> = in_folder
        .iter()
        .map(|(path, bytes)| {
            let (length, crc) = known(bytes);
            (path.to_lowercase(), (path.clone(), length, crc))
        })
        .collect();
    let same = theirs.len() == put.len()
        && theirs.iter().all(|(path, bytes)| {
            put.get(&path.to_lowercase())
                .is_some_and(|(_, length, crc)| (*length, *crc) == known(bytes))
        });
    if !same {
        // The card made again: the program's own, and the folder's.
        let mut files: Vec<CardFile> = on_card
            .iter()
            .filter(|(path, _)| is_own(path))
            .cloned()
            .collect();
        files.extend(in_folder);
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
                eprintln!("{shown}: {e}; the folder's files aren't on it");
                return None;
            }
        }
    }
    Some(Shared {
        card: card.to_path_buf(),
        folder,
        own,
        put,
    })
}

/// The X16 stopped: what its program wrote on the card written to the
/// folder, and what it deleted deleted from it, if the folder's copy is
/// still as it was put.
pub fn share_out(shared: &Shared) {
    let Ok(image) = files::read(&shared.card) else {
        return;
    };
    let Ok(on_card) = card::files_of(&image) else {
        return;
    };
    let mut now = HashSet::new();
    for (path, bytes) in &on_card {
        let key = path.to_lowercase();
        if shared.own.contains(&key) {
            continue;
        }
        now.insert(key.clone());
        match shared.put.get(&key) {
            Some((_, length, crc)) if (*length, *crc) == known(bytes) => {}
            // New, or changed: as the folder spells it, if it's there.
            Some((spelt, _, _)) => write_file(&shared.folder, spelt, bytes),
            None => write_file(&shared.folder, path, bytes),
        }
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
