//! Starpath's Supercharger, as Stella has it: 6K of RAM in three 2K banks,
//! and a 2K BIOS, loading a game from a cassette into the RAM, a load at a
//! time, each 8448 bytes as an image has them: its 24 pages of 256 bytes,
//! and a header saying where each goes, its number, its start address and
//! its banks.
//!
//! Not Arcadia's BIOS: Stella's own stand-in for it (from its scrom.asm,
//! GPL-2 or later), which reads the load number the game's left at $80 and
//! then, reading $1850, has the load copied straight into the RAM, as if
//! from the tape, its banks and start address put at $FE and $FF for it to
//! go to.
//!
//! What's at $1000 to $17FF and at $1800 to $1FFF, as the byte last held is
//! told at $1FF8: bits 2 to 4, which of the RAM's banks or the BIOS; bit 1,
//! the RAM written; bit 0, the BIOS powered off. A write's no write: an
//! access to $1000 to $10FF holds its address's low byte, and the access
//! 5 distinct addresses later, to RAM, writes it there.

use serde::{Deserialize, Serialize};

const BANK: usize = 0x800;
pub const RAM: usize = 3 * BANK;
/// A load of the tape, as an image has it; its header's place in one.
pub const LOAD: usize = 8448;
const HEADER: usize = 0x2000;

/// Stella's stand-in BIOS, its code at $F800 up: 0xFF at its 109th byte, the
/// loading bars skipped; the rest of its 2K the 6502's jam, and its vectors
/// to $F80A.
const BIOS: [u8; 294] = [
    0xa5, 0xfa, 0x85, 0x80, 0x4c, 0x18, 0xf8, 0xff, 0xff, 0xff, 0x78, 0xd8, 0xa0, 0x00, 0xa2, 0x00,
    0x94, 0x00, 0xe8, 0xd0, 0xfb, 0x4c, 0x50, 0xf8, 0xa2, 0x00, 0xbd, 0x06, 0xf0, 0xad, 0xf8, 0xff,
    0xa2, 0x00, 0xad, 0x00, 0xf0, 0xea, 0xbd, 0x00, 0xf7, 0xca, 0xd0, 0xf6, 0x4c, 0x50, 0xf8, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xa2, 0x03, 0xbc, 0x22, 0xf9, 0x94, 0xfa, 0xca, 0x10, 0xf8, 0xa0, 0x00, 0xa2, 0x28, 0x94, 0x04,
    0xca, 0x10, 0xfb, 0xa2, 0x1c, 0x94, 0x81, 0xca, 0x10, 0xfb, 0xa9, 0xff, 0xc9, 0x00, 0xd0, 0x03,
    0x4c, 0x13, 0xf9, 0xa9, 0x00, 0x85, 0x1b, 0x85, 0x1c, 0x85, 0x1d, 0x85, 0x1e, 0x85, 0x1f, 0x85,
    0x19, 0x85, 0x1a, 0x85, 0x08, 0x85, 0x01, 0xa9, 0x10, 0x85, 0x21, 0x85, 0x02, 0xa2, 0x07, 0xca,
    0xca, 0xd0, 0xfd, 0xa9, 0x00, 0x85, 0x20, 0x85, 0x10, 0x85, 0x11, 0x85, 0x02, 0x85, 0x2a, 0xa9,
    0x05, 0x85, 0x0a, 0xa9, 0xff, 0x85, 0x0d, 0x85, 0x0e, 0x85, 0x0f, 0x85, 0x84, 0x85, 0x85, 0xa9,
    0xf0, 0x85, 0x83, 0xa9, 0x74, 0x85, 0x09, 0xa9, 0x0c, 0x85, 0x15, 0xa9, 0x1f, 0x85, 0x17, 0x85,
    0x82, 0xa9, 0x07, 0x85, 0x19, 0xa2, 0x08, 0xa0, 0x00, 0x85, 0x02, 0x88, 0xd0, 0xfb, 0x85, 0x02,
    0x85, 0x02, 0xa9, 0x02, 0x85, 0x02, 0x85, 0x00, 0x85, 0x02, 0x85, 0x02, 0x85, 0x02, 0xa9, 0x00,
    0x85, 0x00, 0xca, 0x10, 0xe4, 0x06, 0x83, 0x66, 0x84, 0x26, 0x85, 0xa5, 0x83, 0x85, 0x0d, 0xa5,
    0x84, 0x85, 0x0e, 0xa5, 0x85, 0x85, 0x0f, 0xa6, 0x82, 0xca, 0x86, 0x82, 0x86, 0x17, 0xe0, 0x0a,
    0xd0, 0xc3, 0xa9, 0x02, 0x85, 0x01, 0xa2, 0x1c, 0xa0, 0x00, 0x84, 0x19, 0x84, 0x09, 0x94, 0x81,
    0xca, 0x10, 0xfb, 0xa6, 0x80, 0xdd, 0x00, 0xf0, 0xa9, 0x9a, 0xa2, 0xff, 0xa0, 0x00, 0x9a, 0x4c,
    0xfa, 0x00, 0xcd, 0xf8, 0xff, 0x4c,
];
const FAST_BIOS_AT: usize = 109;

/// The BIOS's byte at `at`, $000 to $7FF.
pub fn bios(at: usize) -> u8 {
    match at {
        0x7FC | 0x7FE => 0x0A,
        0x7FD | 0x7FF => 0xF8,
        FAST_BIOS_AT => 0xFF,
        _ => BIOS.get(at).copied().unwrap_or(0x02),
    }
}

/// Where each of the two 2K halves is, by bits 2 to 4 of the configuration:
/// a bank of the RAM, or the BIOS, the fourth.
const LOWER: [usize; 8] = [2, 0, 2, 0, 2, 1, 2, 1];
const UPPER: [usize; 8] = [3, 3, 0, 2, 3, 3, 1, 2];

/// Its registers: where each half's mapped, its RAM written or not, its
/// BIOS powered; the byte held to be written, and whether it's to be, and
/// at which of the distinct accesses it was held; and those, counted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Supercharger {
    halves: [usize; 2],
    write_enabled: bool,
    powered: bool,
    held: u8,
    write_pending: bool,
    held_at: u64,
    accesses: u64,
    last_address: u16,
}

impl Supercharger {
    pub fn new() -> Supercharger {
        let mut supercharger = Supercharger {
            halves: [0; 2],
            write_enabled: false,
            powered: true,
            held: 0,
            write_pending: false,
            held_at: 0,
            accesses: 0,
            last_address: 0,
        };
        supercharger.configure(0);
        supercharger
    }

    /// An access of the CPU's, to `addr`: counted if it's not the last's.
    pub fn seen(&mut self, addr: u16) {
        if addr != self.last_address {
            self.accesses += 1;
            self.last_address = addr;
        }
    }

    fn configure(&mut self, value: u8) {
        let banks = ((value >> 2) & 7) as usize;
        self.powered = value & 0x01 == 0;
        self.write_enabled = value & 0x02 != 0;
        self.halves = [LOWER[banks], UPPER[banks]];
    }

    /// The bank `addr`'s in, and where in it.
    fn place(&self, addr: u16) -> (usize, usize) {
        let half = (addr as usize >> 11) & 1;
        (self.halves[half], addr as usize & 0x7FF)
    }

    /// An access of $1000 to $1FFF: a byte held, the configuration set, or
    /// the byte held written, 5 distinct accesses on.
    fn hotspot(&mut self, addr: u16, ram: &mut [u8]) {
        if self.write_pending && self.accesses > self.held_at + 5 {
            self.write_pending = false;
        }
        if addr & 0x0F00 == 0 && (!self.write_enabled || !self.write_pending) {
            self.held = addr as u8;
            self.held_at = self.accesses;
            self.write_pending = true;
        } else if addr & 0x1FFF == 0x1FF8 {
            self.write_pending = false;
            self.configure(self.held);
        } else if self.write_enabled && self.write_pending && self.accesses == self.held_at + 5 {
            let (bank, at) = self.place(addr);
            if bank < 3 {
                ram[bank * BANK + at] = self.held;
            }
            self.write_pending = false;
        }
    }

    /// Whether a read of `addr` has the BIOS ask for a load: $1850, the BIOS
    /// at $1800.
    pub fn loading(&self, addr: u16) -> bool {
        addr & 0x1FFF == 0x1850 && self.halves[1] == 3
    }

    /// The load numbered `number` copied into the RAM from `tape`, its pages
    /// where its header says; its banks and start address put in the
    /// console's RAM `console`, $FE, $FF and $80, for the BIOS.
    pub fn load(&mut self, tape: &[u8], ram: &mut [u8], console: &mut [u8]) {
        let number = console[0];
        let Some(load) = tape
            .chunks_exact(LOAD)
            .find(|load| load[HEADER + 5] == number)
        else {
            return;
        };
        let header = &load[HEADER..];
        for (n, &place) in header[16..16 + (header[3] as usize).min(24)]
            .iter()
            .enumerate()
        {
            let (bank, page) = ((place & 3) as usize, ((place >> 2) & 7) as usize);
            if bank < 3 {
                let at = bank * BANK + page * 0x100;
                ram[at..at + 0x100].copy_from_slice(&load[n * 0x100..(n + 1) * 0x100]);
            }
        }
        (console[0x7E], console[0x7F], console[0]) = (header[0], header[1], header[2]);
    }

    pub fn read(&mut self, addr: u16, ram: &mut [u8]) -> u8 {
        self.hotspot(addr, ram);
        match self.place(addr) {
            (3, at) => bios(at),
            (bank, at) => ram[bank * BANK + at],
        }
    }

    pub fn write(&mut self, addr: u16, ram: &mut [u8]) {
        self.hotspot(addr, ram);
    }
}
