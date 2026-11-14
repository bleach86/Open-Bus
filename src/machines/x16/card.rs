//! SD cards for the X16, made: a game's files on a card of its own, with
//! an AUTOBOOT.X16 that runs it, the ROM loading that at power-on.
//!
//! A card's an MBR and one FAT32 partition from 1M on, as
//! tools/make-x16-sd has mtools make it: clusters of a sector, 2 FATs, the
//! root at cluster 2, an FSInfo sector and a backup boot sector. Its size
//! is its files' and 16M more, 40M at least, FAT32's 65,525 clusters. Each
//! file's and folder's clusters follow on from the last. A name that
//! isn't 8.3, upper case, has a long name, its short one made up as
//! Windows makes them, NAME~1.EXT.
//!
//! A game's files are a zip's or a folder's, as they are, but for one
//! folder holding them all: its files. Its program's one that loads where
//! BASIC's do, `$0801`, named .PRG or with no ending at all: one its name's
//! part of the game's, or the shortest; AUTOBOOT.X16 a line of BASIC, `10 LOAD"PROGRAM"`, which
//! loads it, and runs it, as a LOAD in a program does.

use crate::host::clock::DateTime;
use std::collections::{BTreeMap, HashSet};

const SECTOR: usize = 512;
/// The partition's first sector; its reserved sectors, the boot sector's,
/// FSInfo's (1) and the backups' (6 and 7).
const START: usize = 2048;
const RESERVED: usize = 32;
const FATS: usize = 2;
const MIN_CLUSTERS: usize = 65_525;
const ROOT: u32 = 2;
/// A cluster chain's end.
const END: u32 = 0x0FFF_FFFF;
const MB: usize = 1 << 20;

/// Directory entries' attributes.
const VOLUME: u8 = 0x08;
const FOLDER: u8 = 0x10;
const ARCHIVE: u8 = 0x20;
const LONG_NAME: u8 = 0x0F;

/// A file for a card: its path, folders by `/`, and its bytes.
pub type CardFile = (String, Vec<u8>);

/// A folder, as it's put on the card: its files and folders, by name.
#[derive(Default)]
struct Folder {
    files: BTreeMap<String, Vec<u8>>,
    folders: BTreeMap<String, Folder>,
    /// Its cluster, and how many it takes.
    cluster: u32,
    clusters: usize,
}

/// A folder's entries' 32 bytes each: a long name's, then the short.
fn entries_of(name: &str) -> usize {
    if is_short(name) {
        1
    } else {
        1 + name.encode_utf16().count().div_ceil(13)
    }
}

/// Whether `name` is a short name as it is: 8.3, upper case.
fn is_short(name: &str) -> bool {
    let (base, ext) = name.split_once('.').unwrap_or((name, ""));
    let ok =
        |part: &str, most: usize| (1..=most).contains(&part.len()) && part.bytes().all(short_char);
    ok(base, 8) && (ext.is_empty() || ok(ext, 3)) && name != "." && name != ".."
}

fn short_char(c: u8) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit() || b"!#$%&'()-@^_`{}~".contains(&c)
}

/// `name`'s short name, 8 and 3, padded: as it is, if it's short; else
/// made up, NAME~N.EXT, one not `taken`.
fn short_name(name: &str, taken: &mut HashSet<[u8; 11]>) -> [u8; 11] {
    let pad = |base: &[u8], ext: &[u8]| {
        let mut short = [b' '; 11];
        short[..base.len()].copy_from_slice(base);
        short[8..8 + ext.len()].copy_from_slice(ext);
        short
    };
    if is_short(name) {
        let (base, ext) = name.split_once('.').unwrap_or((name, ""));
        let short = pad(base.as_bytes(), ext.as_bytes());
        if taken.insert(short) {
            return short;
        }
    }
    let clean = |part: &str| -> Vec<u8> {
        part.bytes()
            .filter(|&c| c != b' ' && c != b'.')
            .map(|c| c.to_ascii_uppercase())
            .map(|c| if short_char(c) { c } else { b'_' })
            .collect()
    };
    let (base, ext) = match name.rsplit_once('.') {
        Some((base, ext)) if !base.is_empty() => (clean(base), clean(ext)),
        _ => (clean(name), Vec::new()),
    };
    let ext = &ext[..ext.len().min(3)];
    for n in 1.. {
        let tail = format!("~{n}");
        let keep = base.len().min(8 - tail.len());
        let mut made = base[..keep].to_vec();
        made.extend(tail.bytes());
        let short = pad(&made, ext);
        if taken.insert(short) {
            return short;
        }
    }
    unreachable!()
}

/// A short name's checksum, as its long name's entries have it.
fn checksum(short: &[u8; 11]) -> u8 {
    short
        .iter()
        .fold(0u8, |sum, &c| sum.rotate_right(1).wrapping_add(c))
}

/// FAT's time and date of `when`.
fn fat_time(when: DateTime) -> (u16, u16) {
    let time = (when.hour << 11 | when.minute << 5 | (when.second / 2)) as u16;
    let year = (when.year - 1980).clamp(0, 127) as u32;
    let date = (year << 9 | when.month << 5 | when.day) as u16;
    (time, date)
}

/// The card being written: its bytes, its clusters' first sector.
struct Card {
    bytes: Vec<u8>,
    data: usize,
    time: (u16, u16),
}

impl Card {
    fn cluster_at(&self, cluster: u32) -> usize {
        (self.data + cluster as usize - 2) * SECTOR
    }

    /// A short entry: its name, attributes, first cluster and size.
    fn entry(&self, short: &[u8; 11], attributes: u8, cluster: u32, size: u32) -> [u8; 32] {
        let mut entry = [0u8; 32];
        entry[..11].copy_from_slice(short);
        entry[11] = attributes;
        let (time, date) = self.time;
        for at in [14, 22] {
            entry[at..at + 2].copy_from_slice(&time.to_le_bytes());
        }
        for at in [16, 18, 24] {
            entry[at..at + 2].copy_from_slice(&date.to_le_bytes());
        }
        entry[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
        entry[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
        entry[28..32].copy_from_slice(&size.to_le_bytes());
        entry
    }

    /// A folder's entries written, and its files', and its folders', in
    /// turn; `parent` its parent's cluster, 0 for the root.
    fn write_folder(&mut self, folder: &Folder, parent: Option<u32>) {
        let mut entries: Vec<[u8; 32]> = Vec::new();
        let mut taken = HashSet::new();
        match parent {
            None => entries.push(self.entry(b"X16        ", VOLUME, 0, 0)),
            Some(parent) => {
                entries.push(self.entry(b".          ", FOLDER, folder.cluster, 0));
                entries.push(self.entry(b"..         ", FOLDER, parent, 0));
            }
        }
        let mut named = |name: &str, entry: &dyn Fn(&[u8; 11]) -> [u8; 32]| {
            let short = short_name(name, &mut taken);
            if !is_short(name) {
                entries.extend(long_entries(name, &short));
            }
            entries.push(entry(&short));
        };
        // Clusters as `allocate` gave them: files, then folders.
        let mut cluster = folder.cluster + folder.clusters as u32;
        let mut files = Vec::new();
        for (name, bytes) in &folder.files {
            let first = if bytes.is_empty() { 0 } else { cluster };
            named(name, &|short| {
                self.entry(short, ARCHIVE, first, bytes.len() as u32)
            });
            files.push((first, bytes));
            cluster += bytes.len().div_ceil(SECTOR) as u32;
        }
        for (name, sub) in &folder.folders {
            named(name, &|short| self.entry(short, FOLDER, sub.cluster, 0));
        }
        let at = self.cluster_at(folder.cluster);
        for (n, entry) in entries.iter().enumerate() {
            self.bytes[at + n * 32..at + n * 32 + 32].copy_from_slice(entry);
        }
        for (first, bytes) in files {
            if first != 0 {
                let at = self.cluster_at(first);
                self.bytes[at..at + bytes.len()].copy_from_slice(bytes);
            }
        }
        let own = if parent.is_none() { 0 } else { folder.cluster };
        for sub in folder.folders.values() {
            self.write_folder(sub, Some(own));
        }
    }
}

/// A long name's entries, last first, as they go before its short one.
fn long_entries(name: &str, short: &[u8; 11]) -> Vec<[u8; 32]> {
    let mut chars: Vec<u16> = name.encode_utf16().collect();
    let count = chars.len().div_ceil(13);
    if !chars.len().is_multiple_of(13) {
        chars.push(0);
    }
    chars.resize(count * 13, 0xFFFF);
    let sum = checksum(short);
    (0..count)
        .rev()
        .map(|n| {
            let mut entry = [0u8; 32];
            entry[0] = (n + 1) as u8 | if n + 1 == count { 0x40 } else { 0 };
            entry[11] = LONG_NAME;
            entry[13] = sum;
            let part = &chars[n * 13..n * 13 + 13];
            let places = (1..11)
                .step_by(2)
                .chain((14..26).step_by(2))
                .chain((28..32).step_by(2));
            for (at, &c) in places.zip(part) {
                entry[at..at + 2].copy_from_slice(&c.to_le_bytes());
            }
            entry
        })
        .collect()
}

/// Each folder's clusters given out, from `next`: its own, then its
/// files', then its folders'.
fn allocate(folder: &mut Folder, next: &mut u32, root: bool) {
    let names = folder.files.keys().chain(folder.folders.keys());
    let entries = if root { 1 } else { 2 } + names.map(|name| entries_of(name)).sum::<usize>();
    folder.clusters = (entries * 32).div_ceil(SECTOR).max(1);
    folder.cluster = *next;
    *next += folder.clusters as u32;
    for bytes in folder.files.values() {
        *next += bytes.len().div_ceil(SECTOR) as u32;
    }
    for sub in folder.folders.values_mut() {
        allocate(sub, next, false);
    }
}

/// Each cluster's FAT entry, a folder's and its files' chains.
fn chains(folder: &Folder, fat: &mut [u32]) {
    let mut chain = |first: u32, count: usize| {
        for n in 0..count as u32 {
            fat[(first + n) as usize] = if n + 1 == count as u32 {
                END
            } else {
                first + n + 1
            };
        }
    };
    chain(folder.cluster, folder.clusters);
    let mut cluster = folder.cluster + folder.clusters as u32;
    for bytes in folder.files.values() {
        let count = bytes.len().div_ceil(SECTOR);
        if count > 0 {
            chain(cluster, count);
        }
        cluster += count as u32;
    }
    for sub in folder.folders.values() {
        chains(sub, fat);
    }
}

/// A card of `files`, made at `when`: its image's bytes.
pub fn card(files: &[CardFile], when: DateTime) -> Result<Vec<u8>, String> {
    let mut root = Folder::default();
    for (path, bytes) in files {
        let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        let Some((name, folders)) = parts.split_last() else {
            continue;
        };
        if parts.iter().any(|&part| part == "." || part == "..") {
            return Err(format!("{path}: not a name for a card"));
        }
        let mut folder = &mut root;
        for &part in folders {
            folder = folder.folders.entry(part.to_string()).or_default();
        }
        folder.files.insert(name.to_string(), bytes.clone());
    }
    let mut next = ROOT;
    allocate(&mut root, &mut next, true);
    let used = (next - ROOT) as usize;
    // Its size, in whole megabytes; its FATs as big as its clusters need.
    let size = ((used * SECTOR + 16 * MB).div_ceil(MB) + 1).max(40) * MB;
    let sectors = size / SECTOR - START;
    let mut fat_sectors = (sectors * 4).div_ceil(SECTOR);
    let mut clusters = 0;
    for _ in 0..4 {
        clusters = sectors - RESERVED - FATS * fat_sectors;
        fat_sectors = ((clusters + 2) * 4).div_ceil(SECTOR);
    }
    if clusters < MIN_CLUSTERS.max(used) {
        return Err("too few clusters for FAT32".to_string());
    }
    let mut card = Card {
        bytes: vec![0; size],
        data: START + RESERVED + FATS * fat_sectors,
        time: fat_time(when),
    };
    let le32 = |bytes: &mut [u8], at: usize, value: u32| {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes())
    };
    // The MBR: one partition, FAT32 by LBA.
    let mbr = &mut card.bytes[..SECTOR];
    mbr[446..450].copy_from_slice(&[0x00, 0xFE, 0xFF, 0xFF]);
    mbr[450..454].copy_from_slice(&[0x0C, 0xFE, 0xFF, 0xFF]);
    le32(mbr, 454, START as u32);
    le32(mbr, 458, sectors as u32);
    mbr[510..512].copy_from_slice(&[0x55, 0xAA]);
    // The boot sector, its BPB, and FSInfo; their backups.
    let mut boot = [0u8; SECTOR];
    boot[..11].copy_from_slice(b"\xEB\x58\x90MSWIN4.1");
    boot[11..13].copy_from_slice(&(SECTOR as u16).to_le_bytes());
    boot[13] = 1; // a sector a cluster
    boot[14..16].copy_from_slice(&(RESERVED as u16).to_le_bytes());
    boot[16] = FATS as u8;
    boot[21] = 0xF8;
    boot[24..26].copy_from_slice(&32u16.to_le_bytes());
    boot[26..28].copy_from_slice(&64u16.to_le_bytes());
    le32(&mut boot, 28, START as u32);
    le32(&mut boot, 32, sectors as u32);
    le32(&mut boot, 36, fat_sectors as u32);
    le32(&mut boot, 44, ROOT);
    boot[48..50].copy_from_slice(&1u16.to_le_bytes());
    boot[50..52].copy_from_slice(&6u16.to_le_bytes());
    boot[64] = 0x80;
    boot[66] = 0x29;
    le32(&mut boot, 67, 0x0016_0000 | (card.time.1 as u32));
    boot[71..90].copy_from_slice(b"X16        FAT32   ");
    boot[510..512].copy_from_slice(&[0x55, 0xAA]);
    let mut info = [0u8; SECTOR];
    le32(&mut info, 0, 0x4161_5252);
    le32(&mut info, 484, 0x6141_7272);
    le32(&mut info, 488, (clusters - used) as u32);
    le32(&mut info, 492, next);
    le32(&mut info, 508, 0xAA55_0000);
    for (sector, bytes) in [(0, &boot), (1, &info), (6, &boot), (7, &info)] {
        let at = (START + sector) * SECTOR;
        card.bytes[at..at + SECTOR].copy_from_slice(bytes);
    }
    // The FATs.
    let mut fat = vec![0u32; clusters + 2];
    (fat[0], fat[1]) = (0x0FFF_FFF8, END);
    chains(&root, &mut fat);
    for n in 0..FATS {
        let at = (START + RESERVED + n * fat_sectors) * SECTOR;
        for (cluster, &entry) in fat.iter().enumerate() {
            le32(&mut card.bytes, at + cluster * 4, entry);
        }
    }
    card.write_folder(&root, None);
    Ok(card.bytes)
}

/// A game's files on a card, and the program they're run by.
pub struct Game {
    pub files: Vec<CardFile>,
    pub program: String,
}

/// Its letters and digits, upper case: a name, to compare.
fn plain(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// The game `name`'s files, as a zip or a folder has them: its program
/// found, and an AUTOBOOT.X16 to run it, if it hasn't one.
pub fn game(name: &str, mut files: Vec<CardFile>) -> Result<Game, String> {
    files.retain(|(path, _)| !path.ends_with('/'));
    // One folder holding them all: its files.
    let first = |path: &str| path.split_once('/').map(|(folder, _)| folder.to_string());
    let folder = files.first().and_then(|(path, _)| first(path));
    if let Some(folder) = folder.filter(|folder| {
        files
            .iter()
            .all(|(path, _)| first(path).as_ref() == Some(folder))
    }) {
        for (path, _) in &mut files {
            *path = path[folder.len() + 1..].to_string();
        }
    }
    let wanted = plain(name);
    let program = files
        .iter()
        .filter(|(path, bytes)| !path.contains('/') && is_program(path, bytes))
        .map(|(path, _)| path)
        .min_by_key(|path| {
            let stem = plain(
                path.rsplit_once('.')
                    .map_or(path.as_str(), |(stem, _)| stem),
            );
            (!wanted.contains(&stem), path.len(), path.to_string())
        })
        .ok_or(format!(
            "{name}: no program in it, a file that loads at $0801"
        ))?
        .clone();
    let has_autoboot = files
        .iter()
        .any(|(path, _)| path.eq_ignore_ascii_case("AUTOBOOT.X16"));
    if !has_autoboot {
        files.push(("AUTOBOOT.X16".to_string(), autoboot(&program)));
    }
    Ok(Game { files, program })
}

/// Whether the file at `path`, its `bytes`, is a program BASIC loads: at
/// `$0801`, named .PRG or with no ending.
pub fn is_program(path: &str, bytes: &[u8]) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let named = match name.rsplit_once('.') {
        Some((_, ending)) => ending.eq_ignore_ascii_case("prg"),
        None => true,
    };
    named && bytes.starts_with(&[0x01, 0x08])
}

/// AUTOBOOT.X16 to run `program`: `10 LOAD"PROGRAM"`, its name upper
/// case, as BASIC's PETSCII has letters.
pub fn autoboot(program: &str) -> Vec<u8> {
    const LOAD: u8 = 0x93;
    let mut line = vec![10, 0, LOAD, b'"'];
    line.extend(program.to_ascii_uppercase().bytes());
    line.extend([b'"', 0]);
    let next = 0x0801 + 2 + line.len() as u16;
    let mut bytes = vec![0x01, 0x08];
    bytes.extend(next.to_le_bytes());
    bytes.extend(line);
    bytes.extend([0, 0]);
    bytes
}
