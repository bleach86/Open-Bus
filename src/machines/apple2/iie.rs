//! The Apple IIe's own, beside what it has of the II+: its auxiliary 64K
//! (the extended 80-column card's); its ROM's first 4K, $C000 to $CFFF,
//! its own firmware in the slots' places (slot 3's the 80 columns'); its
//! video ROM, the characters' dots; and its MMU's and IOU's soft switches.
//!
//! The MMU's, written at $C000 to $C00B, each off at its even address and
//! on at its odd: 80STORE, RAMRD, RAMWRT, INTCXROM, ALTZP and SLOTC3ROM.
//! Of $0000 to $BFFF, zero page and the stack are the auxiliary 48K's with
//! ALTZP (and the language card's RAM, its auxiliary 16K); the text page,
//! and with HIRES high-res page 1, with 80STORE, are as PAGE2 says,
//! auxiliary or main; the rest is read from it with RAMRD, written with
//! RAMWRT. Of $C100 to $CFFF, the IIe's own ROM answers all of it with
//! INTCXROM; slot 3's page with SLOTC3ROM off; and $C800 up while INTC8ROM
//! is on: an access to slot 3's own page switches it on, an access to
//! $CFFF off. RESET switches them all off.
//!
//! The IOU's: 80COL ($C00C and $C00D), ALTCHARSET ($C00E and $C00F), and
//! annunciator 3, its $C05E switching double high-res on, $C05F off. Each
//! switch is read back at $C011 to $C01F, in bit 7.

use serde::{Deserialize, Serialize};

/// The enhanced IIe's ROMs, by the names they're dumped under: its CD
/// ROM ($C000 to $DFFF), its EF ROM ($E000 to $FFFF), its video ROM.
pub const ENHANCED_ROMS: [&str; 3] = [
    "Apple IIe Enhanced ROM Pages C0-DF - 342-0304-A - 1984.bin",
    "Apple IIe Enhanced ROM Pages E0-FF - 342-0303-A - 1984.bin",
    "Apple IIe Enhanced Video ROM - 342-0265-A - US 1983.bin",
];

/// The first IIe's.
pub const ROMS: [&str; 3] = [
    "Apple IIe ROM Pages C0-DF - 342-0135-A - 1982.bin",
    "Apple IIe ROM Pages E0-FF - 342-0134-A - 1982.bin",
    "Apple IIe Video ROM - 342-0133-A - US 1982.bin",
];

/// Its ROMs aren't in a save state: the machine it's loaded into has them.
#[derive(Serialize, Deserialize)]
pub struct Iie {
    /// The auxiliary 48K, $0000 to $BFFF; its 16K more is the language
    /// card's.
    pub aux: Vec<u8>,
    /// The ROM's $C000 to $CFFF: its own firmware, $C100 up.
    #[serde(skip)]
    pub internal_rom: Vec<u8>,
    /// The video ROM: each character's 8 rows of dots, the lowest bit the
    /// leftmost, a dot shown where its bit is 0.
    #[serde(skip)]
    pub characters: Vec<u8>,
    // The MMU's switches.
    pub store80: bool,
    pub ramrd: bool,
    pub ramwrt: bool,
    pub intcxrom: bool,
    pub altzp: bool,
    pub slotc3rom: bool,
    pub intc8rom: bool,
    // The IOU's.
    pub col80: bool,
    pub altcharset: bool,
    pub dhires: bool,
}

impl Iie {
    /// The IIe's with its CD ROM's first 4K, `internal_rom`, and its video
    /// ROM, `characters`: the enhanced IIe's or the first's, their ROMs
    /// all that differs, and their CPUs.
    pub fn new(internal_rom: Vec<u8>, characters: Vec<u8>) -> Self {
        assert_eq!(internal_rom.len(), 0x1000, "the CD ROM's $C000 to $CFFF is 4K");
        assert_eq!(characters.len(), 0x1000, "the video ROM is 4K");
        Iie {
            aux: vec![0; 0xC000],
            internal_rom,
            characters,
            store80: false,
            ramrd: false,
            ramwrt: false,
            intcxrom: false,
            altzp: false,
            slotc3rom: false,
            intc8rom: false,
            col80: false,
            altcharset: false,
            dhires: false,
        }
    }

    /// RESET: the MMU's switches off, and the IOU's 80COL and ALTCHARSET.
    pub fn reset(&mut self) {
        (self.store80, self.ramrd, self.ramwrt, self.intcxrom) = (false, false, false, false);
        (self.altzp, self.slotc3rom, self.intc8rom) = (false, false, false);
        (self.col80, self.altcharset) = (false, false);
    }

    /// A write to $C000 to $C00F: a switch, off at the even address, on at
    /// the odd.
    pub fn switch(&mut self, addr: u16) {
        let on = addr & 1 != 0;
        match addr & 0x0E {
            0x0 => self.store80 = on,
            0x2 => self.ramrd = on,
            0x4 => self.ramwrt = on,
            0x6 => self.intcxrom = on,
            0x8 => self.altzp = on,
            0xA => self.slotc3rom = on,
            0xC => self.col80 = on,
            _ => self.altcharset = on,
        }
    }

    /// Whether `addr`, $0000 to $BFFF, is the auxiliary 48K's, for a read
    /// or a `write`, as the switches have it, PAGE2's and HIRES's too.
    pub fn is_aux(&self, addr: u16, write: bool, page2: bool, hires: bool) -> bool {
        match addr {
            0x0000..=0x01FF => self.altzp,
            0x0400..=0x07FF if self.store80 => page2,
            0x2000..=0x3FFF if self.store80 && hires => page2,
            _ if write => self.ramwrt,
            _ => self.ramrd,
        }
    }

    /// An access to $C100 to $CFFF: whether its own ROM answers it, and
    /// INTC8ROM switched as the access does it.
    pub fn internal(&mut self, addr: u16) -> bool {
        let page = addr >> 8;
        if page == 0xC3 && !self.slotc3rom {
            self.intc8rom = true;
        }
        let internal = self.intcxrom || (page == 0xC3 && !self.slotc3rom) || (page >= 0xC8 && self.intc8rom);
        if addr == 0xCFFF {
            self.intc8rom = false;
        }
        internal
    }

    /// The text page the video shows: page 2 only with PAGE2 and not
    /// 80STORE, which makes PAGE2 the memory's choice instead.
    pub fn shown_page2(&self, page2: bool) -> bool {
        page2 && !self.store80
    }
}
