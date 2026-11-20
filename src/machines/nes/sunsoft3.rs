//! Sunsoft-3 (mapper 67): a 16K bank, the last fixed; four 2K banks of
//! CHR; the mirroring; and an IRQ counting CPU cycles down from what's
//! written to it, its high byte then its low, once past 0.

use super::cartridge::Mirroring;
use serde::{Deserialize, Serialize};

/// Sunsoft-3's registers: its PRG and CHR banks; its IRQ's counter, which
/// of its bytes is written next, its enable and line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sunsoft3 {
    prg: u8,
    chr: [u8; 4],
    counter: u16,
    low_next: bool,
    enabled: bool,
    irq: bool,
}

impl Sunsoft3 {
    /// A write to `addr`, $8000 up: the mirroring, if that's changed.
    pub fn write(&mut self, addr: u16, value: u8) -> Option<Mirroring> {
        match addr & 0xF800 {
            0x8800 | 0x9800 | 0xA800 | 0xB800 => {
                self.chr[(addr as usize >> 12) - 8] = value;
            }
            0xC800 => {
                self.counter = if self.low_next {
                    (self.counter & 0xFF00) | value as u16
                } else {
                    (self.counter & 0x00FF) | (value as u16) << 8
                };
                self.low_next = !self.low_next;
            }
            0xD800 => {
                self.enabled = value & 0x10 != 0;
                self.low_next = false;
                self.irq = false;
            }
            0xE800 => {
                return Some(match value & 3 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleLower,
                    _ => Mirroring::SingleUpper,
                });
            }
            0xF800 => self.prg = value,
            _ => {}
        }
        None
    }

    /// The 16K bank at $8000.
    pub fn prg_bank(&self) -> usize {
        self.prg as usize
    }

    /// The 2K bank CHR address `addr` is in.
    pub fn chr_bank(&self, addr: u16) -> usize {
        self.chr[addr as usize >> 11 & 3] as usize
    }

    /// A CPU cycle: its IRQ's count down, the IRQ once it's past 0, and no
    /// more counting.
    pub fn cycle(&mut self) {
        if self.enabled {
            self.counter = self.counter.wrapping_sub(1);
            if self.counter == 0xFFFF {
                self.enabled = false;
                self.irq = true;
            }
        }
    }

    pub fn irq(&self) -> bool {
        self.irq
    }
}
