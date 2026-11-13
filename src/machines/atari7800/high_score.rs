//! The High Score Cartridge, Atari's CX78HSC: plugged in between the console
//! and the game, which it passes through, it has 2K of RAM, kept by a
//! battery, at $1000 to $17FF, and its own 4K of ROM at $3000 to $3FFF, its
//! routines, which the games that know of it call to keep their high
//! scores in that RAM. As a7800 has it. One cartridge, the scores of every
//! game kept in it, so its RAM's not a save state's: it stays as it is,
//! whatever state's loaded.
//!
//! It's left out for the two games that write over its RAM without ever
//! calling it, as if it were none of theirs, Xenophobe and Scrapyard Dog:
//! by their ROMs' MD5s, as ProSystem's database knows them, and their
//! hacks'. An owner of one would have taken it out to play them.
//!
//! Its ROM is found as MAME names it, highscre.bin, beside the game, in
//! roms/atari7800/, or as the web page keeps it; its RAM's kept beside
//! that, highscre.srm (all $FF, as a7800 starts it, until there is one).

use crate::host::files;
use std::path::{Path, PathBuf};

/// Its ROM's name, and its size.
pub const ROM: &str = "highscre.bin";
const ROM_SIZE: usize = 0x1000;
const RAM_SIZE: usize = 0x800;

/// The ROMs it's left out for: Xenophobe's, NTSC's and PAL's; Scrapyard
/// Dog's, and its hacks'.
const WRITTEN_OVER_BY: [&str; 7] = [
    "05fb699db9eef564e2fe45c568746dbc",
    "70937c3184f0be33d06f7f4382ca54de",
    "980c35ae9625773a450aa7ef51751c04",
    "53db322c201323fe2ca8f074c0a2bf86",
    "0070751edb8bbf4dd4a685f58b5b72c5",
    "32f1a1b5a7e3b4493c3b7b637aeea7d8",
    "6e27f73bd4a49b647bcd58bc5f8b739d",
];

/// Whether it's left out for the game whose ROM's MD5 is `md5`.
pub fn left_out_for(md5: &str) -> bool {
    WRITTEN_OVER_BY.contains(&md5)
}

pub struct HighScore {
    rom: Vec<u8>,
    pub ram: Vec<u8>,
    /// Where its RAM's kept, and whether it's been written since it was.
    file: Option<PathBuf>,
    written: bool,
}

impl HighScore {
    /// One with `rom`, its RAM all $FF, not kept.
    pub fn new(rom: Vec<u8>) -> HighScore {
        HighScore {
            rom,
            ram: vec![0xFF; RAM_SIZE],
            file: None,
            written: false,
        }
    }

    /// One plugged in for the game at `path`, if its ROM's found, its RAM as
    /// it was kept.
    pub fn found(path: &Path) -> Option<HighScore> {
        let places = [
            path.with_file_name(ROM),
            Path::new("roms/atari7800").join(ROM),
            Path::new(ROM).to_path_buf(),
        ];
        let (place, rom) = places.into_iter().find_map(|place| {
            let rom = files::read(&place).ok()?;
            (rom.len() == ROM_SIZE).then_some((place, rom))
        })?;
        let mut high_score = HighScore::new(rom);
        let file = place.with_extension("srm");
        if let Ok(kept) = files::read(&file) {
            let len = kept.len().min(RAM_SIZE);
            high_score.ram[..len].copy_from_slice(&kept[..len]);
        }
        high_score.file = Some(file);
        Some(high_score)
    }

    /// Whether `addr` is its: its RAM's or its ROM's.
    pub fn has(addr: u16) -> bool {
        matches!(addr, 0x1000..=0x17FF | 0x3000..=0x3FFF)
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x1000..=0x17FF => self.ram[addr as usize - 0x1000],
            _ => self.rom[addr as usize - 0x3000],
        }
    }

    /// A write: its RAM's, its ROM's nothing.
    pub fn write(&mut self, addr: u16, value: u8) {
        if let 0x1000..=0x17FF = addr {
            self.ram[addr as usize - 0x1000] = value;
            self.written = true;
        }
    }

    /// Its RAM kept, if it's been written since it was.
    pub fn keep_saved(&mut self) {
        let Some(file) = &self.file else {
            return;
        };
        if std::mem::take(&mut self.written) {
            files::save_later(file, self.ram.clone());
        }
    }
}
