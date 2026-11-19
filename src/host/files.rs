//! The files the machines read and keep: ROMs, disk images, volumes, save
//! states, a battery's RAM. All of it goes through here: on the host's
//! disk; a web build has `files_web.rs` in its place, the same functions
//! on files held in memory.

use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};

pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    std::fs::read(path)
}

/// `bytes` saved to `path`: to a file beside it, then put in its place, so
/// that it's never half saved.
pub fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // A save's folder made, as one kept apart has its own.
    if let Some(folder) = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
    {
        std::fs::create_dir_all(folder)?;
    }
    let mut saving = path.as_os_str().to_owned();
    saving.push(".saving");
    std::fs::write(&saving, bytes)?;
    std::fs::rename(&saving, path)
}

/// Saves made on a thread of their own, not the machine's, so that a slow
/// disk (a network share's) doesn't hold up its frames: each file's
/// latest bytes, to be saved; whether one's being saved; and whether the
/// thread's been started.
struct Saving {
    due: Vec<(PathBuf, Vec<u8>)>,
    saving: bool,
    started: bool,
}

static SAVING: Mutex<Saving> = Mutex::new(Saving {
    due: Vec::new(),
    saving: false,
    started: false,
});

/// Told when there's more to save, or a save's done.
static SAVES: Condvar = Condvar::new();

/// `bytes` saved to `path` soon, on the saving thread: over what's still to
/// be saved there, if there is. Any that won't save are told of.
pub fn save_later(path: &Path, bytes: Vec<u8>) {
    let mut saving = SAVING.lock().unwrap();
    match saving.due.iter_mut().find(|(due, _)| due == path) {
        Some((_, due)) => *due = bytes,
        None => saving.due.push((path.to_path_buf(), bytes)),
    }
    if !saving.started {
        saving.started = true;
        std::thread::spawn(keep_saving);
    }
    SAVES.notify_all();
}

/// The saving thread: each file, in the order they were asked for.
fn keep_saving() {
    let mut saving = SAVING.lock().unwrap();
    loop {
        if saving.due.is_empty() {
            saving.saving = false;
            SAVES.notify_all();
            saving = SAVES.wait(saving).unwrap();
            continue;
        }
        let (path, bytes) = saving.due.remove(0);
        saving.saving = true;
        drop(saving);
        if let Err(e) = save(&path, &bytes) {
            eprintln!("{}: not saved: {e}", path.display());
        }
        saving = SAVING.lock().unwrap();
    }
}

/// Once all that's been asked to be saved later has been.
pub fn all_saved() {
    let mut saving = SAVING.lock().unwrap();
    while saving.saving || !saving.due.is_empty() {
        saving = SAVES.wait(saving).unwrap();
    }
}

/// Up to `length` bytes of the file at `path`, from `offset`, fewer past
/// its end: a block of a volume too big to read whole.
pub fn read_at(path: &Path, offset: u64, length: usize) -> io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::with_capacity(length);
    file.take(length as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
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
