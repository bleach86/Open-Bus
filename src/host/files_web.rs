//! `files.rs` in a web page, which has no disk: the files held in memory,
//! the same functions on them. The page puts in what it's given or has
//! kept (`put`), and takes what's been written, to keep it: whole, for a
//! file saved (`written`), or the parts written over (`changed`), a hard
//! disk's volume being tens of megabytes for a block's change.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

thread_local! {
    static FILES: RefCell<BTreeMap<PathBuf, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    static WRITTEN: RefCell<BTreeSet<PathBuf>> = const { RefCell::new(BTreeSet::new()) };
    static CHANGED: RefCell<BTreeMap<PathBuf, BTreeSet<u32>>> = const { RefCell::new(BTreeMap::new()) };
}

/// A part of a file, as it's told changed.
pub const PART: usize = 64 * 1024;

fn not_found(path: &Path) -> io::Error {
    io::Error::new(ErrorKind::NotFound, format!("{}: no such file", path.display()))
}

/// A file put in by the page.
pub fn put(path: &Path, bytes: Vec<u8>) {
    FILES.with_borrow_mut(|files| files.insert(path.to_path_buf(), bytes));
}

/// The files written since this was last asked, for the page to keep.
pub fn written() -> Vec<PathBuf> {
    WRITTEN.with_borrow_mut(|written| std::mem::take(written).into_iter().collect())
}

/// The files with parts written over since the page took them.
pub fn changed() -> Vec<PathBuf> {
    CHANGED.with_borrow(|changed| changed.keys().cloned().collect())
}

/// Where in the file its parts written over start, for the page to keep.
pub fn take_changed_parts(path: &Path) -> Vec<u32> {
    let parts = CHANGED.with_borrow_mut(|changed| changed.remove(path));
    parts.into_iter().flatten().collect()
}

/// The part of the file from `offset`: to the file's end, if that's nearer.
pub fn read_part(path: &Path, offset: u32) -> Option<Vec<u8>> {
    FILES.with_borrow(|files| {
        let file = files.get(path)?;
        let start = (offset as usize).min(file.len());
        Some(file[start..(start + PART).min(file.len())].to_vec())
    })
}

pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    FILES.with_borrow(|files| files.get(path).cloned().ok_or_else(|| not_found(path)))
}

pub fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    put(path, bytes.to_vec());
    CHANGED.with_borrow_mut(|changed| changed.remove(path));
    WRITTEN.with_borrow_mut(|written| written.insert(path.to_path_buf()));
    Ok(())
}

pub fn write_at(path: &Path, offset: u64, bytes: &[u8]) -> io::Result<()> {
    FILES.with_borrow_mut(|files| {
        let file = files.get_mut(path).ok_or_else(|| not_found(path))?;
        let start = offset as usize;
        if file.len() < start + bytes.len() {
            file.resize(start + bytes.len(), 0);
        }
        file[start..start + bytes.len()].copy_from_slice(bytes);
        Ok::<(), io::Error>(())
    })?;
    if WRITTEN.with_borrow(|written| written.contains(path)) {
        return Ok(()); // it's going whole
    }
    let parts = (offset as usize / PART)..(offset as usize + bytes.len()).div_ceil(PART);
    CHANGED.with_borrow_mut(|changed| {
        let file = changed.entry(path.to_path_buf()).or_default();
        file.extend(parts.map(|part| (part * PART) as u32));
    });
    Ok(())
}

pub fn exists(path: &Path) -> bool {
    FILES.with_borrow(|files| files.contains_key(path))
}

pub fn size(path: &Path) -> Option<u64> {
    FILES.with_borrow(|files| files.get(path).map(|file| file.len() as u64))
}

/// Nothing is: the page's files are all its own.
pub fn read_only(path: &Path) -> io::Result<bool> {
    if exists(path) { Ok(false) } else { Err(not_found(path)) }
}

pub fn is_folder(path: &Path) -> bool {
    FILES.with_borrow(|files| files.keys().any(|file| file.parent() == Some(path)))
}

pub fn list(path: &Path) -> io::Result<Vec<PathBuf>> {
    Ok(FILES.with_borrow(|files| {
        files.keys().filter(|file| file.parent() == Some(path)).cloned().collect()
    }))
}

pub fn canonical(path: &Path) -> PathBuf {
    path.to_path_buf()
}
