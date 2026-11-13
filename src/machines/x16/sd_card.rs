//! The SD card in VERA's SPI port, as x16emu (BSD-2) has it: an SDHC
//! card in SPI mode, its blocks 512 bytes, addressed by number, from an
//! image of it, a FAT32 one, say, partitioned as the X16's are.
//!
//! A command's 6 bytes, its first `01` and its number, then its argument
//! and a CRC (not checked); its response read out with $FFs sent, each
//! giving its next byte, $FF once it's given. It goes idle (CMD0), says
//! its voltage (CMD8), is started (CMD55 then ACMD41), gives its CSD
//! (CMD9: its size), its status (CMD13) and its OCR (CMD58). It reads a
//! block (CMD17), a data token then its bytes and a CRC; or blocks on from
//! one till it's stopped (CMD18, CMD12). It writes a block (CMD24), the
//! block sent after a data token, written to its image as it's written; a
//! read only image's are lost. A block past its end reads as an error
//! token. The rest just say it's not idle. As x16emu's, it doesn't answer
//! a block written, and it's never busy.

use crate::host::files;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

const BLOCK: usize = 512;

/// The commands, an application's (after CMD55) with bit 7 set.
const GO_IDLE: u8 = 0;
const SEND_IF_COND: u8 = 8;
const SEND_CSD: u8 = 9;
const STOP: u8 = 12;
const SEND_STATUS: u8 = 13;
const READ_BLOCK: u8 = 17;
const READ_BLOCKS: u8 = 18;
const WRITE_BLOCK: u8 = 24;
const APP: u8 = 55;
const READ_OCR: u8 = 58;
const SEND_OP_COND: u8 = 0x80 | 41;

/// A block's data token; and the error token for one out of range.
const DATA: u8 = 0xFE;
const OUT_OF_RANGE: u8 = 0x08;

/// Its image: where it is, its size, and whether it can't be written.
#[derive(Clone, Debug)]
pub struct Image {
    pub path: PathBuf,
    size: u64,
    read_only: bool,
}

impl Image {
    /// The image at `path`: whole blocks, and 512K at least, its CSD
    /// giving its size in those.
    pub fn open(path: &Path) -> Result<Image, String> {
        let shown = path.display();
        let size = files::size(path).ok_or_else(|| format!("{shown}: no such file"))?;
        if size % BLOCK as u64 != 0 || size < 0x80000 {
            return Err(format!(
                "{shown}: {size} bytes: not a card's, of 512-byte blocks, 512K at least"
            ));
        }
        let read_only = files::read_only(path).map_err(|e| format!("{shown}: {e}"))?;
        Ok(Image {
            path: path.to_path_buf(),
            size,
            read_only,
        })
    }
}

/// Its image isn't in a save state, but where it's at with it is.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SdCard {
    #[serde(skip)]
    pub image: Option<Image>,
    selected: bool,
    /// A command, or a block to write, coming in.
    received: Vec<u8>,
    /// What it's to give, as $FFs are sent.
    response: VecDeque<u8>,
    /// The block read or written; the last command; whether the next's an
    /// application's.
    block: u32,
    last: u8,
    app: bool,
    idle: bool,
    initialized: bool,
    /// Reading blocks on, as CMD18 has it.
    reading_on: bool,
    /// A block read or written, since it was last asked.
    #[serde(skip)]
    pub used: bool,
}

impl SdCard {
    pub fn new() -> SdCard {
        SdCard {
            image: None,
            selected: false,
            received: Vec::new(),
            response: VecDeque::new(),
            block: 0,
            last: 0,
            app: false,
            idle: true,
            initialized: false,
            reading_on: false,
            used: false,
        }
    }

    /// Its chip select, from high to low: a command from its start. As
    /// x16emu's, it's never let go: unselected, it's sent nothing.
    pub fn select(&mut self) {
        self.selected = true;
        self.received.clear();
    }

    /// A byte sent; what it sends back at once.
    pub fn exchange(&mut self, byte: u8) -> u8 {
        if !self.selected || self.image.is_none() {
            return 0xFF;
        }
        if self.received.is_empty() && byte == 0xFF {
            let Some(out) = self.response.pop_front() else {
                return 0xFF;
            };
            if self.response.is_empty() && self.reading_on {
                self.block = self.block.wrapping_add(1);
                self.response = self.read();
                self.reading_on = self.response.len() > 1;
            }
            return out;
        }
        self.received.push(byte);
        if self.received[0] & 0xC0 == 0x40 && self.received.len() == 6 {
            self.command();
        } else if self.received.len() == 1 + BLOCK + 2 {
            self.write();
        }
        0xFF
    }

    /// The command received, and its response.
    fn command(&mut self) {
        let received = std::mem::take(&mut self.received);
        let mut command = received[0] & 0x3F;
        if std::mem::take(&mut self.app) {
            command |= 0x80;
        }
        self.last = command;
        let argument = u32::from_be_bytes(received[1..5].try_into().unwrap());
        let r1 = VecDeque::from([self.idle as u8]);
        self.response = match command {
            GO_IDLE => {
                self.idle = true;
                VecDeque::from([1])
            }
            SEND_IF_COND => VecDeque::from([0x01, 0x00, 0x00, 0x01, 0xAA]),
            SEND_CSD => self.csd(),
            SEND_OP_COND => {
                (self.idle, self.initialized) = (false, true);
                VecDeque::from([0])
            }
            STOP => {
                self.reading_on = false;
                r1
            }
            SEND_STATUS if self.initialized => VecDeque::from([0x00, 0x00]),
            SEND_STATUS => VecDeque::from([0x1F, 0xFF]),
            READ_BLOCK | READ_BLOCKS => {
                self.block = argument;
                let mut response = self.read();
                self.reading_on = command == READ_BLOCKS && response.len() > 1;
                response.push_front(0x00);
                response
            }
            WRITE_BLOCK => {
                self.block = argument;
                r1
            }
            APP => {
                self.app = true;
                r1
            }
            READ_OCR => VecDeque::from([0xC0, 0xFF, 0x80, 0x00]),
            _ => r1,
        };
    }

    fn image(&self) -> &Image {
        self.image.as_ref().expect("a card")
    }

    /// Where its block is in its image, if it's in it.
    fn at(&self) -> Option<u64> {
        let at = self.block as u64 * BLOCK as u64;
        (at < self.image().size).then_some(at)
    }

    /// Its block: a data token, its bytes and a CRC; or an error token.
    fn read(&mut self) -> VecDeque<u8> {
        let Some(at) = self.at() else {
            return VecDeque::from([OUT_OF_RANGE]);
        };
        self.used = true;
        let path = &self.image().path;
        let mut bytes = files::read_at(path, at, BLOCK).unwrap_or_else(|e| {
            eprintln!("{}: block {} not read: {e}", path.display(), self.block);
            Vec::new()
        });
        bytes.resize(BLOCK, 0);
        let mut response = VecDeque::from(bytes);
        response.push_front(DATA);
        response.extend([0, 0]);
        response
    }

    /// The block received, after CMD24 and its data token, written.
    fn write(&mut self) {
        let received = std::mem::take(&mut self.received);
        if self.last != WRITE_BLOCK || received[0] != DATA {
            return;
        }
        let Some(at) = self.at() else { return };
        self.used = true;
        let image = self.image();
        let shown = image.path.display();
        if image.read_only {
            eprintln!("{shown}: read only: block {} not written", self.block);
        } else if let Err(e) = files::write_at(&image.path, at, &received[1..1 + BLOCK]) {
            eprintln!("{shown}: block {} not written: {e}", self.block);
        }
    }

    /// Its CSD, version 2's, an SDHC card's: its size in 512K, less one.
    fn csd(&self) -> VecDeque<u8> {
        let size = (self.image().size >> 19) - 1;
        #[rustfmt::skip]
        let csd = [
            0xFF, 0xFF, 0x00, 0xFF, DATA, // R1 between filler, the token
            0x40, 0x0E, 0x00, 0x32, 0x5B, 0x59, 0x00,
            (size >> 16) as u8 & 0x3F, (size >> 8) as u8, size as u8,
            0x7F, 0x80, 0x0A, 0x40, 0x00, 0x01,
        ];
        VecDeque::from(csd)
    }
}

impl Default for SdCard {
    fn default() -> SdCard {
        SdCard::new()
    }
}
