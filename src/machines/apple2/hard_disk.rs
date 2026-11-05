//! A hard disk card, in slot 7: a ProDOS block device of two volumes, up
//! to 32MB each, from .hdv, .po or .2mg images. Its firmware, our own
//! (roms/hard_disk.s, built in here), boots drive 1's volume and is
//! ProDOS's driver for both. It has the card's registers do the work: the
//! command, the unit and the block written, then done; a block's 512
//! bytes passed through the data register, a byte at a time, as a real
//! card's are. The register steps on through the block only as the
//! command has it, read while reading, written while writing: a store to
//! it reads it first, as the 6502's STA abs,X does, and that mustn't. A volume's blocks written are written to its image as they
//! are: a read only image is write protected.

use crate::host::files;
use crate::host::save_state;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The firmware, assembled (`make -C roms`) and committed.
pub const FIRMWARE: &[u8; 256] = include_bytes!("../../../roms/hard_disk.bin");

const BLOCK: usize = 512;

/// ProDOS's errors: an I/O error (a block not on the volume), no device
/// connected, write protected.
const IO_ERROR: u8 = 0x27;
const NO_DEVICE: u8 = 0x28;
const WRITE_PROTECTED: u8 = 0x2B;

/// A volume: its blocks; the image they're from, and where in it they
/// start; whether it's write protected.
pub struct Volume {
    pub blocks: Vec<u8>,
    pub path: PathBuf,
    offset: u64,
    pub write_protected: bool,
}

impl Volume {
    /// The volume in the image at `path`: a .2mg's, after its header, in
    /// ProDOS's order; or the whole of a .hdv's or .po's. Write protected
    /// if the .2mg says so or the file is read only.
    pub fn open(path: &Path) -> Result<Volume, String> {
        let image = files::read(path).map_err(|e| e.to_string())?;
        let read_only = files::read_only(path).map_err(|e| e.to_string())?;
        let (offset, length, protected) = if image.starts_with(b"2IMG") {
            let word = |at: usize| u32::from_le_bytes(image[at..at + 4].try_into().unwrap());
            if image.len() < 64 || word(0x0C) != 1 {
                return Err("a .2mg not of ProDOS's order".to_string());
            }
            (word(0x18) as usize, word(0x1C) as usize, word(0x10) & 0x8000_0000 != 0)
        } else {
            (0, image.len(), false)
        };
        let blocks = image.get(offset..offset + length).ok_or("its blocks past its end")?.to_vec();
        if blocks.is_empty() || blocks.len() % BLOCK != 0 || blocks.len() > 0xFFFF * BLOCK {
            return Err(format!("{} bytes: not a volume of 512-byte blocks, up to 65,535", blocks.len()));
        }
        Ok(Volume { blocks, path: path.to_path_buf(), offset: offset as u64, write_protected: protected || read_only })
    }

    fn count(&self) -> u16 {
        (self.blocks.len() / BLOCK) as u16
    }
}

/// Its volumes aren't in a save state, but where it's at with them: the
/// machine it's loaded into has them, the same.
#[derive(Serialize, Deserialize)]
pub struct HardDisk {
    /// Drives 1 and 2's volumes.
    #[serde(skip)]
    pub units: [Option<Volume>; 2],
    // The registers: the command, the unit, the block; the last
    // command's error.
    command: u8,
    unit: u8,
    block: u16,
    error: u8,
    /// A block, read or to be written, and the data register's place in it.
    #[serde(with = "save_state::bytes")]
    buffer: [u8; BLOCK],
    at: usize,
}

impl HardDisk {
    pub fn new() -> Self {
        HardDisk { units: [None, None], command: 0, unit: 0, block: 0, error: 0, buffer: [0; BLOCK], at: 0 }
    }

    /// The firmware, at $Cn00 to $CnFF.
    pub fn rom(&self, addr: u16) -> u8 {
        FIRMWARE[(addr & 0xFF) as usize]
    }

    /// The volume of the unit set: drive 1's, or with bit 7, drive 2's.
    fn volume(&self) -> Option<&Volume> {
        self.units[(self.unit >> 7) as usize].as_ref()
    }

    /// A register, $C0n0 to $C0nF, read or written (`written`, the byte):
    /// what a read gives; at an address it hasn't, nothing.
    pub fn io(&mut self, addr: u16, written: Option<u8>) -> Option<u8> {
        let register = addr & 0x0F;
        match (register, written) {
            (0, Some(value)) => {
                self.command = value;
                self.at = 0;
            }
            (1, Some(value)) => self.unit = value,
            (2, Some(value)) => self.block = (self.block & 0xFF00) | value as u16,
            (3, Some(value)) => self.block = (self.block & 0x00FF) | (value as u16) << 8,
            (4, Some(_)) => self.execute(),
            (4, None) => return Some(self.error),
            (5, None) => return Some(self.volume().map_or(0, Volume::count) as u8),
            (6, None) => return Some((self.volume().map_or(0, Volume::count) >> 8) as u8),
            (8, Some(value)) => {
                if self.command == 2 {
                    self.buffer[self.at] = value;
                    self.at = (self.at + 1) % BLOCK;
                }
            }
            (8, None) => {
                let value = self.buffer[self.at];
                if self.command == 1 {
                    self.at = (self.at + 1) % BLOCK;
                }
                return Some(value);
            }
            _ => {}
        }
        None
    }

    /// The command done: STATUS, whether there's a volume; READ, its block
    /// into the buffer; WRITE, the buffer to its block, and its image.
    fn execute(&mut self) {
        self.at = 0;
        let (command, block) = (self.command, self.block as usize);
        let drive = (self.unit >> 7) as usize;
        let Some(volume) = &mut self.units[drive] else {
            self.error = NO_DEVICE;
            return;
        };
        let at = block * BLOCK;
        self.error = match command {
            0 => 0,
            _ if at >= volume.blocks.len() => IO_ERROR,
            1 => {
                self.buffer.copy_from_slice(&volume.blocks[at..at + BLOCK]);
                0
            }
            2 if volume.write_protected => WRITE_PROTECTED,
            2 => {
                volume.blocks[at..at + BLOCK].copy_from_slice(&self.buffer);
                let written = files::write_at(&volume.path, volume.offset + at as u64, &self.buffer);
                match written {
                    Ok(()) => 0,
                    Err(e) => {
                        eprintln!("{}: block {block} not saved: {e}", volume.path.display());
                        IO_ERROR
                    }
                }
            }
            _ => IO_ERROR,
        };
    }
}
