//! Jaleco's SS88006 (mapper 18): three 8K banks, the last fixed, and eight
//! 1K banks of CHR, each written a nibble at a time; the mirroring; and an
//! IRQ counting CPU cycles down, all 16 bits of it, or 12, 8 or 4, from a
//! latch written a nibble at a time too. Its ADPCM voice chip, which a few
//! games have for speech, isn't here.

use super::cartridge::Mirroring;
use serde::{Deserialize, Serialize};

/// The bits of the IRQ's counter that count, by its size.
const COUNTED: [u16; 4] = [0xFFFF, 0x0FFF, 0x00FF, 0x000F];

/// SS88006's registers: its PRG and CHR banks; its IRQ's latch, counter,
/// size, enable and line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ss88006 {
    prg: [u8; 3],
    chr: [u8; 8],
    latch: u16,
    counter: u16,
    size: u8,
    enabled: bool,
    irq: bool,
}

impl Ss88006 {
    /// A write to `addr`, $8000 up: the mirroring, if that's changed. Each
    /// bank's written a nibble at a time, the low at an even address, the
    /// high at the odd after it.
    pub fn write(&mut self, addr: u16, value: u8) -> Option<Mirroring> {
        let value = value & 0x0F;
        let nibble = |bank: &mut u8| {
            *bank = if addr & 1 != 0 {
                (*bank & 0x0F) | value << 4
            } else {
                (*bank & 0xF0) | value
            }
        };
        // Its registers, by address: two to a bank, at $8000 to $DFFF.
        let register = ((addr as usize >> 12) - 8) * 2 + (addr as usize >> 1 & 1);
        match addr & 0xF003 {
            0x8000..=0x8003 | 0x9000 | 0x9001 => nibble(&mut self.prg[register]),
            0xA000..=0xD003 => nibble(&mut self.chr[register - 4]),
            0xE000..=0xE003 => {
                let at = (addr & 3) * 4;
                self.latch = (self.latch & !(0x0F << at)) | (value as u16) << at;
            }
            0xF000 => {
                self.irq = false;
                self.counter = self.latch;
            }
            0xF001 => {
                self.irq = false;
                self.enabled = value & 0x01 != 0;
                self.size = match value {
                    _ if value & 0x08 != 0 => 3,
                    _ if value & 0x04 != 0 => 2,
                    _ if value & 0x02 != 0 => 1,
                    _ => 0,
                };
            }
            0xF002 => {
                return Some(match value & 3 {
                    0 => Mirroring::Horizontal,
                    1 => Mirroring::Vertical,
                    2 => Mirroring::SingleLower,
                    _ => Mirroring::SingleUpper,
                });
            }
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

    /// A CPU cycle: its counting bits a count down, its IRQ when they
    /// reach 0, the rest left as they are.
    pub fn cycle(&mut self) {
        if !self.enabled {
            return;
        }
        let counted = COUNTED[self.size as usize];
        let count = (self.counter & counted).wrapping_sub(1) & counted;
        self.counter = (self.counter & !counted) | count;
        self.irq |= count == 0;
    }

    pub fn irq(&self) -> bool {
        self.irq
    }
}
