//! The drive, `$FF40`: a disk drive with its own DOS, like a 1541. The
//! machine sends it a file name and a command, then reads or writes bytes.
//! On the host the disk is a directory; names stay inside it.

use std::fs;
use std::path::PathBuf;

const DRIVE_NAME: u16 = 0xFF40;
const DRIVE_CMD: u16 = 0xFF41;
const DRIVE_DATA: u16 = 0xFF42;

const OPEN_READ: u8 = 1;
const OPEN_WRITE: u8 = 2;
const OPEN_DIRECTORY: u8 = 3;
const CLOSE: u8 = 4;
const DELETE: u8 = 5;

const READABLE: u8 = 0x01;
const FAILED: u8 = 0x80;

enum File {
    None,
    Reading { data: Vec<u8>, at: usize },
    Writing { path: PathBuf, data: Vec<u8> },
}

pub struct Drive {
    directory: Option<PathBuf>,
    name: Vec<u8>,
    file: File,
    failed: bool,
}

impl Drive {
    /// A drive whose disk is `directory`; with none, every command fails.
    pub fn new(directory: Option<PathBuf>) -> Drive {
        Drive {
            directory,
            name: Vec::new(),
            file: File::None,
            failed: false,
        }
    }

    /// What a read gives, if reading takes nothing (DRIVE_DATA takes a byte).
    pub fn peek(&self, addr: u16) -> Option<u8> {
        (addr == DRIVE_CMD).then(|| {
            let readable = matches!(&self.file, File::Reading { data, at } if at < &data.len());
            (if readable { READABLE } else { 0 }) | if self.failed { FAILED } else { 0 }
        })
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            DRIVE_CMD => {
                let readable = matches!(&self.file, File::Reading { data, at } if at < &data.len());
                (if readable { READABLE } else { 0 }) | if self.failed { FAILED } else { 0 }
            }
            DRIVE_DATA => match &mut self.file {
                File::Reading { data, at } if *at < data.len() => {
                    *at += 1;
                    data[*at - 1]
                }
                _ => 0,
            },
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            DRIVE_NAME => self.name.push(value),
            DRIVE_CMD => {
                let name = std::mem::take(&mut self.name);
                self.failed = self.command(value, &name).is_none();
            }
            DRIVE_DATA => {
                if let File::Writing { data, .. } = &mut self.file {
                    data.push(value);
                }
            }
            _ => {}
        }
    }

    /// Carry out `command` on the file `name`; `None` if it failed.
    fn command(&mut self, command: u8, name: &[u8]) -> Option<()> {
        match command {
            OPEN_READ => {
                let data = fs::read(self.path(name)?).ok()?;
                self.file = File::Reading { data, at: 0 };
            }
            OPEN_WRITE => {
                let path = self.path(name)?;
                self.file = File::Writing { path, data: Vec::new() };
            }
            OPEN_DIRECTORY => {
                let mut names: Vec<String> = fs::read_dir(self.directory.as_ref()?)
                    .ok()?
                    .flatten()
                    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .filter(|name| !name.starts_with('.'))
                    .collect();
                names.sort();
                let data = names.iter().flat_map(|name| name.bytes().chain([0])).collect();
                self.file = File::Reading { data, at: 0 };
            }
            CLOSE => {
                if let File::Writing { path, data } = std::mem::replace(&mut self.file, File::None) {
                    // Replaced whole, so losing power part way leaves the old file.
                    let partial = path.with_extension("new");
                    fs::write(&partial, data).and_then(|()| fs::rename(&partial, &path)).ok()?;
                }
                self.file = File::None;
            }
            DELETE => fs::remove_file(self.path(name)?).ok()?,
            _ => return None,
        }
        Some(())
    }

    /// The host path of `name`, if it is a plain file name.
    fn path(&self, name: &[u8]) -> Option<PathBuf> {
        let name = std::str::from_utf8(name).ok()?;
        let plain = !name.is_empty() && !name.contains('/') && name != "." && name != "..";
        plain.then(|| self.directory.as_ref().map(|directory| directory.join(name)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(drive: &mut Drive, command: u8, name: &str) -> u8 {
        for byte in name.bytes() {
            drive.write(DRIVE_NAME, byte);
        }
        drive.write(DRIVE_CMD, command);
        drive.read(DRIVE_CMD)
    }

    fn read_all(drive: &mut Drive) -> Vec<u8> {
        let mut data = Vec::new();
        while drive.read(DRIVE_CMD) & READABLE != 0 {
            data.push(drive.read(DRIVE_DATA));
        }
        data
    }

    #[test]
    fn writes_reads_lists_and_deletes_files() {
        let directory = std::path::Path::new(env!("OUT_DIR")).join("test-drive");
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(".hidden"), "x").unwrap();
        let mut drive = Drive::new(Some(directory.clone()));

        assert_eq!(command(&mut drive, OPEN_READ, "missing"), FAILED);
        assert_eq!(command(&mut drive, OPEN_WRITE, "b.txt"), 0);
        for byte in *b"hi" {
            drive.write(DRIVE_DATA, byte);
        }
        assert_eq!(command(&mut drive, CLOSE, ""), 0);
        command(&mut drive, OPEN_WRITE, "a.crt");
        command(&mut drive, CLOSE, "");

        assert_eq!(command(&mut drive, OPEN_READ, "b.txt"), READABLE);
        assert_eq!(read_all(&mut drive), b"hi");
        assert_eq!(drive.read(DRIVE_DATA), 0, "past the end");
        command(&mut drive, OPEN_DIRECTORY, "");
        assert_eq!(read_all(&mut drive), b"a.crt\0b.txt\0", "sorted, hidden files left out");

        assert_eq!(command(&mut drive, DELETE, "b.txt"), 0);
        assert_eq!(command(&mut drive, OPEN_READ, "b.txt"), FAILED);
        assert_eq!(command(&mut drive, OPEN_READ, "../test-drive/a.crt"), FAILED, "stays on the disk");
    }
}
