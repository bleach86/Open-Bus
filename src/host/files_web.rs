//! `files.rs` in a web page, which has no disk: the files held in memory,
//! the same functions on them. The page puts in what it's given or has
//! kept (`put`), and takes what's been written, to keep it (`written`).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

thread_local! {
    static FILES: RefCell<BTreeMap<PathBuf, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    static WRITTEN: RefCell<BTreeSet<PathBuf>> = const { RefCell::new(BTreeSet::new()) };
}

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

pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    FILES.with_borrow(|files| files.get(path).cloned().ok_or_else(|| not_found(path)))
}

pub fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    put(path, bytes.to_vec());
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
    WRITTEN.with_borrow_mut(|written| written.insert(path.to_path_buf()));
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
