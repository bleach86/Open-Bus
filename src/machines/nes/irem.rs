//! Irem's mappers. G-101 (mapper 32): two 8K banks, the first of them at
//! $8000 or $C000, the last two fixed as the other's; eight 1K banks of
//! CHR; the mirroring, but on Major League's board, one screen only.
//!
//! H3001 (65): three 8K banks, the last fixed; eight 1K banks of CHR; the
//! mirroring; and an IRQ counting CPU cycles down from a latch, once.

use super::cartridge::Mirroring;
use serde::{Deserialize, Serialize};

/// G-101's registers: its PRG banks and their mode, its CHR banks; and
/// whether it's Major League's board, one screen, its PRG's mode fixed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct G101 {
    prg: [u8; 2],
    swapped: bool,
    chr: [u8; 8],
    major_league: bool,
}

impl G101 {
    /// Its submapper 1's is Major League's board.
    pub fn new(submapper: u8) -> Self {
        G101 {
            major_league: submapper == 1,
            ..G101::default()
        }
    }

    /// Its mirroring at first: Major League's one screen, or as the
    /// cartridge has it.
    pub fn mirroring(&self, soldered: Mirroring) -> Mirroring {
        if self.major_league {
            Mirroring::SingleLower
        } else {
            soldered
        }
    }

    /// A write to `addr`, $8000 up: the mirroring, if that's changed.
    pub fn write(&mut self, addr: u16, value: u8) -> Option<Mirroring> {
        match addr & 0xF000 {
            0x8000 => self.prg[0] = value & 0x1F,
            0x9000 if self.major_league => {}
            0x9000 => {
                self.swapped = value & 0x02 != 0;
                return Some(if value & 0x01 != 0 {
                    Mirroring::Horizontal
                } else {
                    Mirroring::Vertical
                });
            }
            0xA000 => self.prg[1] = value & 0x1F,
            0xB000 => self.chr[addr as usize & 7] = value,
            _ => {}
        }
        None
    }

    /// The 8K bank at `addr`, of `banks`.
    pub fn prg_bank(&self, addr: u16, banks: usize) -> usize {
        match (addr >> 13, self.swapped) {
            (4, false) | (6, true) => self.prg[0] as usize,
            (5, _) => self.prg[1] as usize,
            (4, true) | (6, false) => banks - 2,
            _ => banks - 1,
        }
    }

    /// The 1K bank CHR address `addr` is in.
    pub fn chr_bank(&self, addr: u16) -> usize {
        self.chr[addr as usize >> 10 & 7] as usize
    }
}

/// H3001's registers: its PRG and CHR banks; its IRQ's latch, counter,
/// enable and line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct H3001 {
    prg: [u8; 3],
    chr: [u8; 8],
    latch: u16,
    counter: u16,
    enabled: bool,
    irq: bool,
}

impl Default for H3001 {
    /// Its first two banks at first, and the one before the last.
    fn default() -> Self {
        H3001 {
            prg: [0, 1, 0xFE],
            chr: [0; 8],
            latch: 0,
            counter: 0,
            enabled: false,
            irq: false,
        }
    }
}

impl H3001 {
    /// A write to `addr`, $8000 up: the mirroring, if that's changed.
    pub fn write(&mut self, addr: u16, value: u8) -> Option<Mirroring> {
        match addr {
            0x8000 => self.prg[0] = value,
            0x9001 => {
                return Some(if value & 0x80 != 0 {
                    Mirroring::Horizontal
                } else {
                    Mirroring::Vertical
                });
            }
            0x9003 => {
                self.enabled = value & 0x80 != 0;
                self.irq = false;
            }
            0x9004 => {
                self.counter = self.latch;
                self.irq = false;
            }
            0x9005 => self.latch = (self.latch & 0x00FF) | (value as u16) << 8,
            0x9006 => self.latch = (self.latch & 0xFF00) | value as u16,
            0xA000 => self.prg[1] = value,
            0xB000..=0xB007 => self.chr[addr as usize & 7] = value,
            0xC000 => self.prg[2] = value,
            _ => {}
        }
        None
    }

    /// The 8K bank at `addr`, of `banks`: the last fixed.
    pub fn prg_bank(&self, addr: u16, banks: usize) -> usize {
        self.prg
            .get((addr as usize - 0x8000) >> 13)
            .map_or(banks - 1, |&bank| bank as usize)
    }

    /// The 1K bank CHR address `addr` is in.
    pub fn chr_bank(&self, addr: u16) -> usize {
        self.chr[addr as usize >> 10 & 7] as usize
    }

    /// A CPU cycle: its IRQ's count down, the IRQ at 0, and no more
    /// counting.
    pub fn cycle(&mut self) {
        if self.enabled {
            self.counter = self.counter.wrapping_sub(1);
            if self.counter == 0 {
                self.enabled = false;
                self.irq = true;
            }
        }
    }

    pub fn irq(&self) -> bool {
        self.irq
    }
}
