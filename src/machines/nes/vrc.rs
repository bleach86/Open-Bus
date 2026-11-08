//! Konami's VRCs: VRC2 and VRC4, mappers 21, 22, 23 and 25; VRC1, 75; and
//! VRC3, 73. As Mesen has them.
//!
//! Its registers are in four 4K blocks from $8000 up, four in each, which
//! of the four by two of the CPU's address lines, wired to the chip's A0 and
//! A1 as the board has them: 22, VRC2a, A1 and A0; 23, VRC2b, A0 and A1, or
//! VRC4e, A2 and A3; 21, VRC4a, A1 and A2, or VRC4c, A6 and A7; 25, VRC2c
//! and VRC4b, A1 and A0, or VRC4d, A3 and A2. NES 2.0's submapper says which,
//! or the database's; an old header doesn't, so then both of its number's
//! are taken at once, ORed, as every game of them works with.
//!
//! $8000, the 8K at $8000 (or at $C000, with VRC4's PRG swapped); $A000,
//! the 8K at $A000; the second-last fixed at $C000 (or $8000), the last at
//! $E000. $9000, the nametables, vertical, horizontal, or one screen or the
//! other (VRC2's, as it's known to be, only the first two); VRC4's $9002,
//! bit 1, its PRG swapped. $B000 to $E003, the eight 1K banks of CHR, two
//! registers each, its low four bits and its high five; VRC2a's the high
//! eight bits of those nine, a bank of half the number.
//!
//! A VRC2, known to be one, with no RAM: at $6000 up, a latch of one bit,
//! written there, and read back in bit 0, the rest open bus. Some games
//! (Ganbare Goemon 2, Gryzor) go no further if it isn't.
//!
//! VRC4's IRQ ($F000 to $F003): its latch's low and high nibbles; its
//! control, bit 1 enabling it, bit 2 counting CPU cycles, not lines (a line
//! every 341 dots, 113 2/3 cycles, by a prescaler), bit 0 its enabling
//! again when acknowledged; and $F003, acknowledging. Enabled, it counts up
//! from the latch, and going past $FF it's reloaded and the IRQ line held.
//!
//! VRC1's registers, one in each 4K block: $8000, $A000 and $C000, the 8K
//! banks there, the last fixed at $E000; $9000, bit 0 the nametables,
//! horizontal, not vertical, and bits 1 and 2 the high bits of its CHR's
//! two 4K banks, $E000 and $F000 their low four.
//!
//! VRC3's: a 16K bank at $8000 ($F000, bits 0 to 2), the last fixed, and
//! 8K of CHR RAM; its IRQ's 16 bits of latch, a nibble each at $8000 to
//! $B000; $C000 its control, as VRC4's, bit 2 counting in only its low 8
//! bits (and reloading only those); $D000 acknowledging. It counts CPU
//! cycles up, and going past the top it's reloaded and the IRQ line held.

use super::cartridge::Mirroring;
use serde::{Deserialize, Serialize};

/// How a board has the chip's A0 and A1 wired, and so which VRC it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wiring {
    Vrc2a,
    Vrc2b,
    Vrc2c,
    Vrc4a,
    Vrc4b,
    Vrc4c,
    Vrc4d,
    Vrc4e,
}

impl Wiring {
    /// Mapper `number`'s, its `submapper` saying which; and whether that's
    /// a guess, both of its number's taken.
    pub fn of(number: u16, submapper: u8) -> (Wiring, bool) {
        let wiring = match (number, submapper) {
            (21, 2) => Wiring::Vrc4c,
            (21, _) => Wiring::Vrc4a,
            (22, _) => Wiring::Vrc2a,
            (23, 2) => Wiring::Vrc4e,
            (23, _) => Wiring::Vrc2b,
            (25, 2) => Wiring::Vrc4d,
            (25, 3) => Wiring::Vrc2c,
            (25, _) => Wiring::Vrc4b,
            _ => unreachable!("mapper {number}'s not a VRC2 or 4"),
        };
        (wiring, submapper == 0 && number != 22)
    }

    /// Whether it's a VRC4, with its PRG swapped.
    pub fn vrc4(self) -> bool {
        !matches!(self, Wiring::Vrc2a | Wiring::Vrc2b | Wiring::Vrc2c)
    }

    /// Whether its IRQ counts: a VRC4's, or a guess's, which may be one.
    pub fn counts(self, guessed: bool) -> bool {
        self.vrc4() || guessed
    }

    /// `addr` as the chip sees it: its block's, and A1 and A0 as wired;
    /// `guessed`, both of its number's at once.
    fn register(self, addr: u16, guessed: bool) -> u16 {
        let line = |n: u16| (addr >> n) & 1;
        let (a0, a1) = match self {
            Wiring::Vrc2a | Wiring::Vrc2c | Wiring::Vrc4b if !guessed => (line(1), line(0)),
            Wiring::Vrc4d if !guessed => (line(3), line(2)),
            Wiring::Vrc2c | Wiring::Vrc4b | Wiring::Vrc4d => (line(1) | line(3), line(0) | line(2)),
            Wiring::Vrc4a if !guessed => (line(1), line(2)),
            Wiring::Vrc4c if !guessed => (line(6), line(7)),
            Wiring::Vrc4a | Wiring::Vrc4c => (line(1) | line(6), line(2) | line(7)),
            Wiring::Vrc2b if !guessed => (line(0), line(1)),
            Wiring::Vrc4e if !guessed => (line(2), line(3)),
            Wiring::Vrc2b | Wiring::Vrc4e | Wiring::Vrc2a => (line(0) | line(2), line(1) | line(3)),
        };
        (addr & 0xF000) | a1 << 1 | a0
    }
}

/// Its registers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vrc {
    prg: [u8; 2],
    swapped: bool,
    chr: [u16; 8],
    irq: Irq,
    /// A VRC2's latch at $6000 up.
    pub latch: u8,
}

/// VRC4's IRQ.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Irq {
    latch: u8,
    counter: u8,
    prescaler: i16,
    enabled: bool,
    enabled_on_acknowledging: bool,
    counting_cycles: bool,
    held: bool,
}

impl Vrc {
    /// A write to `addr`, $8000 up, as `wiring` has it, `guessed` or not:
    /// the nametables, if they're what's written.
    pub fn write(
        &mut self,
        addr: u16,
        value: u8,
        wiring: Wiring,
        guessed: bool,
    ) -> Option<Mirroring> {
        match wiring.register(addr, guessed) {
            0x8000..=0x8003 => self.prg[0] = value & 0x1F,
            register @ 0x9000..=0x9003 if !wiring.vrc4() || register <= 0x9001 => {
                // A VRC2's, known to be one, only vertical or horizontal.
                let mask = if guessed || wiring.vrc4() { 3 } else { 1 };
                return Some(
                    [
                        Mirroring::Vertical,
                        Mirroring::Horizontal,
                        Mirroring::SingleLower,
                        Mirroring::SingleUpper,
                    ][(value & mask) as usize],
                );
            }
            0x9002..=0x9003 => self.swapped = value & 0x02 != 0,
            0xA000..=0xA003 => self.prg[1] = value & 0x1F,
            register @ 0xB000..=0xE003 => {
                let bank = ((register as usize >> 12) - 0xB) * 2 + (register as usize >> 1 & 1);
                let chr = &mut self.chr[bank];
                *chr = if register & 1 == 0 {
                    (*chr & 0x1F0) | (value & 0x0F) as u16
                } else {
                    (*chr & 0x0F) | ((value & 0x1F) as u16) << 4
                };
            }
            0xF000 => self.irq.latch = (self.irq.latch & 0xF0) | (value & 0x0F),
            0xF001 => self.irq.latch = (self.irq.latch & 0x0F) | (value & 0x0F) << 4,
            0xF002 => {
                self.irq.enabled_on_acknowledging = value & 0x01 != 0;
                self.irq.enabled = value & 0x02 != 0;
                self.irq.counting_cycles = value & 0x04 != 0;
                if self.irq.enabled {
                    self.irq.counter = self.irq.latch;
                    self.irq.prescaler = 341;
                }
                self.irq.held = false;
            }
            0xF003 => {
                self.irq.enabled = self.irq.enabled_on_acknowledging;
                self.irq.held = false;
            }
            _ => {}
        }
        None
    }

    /// Which 8K of its PRG ROM `addr`, $8000 up, is in, of `banks`.
    pub fn prg_bank(&self, addr: u16, banks: usize) -> usize {
        let (second_last, last) = (banks - 2, banks - 1);
        match ((addr - 0x8000) / 0x2000, self.swapped) {
            (0, false) | (2, true) => self.prg[0] as usize,
            (0, true) | (2, false) => second_last,
            (1, _) => self.prg[1] as usize,
            _ => last,
        }
    }

    /// Which 1K of its CHR `addr` is in: VRC2a's, a bank of half the number.
    pub fn chr_bank(&self, addr: u16, wiring: Wiring) -> usize {
        let bank = self.chr[addr as usize >> 10 & 7] as usize;
        if wiring == Wiring::Vrc2a {
            bank >> 1
        } else {
            bank
        }
    }

    /// A CPU cycle: VRC4's IRQ counting, it or a line of them.
    pub fn cycle(&mut self) {
        let irq = &mut self.irq;
        if !irq.enabled {
            return;
        }
        irq.prescaler -= 3;
        if irq.counting_cycles || irq.prescaler <= 0 {
            if irq.counter == 0xFF {
                irq.counter = irq.latch;
                irq.held = true;
            } else {
                irq.counter += 1;
            }
            irq.prescaler += 341;
        }
    }

    /// Whether its IRQ line's held.
    pub fn irq(&self) -> bool {
        self.irq.held
    }
}

/// VRC1's registers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vrc1 {
    prg: [u8; 3],
    chr: [u8; 2],
}

impl Vrc1 {
    /// A write to `addr`, $8000 up: the nametables, if they're written.
    pub fn write(&mut self, addr: u16, value: u8) -> Option<Mirroring> {
        match addr & 0xF000 {
            0x8000 => self.prg[0] = value & 0x0F,
            0xA000 => self.prg[1] = value & 0x0F,
            0xC000 => self.prg[2] = value & 0x0F,
            0x9000 => {
                self.chr[0] = (self.chr[0] & 0x0F) | (value & 0x02) << 3;
                self.chr[1] = (self.chr[1] & 0x0F) | (value & 0x04) << 2;
                return Some(if value & 1 != 0 {
                    Mirroring::Horizontal
                } else {
                    Mirroring::Vertical
                });
            }
            0xE000 => self.chr[0] = (self.chr[0] & 0x10) | (value & 0x0F),
            0xF000 => self.chr[1] = (self.chr[1] & 0x10) | (value & 0x0F),
            _ => {}
        }
        None
    }

    /// Which 8K of its PRG ROM `addr`, $8000 up, is in, of `banks`.
    pub fn prg_bank(&self, addr: u16, banks: usize) -> usize {
        match (addr - 0x8000) / 0x2000 {
            3 => banks - 1,
            slot => self.prg[slot as usize] as usize,
        }
    }

    /// Which 4K of its CHR `addr` is in.
    pub fn chr_bank(&self, addr: u16) -> usize {
        self.chr[addr as usize >> 12 & 1] as usize
    }
}

/// VRC3's bank and its IRQ.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vrc3 {
    prg: u8,
    latch: u16,
    counter: u16,
    enabled: bool,
    enabled_on_acknowledging: bool,
    counting_8_bits: bool,
    held: bool,
}

impl Vrc3 {
    /// A write to `addr`, $8000 up.
    pub fn write(&mut self, addr: u16, value: u8) {
        let nibble = |at: u16| (self.latch & !(0x0F << at)) | ((value & 0x0F) as u16) << at;
        match addr & 0xF000 {
            0x8000 => self.latch = nibble(0),
            0x9000 => self.latch = nibble(4),
            0xA000 => self.latch = nibble(8),
            0xB000 => self.latch = nibble(12),
            0xC000 => {
                self.enabled_on_acknowledging = value & 0x01 != 0;
                self.enabled = value & 0x02 != 0;
                self.counting_8_bits = value & 0x04 != 0;
                if self.enabled {
                    self.counter = self.latch;
                }
                self.held = false;
            }
            0xD000 => {
                self.enabled = self.enabled_on_acknowledging;
                self.held = false;
            }
            0xF000 => self.prg = value & 0x07,
            _ => {}
        }
    }

    /// Its 16K bank at $8000.
    pub fn prg_bank(&self) -> usize {
        self.prg as usize
    }

    /// A CPU cycle: its IRQ counting, all 16 bits or the low 8.
    pub fn cycle(&mut self) {
        if !self.enabled {
            return;
        }
        let top = if self.counting_8_bits { 0x00FF } else { 0xFFFF };
        if self.counter & top == top {
            self.counter = (self.counter & !top) | (self.latch & top);
            self.held = true;
        } else {
            self.counter += 1;
        }
    }

    /// Whether its IRQ line's held.
    pub fn irq(&self) -> bool {
        self.held
    }
}
