//! Taito's mappers. TC0190 (mapper 33): two 8K banks, the last two fixed,
//! two 2K banks of CHR and four 1K. TC0690 (48), the same with an MMC3's
//! scanline counter, its IRQ coming a little later than an MMC3's.
//!
//! X1-005 (80): its registers at $7EF0 up, in with its 128 bytes of RAM
//! at $7F00, mirrored once, there only while $A3 is written to its key;
//! three 8K banks, the last fixed; two 2K banks of CHR and four 1K. On
//! mapper 207's board, each 2K bank's top bit picks the nametable for
//! that half of them, as the CHR does.
//!
//! X1-017 (82): its registers there too; 5K of RAM at $6000, in three
//! parts, each with its own key; its 2K and 1K CHR banks either way
//! round.

use super::cartridge::{Mirroring, Scanlines};
use serde::{Deserialize, Serialize};

/// TC0190's and TC0690's registers: their PRG and CHR banks; TC0690's
/// scanline counter, and the cycles its IRQ's still held back once it's
/// due; and whether it's submapper 1's, The Jetsons'.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tc0190 {
    tc0690: bool,
    prg: [u8; 2],
    chr: [u8; 6],
    lines: Scanlines,
    delay: u8,
    jetsons: bool,
}

impl Tc0190 {
    /// TC0190, or TC0690 if `tc0690`; its submapper 1 (The Jetsons) has
    /// its IRQ a line on, and held back less.
    pub fn new(tc0690: bool, submapper: u8) -> Self {
        Tc0190 {
            tc0690,
            jetsons: submapper == 1,
            ..Tc0190::default()
        }
    }

    /// Its mapper's number and name.
    pub fn name(&self) -> (u16, &'static str) {
        if self.tc0690 {
            (48, "TAITO TC0690")
        } else {
            (33, "TAITO TC0190")
        }
    }

    /// A write to `addr`, $8000 up: the mirroring, if that's changed.
    pub fn write(&mut self, addr: u16, value: u8) -> Option<Mirroring> {
        let horizontal = |value: u8| {
            Some(if value & 0x40 != 0 {
                Mirroring::Horizontal
            } else {
                Mirroring::Vertical
            })
        };
        let mask = if self.tc0690 { 0xE003 } else { 0xA003 };
        match addr & mask {
            0x8000 => {
                self.prg[0] = value & 0x3F;
                if !self.tc0690 {
                    return horizontal(value);
                }
            }
            0x8001 => self.prg[1] = value & 0x3F,
            0x8002 | 0x8003 => self.chr[addr as usize & 1] = value,
            0xA000..=0xA003 => self.chr[2 + (addr as usize & 3)] = value,
            // Either of the first two lets go of the IRQ, as The
            // Flintstones has it.
            0xC000 => {
                self.lines.irq = false;
                self.lines.latch = (value ^ 0xFF).wrapping_add(self.jetsons as u8);
            }
            0xC001 => {
                self.lines.irq = false;
                self.lines.restart();
            }
            0xC002 => self.lines.enable(true),
            0xC003 => self.lines.enable(false),
            0xE000 => return horizontal(value),
            _ => {}
        }
        None
    }

    /// The 8K bank at `addr`, of `banks`.
    pub fn prg_bank(&self, addr: u16, banks: usize) -> usize {
        match addr {
            0x8000..=0x9FFF => self.prg[0] as usize,
            0xA000..=0xBFFF => self.prg[1] as usize,
            0xC000..=0xDFFF => banks - 2,
            _ => banks - 1,
        }
    }

    /// The 1K bank CHR address `addr` is in.
    pub fn chr_bank(&self, addr: u16) -> usize {
        let page = addr as usize >> 10 & 7;
        match page {
            0..=3 => self.chr[page >> 1] as usize * 2 + (page & 1),
            _ => self.chr[page - 2] as usize,
        }
    }

    /// Whether it watches the PPU's A12: TC0690 does.
    pub fn counts_lines(&self) -> bool {
        self.tc0690
    }

    /// A12 on the PPU's bus: a line counted, its IRQ held back once it's
    /// due.
    pub fn a12(&mut self, addr: u16, dot: u64) {
        // "About a 4 CPU cycle delay from the normal MMC3 IRQ time", as
        // NESdev has it; as Mesen holds it back, for The Flintstones and
        // Captain Saver to be still, and The Jetsons.
        if self.lines.a12(addr, dot) {
            self.delay = if self.jetsons { 6 } else { 22 };
        }
    }

    /// A CPU cycle: the IRQ held back a cycle less.
    pub fn cycle(&mut self) {
        if self.delay > 0 {
            self.delay -= 1;
            self.lines.irq |= self.delay == 0;
        }
    }

    pub fn irq(&self) -> bool {
        self.lines.irq
    }
}

/// The X1's 128 bytes of RAM, at $7F00, mirrored once; and its key.
const X1005_RAM: usize = 0x80;
const X1005_KEY: u8 = 0xA3;
/// X1-017's RAM, in parts of 2K, 2K and 1K, and their keys.
pub const X1017_RAM: usize = 0x1400;
const X1017_KEYS: [u8; 3] = [0xCA, 0x69, 0x84];

/// X1-005's and X1-017's registers: their PRG banks; CHR banks, two 2K
/// and four 1K; the keys written to their RAM; X1-017's CHR the other way
/// round; whether it's an X1-017, and whether it's mapper 207's board.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct X1 {
    x1017: bool,
    tables_by_chr: bool,
    prg: [u8; 3],
    chr: [u8; 6],
    keys: [u8; 3],
    chr_swapped: bool,
}

impl X1 {
    /// X1-005, on mapper 207's board if `tables_by_chr`; or X1-017.
    pub fn new(x1017: bool, tables_by_chr: bool) -> Self {
        X1 {
            x1017,
            tables_by_chr,
            ..X1::default()
        }
    }

    /// Its mapper's number and name.
    pub fn name(&self) -> (u16, &'static str) {
        match (self.x1017, self.tables_by_chr) {
            (true, _) => (82, "TAITO X1-017"),
            (false, true) => (207, "TAITO X1-005"),
            (false, false) => (80, "TAITO X1-005"),
        }
    }

    /// The RAM it has of its own.
    pub fn ram(&self) -> usize {
        if self.x1017 { X1017_RAM } else { X1005_RAM }
    }

    /// Whether `addr` is one of its registers, $7EF0 to $7EFF.
    pub fn registers_at(addr: u16) -> bool {
        (0x7EF0..=0x7EFF).contains(&addr)
    }

    /// A write to one of its registers: the mirroring, if that's changed.
    pub fn write(&mut self, addr: u16, value: u8) -> Option<Mirroring> {
        let vertical = |value: u8| {
            Some(if value & 0x01 != 0 {
                Mirroring::Vertical
            } else {
                Mirroring::Horizontal
            })
        };
        match (addr & 0x0F, self.x1017) {
            (register @ 0..=5, _) => self.chr[register as usize] = value,
            (6, true) => {
                self.chr_swapped = value & 0x02 != 0;
                return vertical(value);
            }
            (6 | 7, false) if !self.tables_by_chr => return vertical(value),
            (register @ 7..=9, true) => self.keys[register as usize - 7] = value,
            (8 | 9, false) => self.keys[0] = value,
            (register @ 0x0A..=0x0C, true) => self.prg[register as usize - 0x0A] = value >> 2,
            (register @ 0x0A..=0x0F, false) => self.prg[(register as usize - 0x0A) / 2] = value,
            _ => {}
        }
        None
    }

    /// The 8K bank at `addr`, of `banks`: the last fixed.
    pub fn prg_bank(&self, addr: u16, banks: usize) -> usize {
        match addr {
            0x8000..=0xDFFF => self.prg[(addr as usize - 0x8000) >> 13] as usize,
            _ => banks - 1,
        }
    }

    /// The 1K bank CHR address `addr` is in.
    pub fn chr_bank(&self, addr: u16) -> usize {
        let page = addr as usize >> 10 & 7;
        // X1-017's 2K banks at $1000, its 1K banks at $0000, swapped.
        let page = if self.chr_swapped { page ^ 4 } else { page };
        let low = if self.x1017 { 0xFE } else { 0xFF };
        match page {
            0..=3 => (self.chr[page >> 1] & low) as usize + (page & 1),
            _ => self.chr[page - 2] as usize,
        }
    }

    /// Which nametable `addr`'s is, on mapper 207's board: its 2K CHR
    /// bank's top bit, the first's for the top two, the second's for the
    /// bottom.
    pub fn table(&self, addr: u16) -> Option<usize> {
        let bank = self.chr[(addr as usize >> 11) & 1];
        self.tables_by_chr.then_some(bank as usize >> 7)
    }

    /// Where `addr`, $6000 to $7FFF, is in its RAM, if it's there: only
    /// with its key written.
    pub fn ram_offset(&self, addr: u16) -> Option<usize> {
        let at = addr as usize - 0x6000;
        if !self.x1017 {
            let open = (0x1F00..0x2000).contains(&at) && self.keys[0] == X1005_KEY;
            return open.then_some(at & (X1005_RAM - 1));
        }
        let part = match at {
            0x0000..=0x07FF => 0,
            0x0800..=0x0FFF => 1,
            0x1000..=0x13FF => 2,
            _ => return None,
        };
        (self.keys[part] == X1017_KEYS[part]).then_some(at)
    }
}
