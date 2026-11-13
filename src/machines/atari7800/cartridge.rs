//! A 7800 cartridge, at $4000 to $FFFF, as a7800 has its boards: plain, its
//! ROM at the top, up to 48K; SuperGame, 16K banks, one of them switched in
//! at $8000 by a write there, the last fixed at $C000, and at $4000 the one
//! before the last, or 16K of RAM; SuperGame of 9 banks, bank 0 at $4000
//! and the others after it; Absolute's, F-18 Hornet's, two 16K banks at
//! $4000; Activision's, Double Dragon's and Rampage's, a bank switched by
//! a write at $FF80 to $FF8F, and the rest in 8K pieces of their own order;
//! plain with RAM at $4000, every other page the same (Rescue on
//! Fractalus); and the bankset boards, a homebrew's, two images, the CPU's
//! and MARIA's, each read only by its own: plain at the top (52K of them
//! from $3000) or SuperGame, the bank switched for both; with banked RAM,
//! 16K each at $4000, the CPU writing MARIA's at $C000. A POKEY, if it has
//! one, at $4000 (up to $7FFF), $0450 or $0800.
//!
//! Which, as ProSystem's database says, or its .a78 header, or its size. A
//! board that's more or less ROM than it has room for, as a7800 has it: of
//! more, the last that fits; of less, $FF where it's missing.

use serde::{Deserialize, Serialize};

const BANK: usize = 0x4000;

/// Its board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Plain,
    SuperGame,
    SuperGameRam,
    SuperGame9,
    Absolute,
    Activision,
    MirrorRam,
    Bankset,
    BanksetRam,
    BanksetSuperGame,
    BanksetSuperGameRam,
}

impl Kind {
    /// As ProSystem's database numbers it.
    pub fn numbered(number: u8) -> Result<Kind, String> {
        Ok(match number {
            0 => Kind::Plain,
            // Its "SuperCart ROM", bank 6 at $4000, is a7800's SuperGame.
            1 | 4 => Kind::SuperGame,
            2 => Kind::SuperGame9,
            3 => Kind::SuperGameRam,
            5 => Kind::Absolute,
            6 => Kind::Activision,
            _ => return Err(format!("a cartridge of type {number}, not one we have")),
        })
    }

    /// As an .a78 header's cartridge type, `head`, says, with its POKEY's
    /// address: what a7800 makes of it, two things at $4000 one.
    pub fn headed(head: u16) -> (Kind, Option<u16>) {
        let mut head = head;
        // Two things claimed at $4000: one of them.
        match head & 0x3D {
            0x05 | 0x09 | 0x11 | 0x21 => head &= !0x01,
            0x0C | 0x14 | 0x24 => head &= !0x04,
            0x18 | 0x28 => head &= !0x08,
            0x30 => head &= !0x10,
            _ => {}
        }
        let pokey_4000 = head & 0x01 != 0;
        let (kind, mut pokey) = match head & 0xE02E {
            0x0000 => (Kind::Plain, pokey_4000.then_some(0x4000)),
            0x0002 => (Kind::SuperGame, pokey_4000.then_some(0x4000)),
            0x0006 => (Kind::SuperGameRam, None),
            0x000A => (Kind::SuperGame9, None),
            0x8000 => (Kind::Plain, Some(0x0800)),
            0x8002 => (Kind::SuperGame, Some(0x0800)),
            0x8006 => (Kind::SuperGameRam, Some(0x0800)),
            0x800A => (Kind::SuperGame9, Some(0x0800)),
            0x2000 => (Kind::Bankset, pokey_4000.then_some(0x4000)),
            0x6000 => (Kind::BanksetRam, None),
            0xA000 => (Kind::Bankset, Some(0x0800)),
            0xE000 => (Kind::BanksetRam, Some(0x0800)),
            0x2002 => (Kind::BanksetSuperGame, None),
            0x6002 => (Kind::BanksetSuperGameRam, None),
            0xA002 => (Kind::BanksetSuperGame, Some(0x0800)),
            0xE002 => (Kind::BanksetSuperGameRam, Some(0x0800)),
            // The banked RAM boards and the rest a7800 doesn't know, as
            // plain.
            _ => (Kind::Plain, None),
        };
        if head & 0x40 != 0 {
            pokey = Some(0x0450);
        }
        let kind = match head & 0xFF00 {
            0x0100 => Kind::Activision,
            0x0200 => Kind::Absolute,
            _ if head & 0x80 != 0 => Kind::MirrorRam,
            _ => kind,
        };
        (kind, pokey)
    }

    /// As its size says, `size` bytes, if nothing else says: plain up to
    /// 48K, SuperGame of 9 banks at 144K, else SuperGame.
    pub fn sized(size: usize) -> Kind {
        match size {
            ..=0xC000 => Kind::Plain,
            0x24000 => Kind::SuperGame9,
            _ => Kind::SuperGame,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cartridge {
    #[serde(skip)]
    pub rom: Vec<u8>,
    #[serde(skip)]
    pub kind: Kind,
    /// Its POKEY's address, if it has one.
    #[serde(skip)]
    pub pokey: Option<u16>,
    /// The bank switched in; and its RAM, if it has any.
    bank: usize,
    ram: Vec<u8>,
}

impl Default for Kind {
    fn default() -> Kind {
        Kind::Plain
    }
}

impl Cartridge {
    pub fn new(rom: &[u8], kind: Kind, pokey: Option<u16>) -> Result<Cartridge, String> {
        if rom.is_empty() {
            return Err("an empty cartridge".to_string());
        }
        let ram = match kind {
            Kind::SuperGameRam | Kind::MirrorRam => vec![0; 0x4000],
            Kind::BanksetRam | Kind::BanksetSuperGameRam => vec![0; 0x8000],
            _ => Vec::new(),
        };
        Ok(Cartridge {
            rom: rom.to_vec(),
            kind,
            pokey,
            bank: 0,
            ram,
        })
    }

    /// The last of its 16K banks, as a7800 has it: 9 banks a SuperGame of
    /// 9's, the first fixed at $4000.
    fn last_bank(&self) -> usize {
        let banks = self.rom.len().div_ceil(BANK);
        if banks % 2 == 1 {
            banks.saturating_sub(2)
        } else {
            banks - 1
        }
    }

    /// Its ROM at `at`, $4000 to $FFFF, the top of it there.
    fn top(&self, at: usize) -> u8 {
        match (at + self.rom.len()).checked_sub(0x10000) {
            Some(at) => self.rom[at],
            None => 0xFF,
        }
    }

    /// Whether a POKEY's at `addr`, $4000 to $7FFF, if it's there.
    pub fn pokey_at(&self, addr: u16) -> bool {
        self.pokey == Some(0x4000) && addr < 0x8000
    }

    /// Whether it's a bankset board of 52K images, from $3000.
    pub fn from_3000(&self) -> bool {
        self.kind == Kind::Bankset && self.rom.len() >= 0x1A000
    }

    /// What's at `addr`, $4000 to $FFFF, as the CPU reads it.
    pub fn read(&self, addr: u16) -> u8 {
        self.read_as(addr, false)
    }

    /// What's at `addr`, as MARIA's DMA reads it: a bankset's own image.
    pub fn maria_read(&self, addr: u16) -> u8 {
        self.read_as(addr, true)
    }

    /// What's at `addr`, $4000 to $FFFF (or $3000 up, for a 52K bankset),
    /// to the CPU, or `maria`: its ROM or its RAM.
    fn read_as(&self, addr: u16, maria: bool) -> u8 {
        let at = addr as usize;
        let rom = |at: usize| self.rom.get(at).copied().unwrap_or(0xFF);
        match self.kind {
            Kind::Plain => self.top(at),
            Kind::MirrorRam if at < 0x8000 => self.ram[(at - 0x4000) & 0xFEFF],
            Kind::MirrorRam => self.top(at),
            Kind::SuperGame | Kind::SuperGameRam => {
                let last = self.last_bank();
                match at {
                    ..0x8000 if self.kind == Kind::SuperGameRam => self.ram[at - 0x4000],
                    ..0x8000 => rom(last.saturating_sub(1) * BANK + (at & 0x3FFF)),
                    ..0xC000 => rom(self.bank * BANK + (at & 0x3FFF)),
                    _ => rom(last * BANK + (at & 0x3FFF)),
                }
            }
            Kind::SuperGame9 => match at {
                ..0x8000 => rom(at & 0x3FFF),
                ..0xC000 => rom(self.bank * BANK + (at & 0x3FFF)),
                _ => rom((self.last_bank() + 1) * BANK + (at & 0x3FFF)),
            },
            Kind::Absolute => match at {
                ..0x8000 => rom(self.bank * BANK + (at & 0x3FFF)),
                _ => rom(at),
            },
            Kind::Activision => {
                let offset = at & 0x1FFF;
                match at - 0x4000 {
                    0x0000..0x2000 => rom(offset + 0x1A000),
                    0x2000..0x4000 => rom(offset + 0x18000),
                    0x4000..0x6000 => rom(offset + 0x1E000),
                    0x6000..0x8000 => rom(offset + self.bank * BANK),
                    0x8000..0xA000 => rom(offset + self.bank * BANK + 0x2000),
                    _ => rom(offset + 0x1C000),
                }
            }
            // A bankset's two images, the CPU's then MARIA's; its RAM the
            // same, 16K each.
            Kind::Bankset
            | Kind::BanksetRam
            | Kind::BanksetSuperGame
            | Kind::BanksetSuperGameRam => {
                let half = self.rom.len() / 2;
                let image = if maria { half } else { 0 };
                let ram = |at: usize| self.ram[(maria as usize) * BANK + (at - 0x4000)];
                let has_ram = matches!(self.kind, Kind::BanksetRam | Kind::BanksetSuperGameRam);
                match self.kind {
                    _ if has_ram && (0x4000..0x8000).contains(&at) => ram(at),
                    Kind::Bankset if self.from_3000() => {
                        let image = if maria { 0xD000 } else { 0 };
                        rom(image + at - 0x3000)
                    }
                    Kind::Bankset | Kind::BanksetRam => match (at + half).checked_sub(0x10000) {
                        Some(at) => rom(image + at),
                        None => 0xFF,
                    },
                    _ => {
                        let last = (half / BANK).saturating_sub(1);
                        match at {
                            ..0x8000 if self.pokey.is_some() => 0xFF,
                            ..0x8000 => rom(image + last.saturating_sub(1) * BANK + (at & 0x3FFF)),
                            ..0xC000 => rom(image + self.bank * BANK + (at & 0x3FFF)),
                            _ => rom(image + last * BANK + (at & 0x3FFF)),
                        }
                    }
                }
            }
        }
    }

    /// A write of `value` to `addr`, $4000 to $FFFF: a bank switched, or its
    /// RAM written.
    pub fn write(&mut self, addr: u16, value: u8) {
        let at = addr as usize;
        match self.kind {
            Kind::MirrorRam if at < 0x8000 => self.ram[(at - 0x4000) & 0xFEFF] = value,
            Kind::SuperGameRam if at < 0x8000 => self.ram[at - 0x4000] = value,
            Kind::SuperGame | Kind::SuperGameRam if (0x8000..0xC000).contains(&at) => {
                self.bank = value as usize & self.last_bank();
            }
            Kind::SuperGame9 if (0x8000..0xC000).contains(&at) => {
                self.bank = (value as usize & self.last_bank()) + 1;
            }
            Kind::Absolute if at == 0x8000 => {
                if value & 1 != 0 {
                    self.bank = 0;
                } else if value & 2 != 0 {
                    self.bank = 1;
                }
            }
            Kind::Activision if (0xFF80..=0xFF8F).contains(&at) => self.bank = at & 7,
            // A bankset's RAM: the CPU's at $4000, MARIA's written at $C000.
            Kind::BanksetRam | Kind::BanksetSuperGameRam if at < 0x8000 => {
                self.ram[at - 0x4000] = value;
            }
            Kind::BanksetRam | Kind::BanksetSuperGameRam if at >= 0xC000 => {
                self.ram[BANK + at - 0xC000] = value;
            }
            Kind::BanksetSuperGame | Kind::BanksetSuperGameRam
                if (0x8000..0xC000).contains(&at) =>
            {
                let last = (self.rom.len() / 2 / BANK).saturating_sub(1);
                self.bank = (value as usize & 0x0F) & last;
            }
            _ => {}
        }
    }

    /// Its ROM, taken from `running`, the cartridge a state's loaded into.
    pub fn take_rom(&mut self, running: &mut Cartridge) {
        self.rom = std::mem::take(&mut running.rom);
        self.kind = running.kind;
        self.pokey = running.pokey;
    }

    /// What it is, in words: its board, its size, and its POKEY, if it has
    /// one; plain, its size alone.
    pub fn name(&self) -> String {
        let board = match self.kind {
            Kind::Plain => "",
            Kind::SuperGame => "SUPERGAME ",
            Kind::SuperGameRam => "SUPERGAME RAM ",
            Kind::SuperGame9 => "SUPERGAME 9 BANKS ",
            Kind::Absolute => "ABSOLUTE ",
            Kind::Activision => "ACTIVISION ",
            Kind::MirrorRam => "RAM ",
            Kind::Bankset => "BANKSET ",
            Kind::BanksetRam => "BANKSET RAM ",
            Kind::BanksetSuperGame => "BANKSET SUPERGAME ",
            Kind::BanksetSuperGameRam => "BANKSET SUPERGAME RAM ",
        };
        let pokey = if self.pokey.is_some() { " POKEY" } else { "" };
        format!("{board}{}K{pokey}", self.rom.len() / 1024)
    }
}
