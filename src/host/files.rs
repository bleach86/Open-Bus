//! The files the machines read and keep: ROMs, disk images, volumes, save
//! states, a battery's RAM. All of it goes through here: on the host's
//! disk; a web build has `files_web.rs` in its place, the same functions
//! on files held in memory.

use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    std::fs::read(path)
}

/// `bytes` saved to `path`: to a file beside it, then put in its place, so
/// that it's never half saved.
pub fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut saving = path.as_os_str().to_owned();
    saving.push(".saving");
    std::fs::write(&saving, bytes)?;
    std::fs::rename(&saving, path)
}

/// `bytes` written over the file at `path`, from `offset`: a block of a
/// volume too big to save whole.
pub fn write_at(path: &Path, offset: u64, bytes: &[u8]) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(bytes)
}

pub fn exists(path: &Path) -> bool {
    path.exists()
}

/// Its size in bytes, if it's there.
pub fn size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|file| file.len())
}

/// Whether it can't be written.
pub fn read_only(path: &Path) -> io::Result<bool> {
    Ok(std::fs::metadata(path)?.permissions().readonly())
}

pub fn is_folder(path: &Path) -> bool {
    path.is_dir()
}

/// The files in the folder at `path`.
pub fn list(path: &Path) -> io::Result<Vec<PathBuf>> {
    std::fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect()
}

/// `path` as the same whatever it was reached by.
pub fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
