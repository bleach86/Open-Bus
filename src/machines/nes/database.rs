//! Games known by their ROMs. An iNES header says too little of a board,
//! and an old one often says it wrong; a game found here is taken as the
//! database has it, as Mesen does: its mapper and submapper, its ROMs' and
//! RAMs' sizes, its mirroring. Its battery only ever adds one, that no
//! game's saves are lost to it; and its region's left to the header and
//! the game's name, its NTSC often only a default. The database is
//! Mesen's, MesenNesDB.txt beside this (GPL-3, as Open Bus is), itself made
//! from NewRisingSun's NES 2.0 header database, NesCartDB and Nestopia's.
//!
//! A line: the CRC-32 of the image after its header and trainer; the
//! system; board, PCB and chip names; the mapper; PRG ROM, CHR ROM, CHR
//! RAM, work RAM and save RAM in K; a battery, 0 or 1; the mirroring (h,
//! v, 4 or 1, one screen); the controller, bus conflicts, the submapper,
//! and a Vs. System's type and PPU. A game with a submapper given has been
//! checked: its RAMs are as it says, even none; one without only adds
//! what it has to the header's.

use super::cartridge::Mirroring;
use std::collections::HashMap;
use std::sync::OnceLock;

const DATABASE: &str = include_str!("MesenNesDB.txt");

/// A board as the database has it.
#[derive(Debug, PartialEq)]
pub struct Known {
    pub mapper: u16,
    pub submapper: Option<u8>,
    pub prg_rom: usize,
    pub chr_rom: usize,
    pub chr_ram: usize,
    /// Its work and save RAM together, as the cartridge has them.
    pub prg_ram: usize,
    pub battery: bool,
    pub mirroring: Option<Mirroring>,
    /// Whether it's a Famicom's game, the system it says.
    pub famicom: bool,
}

impl Known {
    /// Whether it's been checked, its RAMs to be taken as they are, none
    /// included.
    pub fn checked(&self) -> bool {
        self.submapper.is_some()
    }
}

/// The game whose ROMs, all of an image after its header and trainer, have
/// the CRC-32 `crc`, if it's known.
pub fn find(crc: u32) -> Option<&'static Known> {
    static GAMES: OnceLock<HashMap<u32, Known>> = OnceLock::new();
    GAMES
        .get_or_init(|| DATABASE.lines().filter_map(game).collect())
        .get(&crc)
}

/// A line's game, unless it's a comment, or a board known only by its UNIF
/// name, which none of the mappers here are.
pub(super) fn game(line: &str) -> Option<(u32, Known)> {
    if line.starts_with('#') {
        return None;
    }
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() < 16 {
        return None;
    }
    let k = |at: usize| fields[at].parse::<usize>().unwrap_or(0) * 1024;
    let crc = u32::from_str_radix(fields[0], 16).ok()?;
    let mapper = fields[5].parse().ok().filter(|&mapper| mapper < 65000)?;
    let mirroring = match fields[12] {
        "h" => Some(Mirroring::Horizontal),
        "v" => Some(Mirroring::Vertical),
        "4" => Some(Mirroring::FourScreen),
        "0" => Some(Mirroring::SingleLower),
        "1" => Some(Mirroring::SingleUpper),
        _ => None,
    };
    let known = Known {
        mapper,
        submapper: fields[15].parse().ok(),
        prg_rom: k(6),
        chr_rom: k(7),
        chr_ram: k(8),
        prg_ram: k(9) + k(10),
        battery: fields[11] == "1",
        mirroring,
        famicom: fields[1] == "Famicom",
    };
    Some((crc, known))
}
