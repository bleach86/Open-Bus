//! The language card, in slot 0: 16K of RAM in the ROM's place, $D000 to
//! $FFFF, making the II+ a 64K machine; DOS loads another BASIC, or Pascal,
//! into it, and switches between it and the ROM.
//!
//! $E000 to $FFFF is 8K of it; $D000 to $DFFF is either of two 4K banks,
//! making the 16K. Its switches, $C080 to $C08F, read or written alike, set
//! which bank ($C088 up bank 1; below, bank 2), and whether reads come from
//! the card's RAM or the ROM ($C080 and $C083, RAM; $C081 and $C082, ROM;
//! the same four again at each four up). Writes go to the RAM when it is
//! write enabled, even while the ROM is read: so DOS copies a BASIC into it
//! under the one running. Enabling writes takes two reads in a row of an odd
//! switch, $C081 or $C083, a safeguard against a stray access: the first
//! sets a flip-flop, the pre-write, and the second, with it set, enables
//! them. Any even switch write protects the RAM again and clears the
//! pre-write, and writing to a switch clears it too. Other accesses between
//! the two reads, not to the switches, don't count.
//!
//! At power-on, and on RESET, the card reads the ROM, writes bank 2.
//!
//! The IIe has one built in, and with the auxiliary 64K a second 16K: the
//! MMU's ALTZP picks which, the switches the same for both.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct LanguageCard {
    /// Bank 1, bank 2 ($D000 to $DFFF, each), then $E000 to $FFFF; and the
    /// IIe's auxiliary 16K, laid out the same (none on a II+'s card).
    ram: Vec<u8>,
    aux: Vec<u8>,
    /// Reads from the RAM, not the ROM.
    pub read_ram: bool,
    pub write_enabled: bool,
    pre_write: bool,
    pub bank2: bool,
}

impl LanguageCard {
    pub fn new() -> Self {
        let mut card = LanguageCard {
            ram: vec![0; 0x4000],
            aux: Vec::new(),
            read_ram: false,
            write_enabled: false,
            pre_write: false,
            bank2: false,
        };
        card.reset();
        card
    }

    /// The IIe's: with the auxiliary 16K.
    pub fn with_aux() -> Self {
        LanguageCard {
            aux: vec![0; 0x4000],
            ..LanguageCard::new()
        }
    }

    /// RESET: the ROM read, and bank 2 written; the RAM as it was.
    pub fn reset(&mut self) {
        self.read_ram = false;
        self.write_enabled = true;
        self.pre_write = false;
        self.bank2 = true;
    }

    /// The switches, at $C080 to $C08F: a read, or a write.
    pub fn switch(&mut self, addr: u16, read: bool) {
        let odd = addr & 1 != 0;
        self.read_ram = matches!(addr & 3, 0 | 3);
        self.bank2 = addr & 8 == 0;
        if !odd {
            self.write_enabled = false;
            self.pre_write = false;
        } else if read {
            self.write_enabled |= self.pre_write;
            self.pre_write = true;
        } else {
            self.pre_write = false;
        }
    }

    /// Where `addr`, $D000 to $FFFF, is in the RAM: at $D000 the bank switched in.
    fn offset(&self, addr: u16) -> usize {
        match addr {
            0xD000..=0xDFFF => (addr - 0xD000) as usize + if self.bank2 { 0x1000 } else { 0 },
            _ => (addr - 0xE000) as usize + 0x2000,
        }
    }

    /// A read of $D000 to $FFFF: the RAM's byte, if it is read and not the
    /// ROM; the auxiliary 16K's if `aux`.
    pub fn read(&self, addr: u16, aux: bool) -> Option<u8> {
        let ram = if aux { &self.aux } else { &self.ram };
        self.read_ram.then(|| ram[self.offset(addr)])
    }

    /// A write to $D000 to $FFFF: to the RAM, if it is write enabled; the
    /// auxiliary 16K if `aux`.
    pub fn write(&mut self, addr: u16, value: u8, aux: bool) {
        if self.write_enabled {
            let at = self.offset(addr);
            let ram = if aux { &mut self.aux } else { &mut self.ram };
            ram[at] = value;
        }
    }
}

impl Default for LanguageCard {
    fn default() -> Self {
        Self::new()
    }
}
