//! Bandai's FCG boards: the FCG-1 and -2, and the LZ93D50, mapper 16 (by
//! submapper 4 and 5; an old header's, 0, both); the LZ93D50 with a 128-byte
//! serial EEPROM, 159; Datach's, 157; and Famicom Jump II's, 153, with 8K of
//! RAM, which an old header can call 16, its 512K of PRG and CHR RAM saying
//! which it is. As Mesen has them.
//!
//! Its registers, sixteen, by the address's low four bits: from $6000 on the
//! FCG-1 and -2, from $8000 on the LZ93D50, and an old header's, both. $0 to
//! $7, the eight 1K banks of CHR ROM, or with 512K of PRG, their bit 0s
//! ORed, PRG's fifth bit; $8, the 16K at $8000, the last ($0F) fixed at
//! $C000; $9, the nametables, vertical, horizontal, or one screen or the
//! other; $A, bit 0 enabling the IRQ, and acknowledging it, and on the
//! LZ93D50 its latch copied to its counter; $B and $C, the latch's low and
//! high bytes, or on the FCG-1 and -2, the counter's. Enabled, the counter
//! counts CPU cycles down, and at 0 the IRQ line's held.
//!
//! The LZ93D50's EEPROM, 256 bytes (a 24C02) or 128 (a 24C01), a game's
//! saves: talked to a bit at a time, $D's bit 5 its clock and bit 6 its
//! data, and what it says read in bit 4 at $6000 up. Datach's has the 256
//! in its base, shared by its games, and Battle Rush 128 more on its own
//! cartridge, its clock from $0 to $3's bit 3. Datach's barcode reader isn't
//! here: its games play without it, but a card can't be read. Famicom Jump
//! II's has RAM at $6000 instead, $D's bit 5 letting it be read and written,
//! and its PRG's fifth bit from the CHR registers, its CHR RAM unbanked.

use super::cartridge::Mirroring;
use serde::{Deserialize, Serialize};

/// Which board it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Board {
    /// The FCG-1 or -2: its registers from $6000, its counter set directly.
    Fcg,
    /// The LZ93D50: its registers from $8000, its counter from a latch.
    Lz93d50,
    /// An old header's mapper 16, which could be either: its registers in
    /// both places, as the LZ93D50's.
    Either,
    /// Datach's: the LZ93D50, its CHR RAM not banked.
    Datach,
    /// Famicom Jump II's: the LZ93D50, with 8K of RAM at $6000.
    Jump2,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bandai {
    /// Its mapper's number, 16, 157 or 159; and which board it is.
    pub number: u16,
    pub board: Board,
    chr: [u8; 8],
    prg: u8,
    irq_enabled: bool,
    irq_counter: u16,
    irq_latch: u16,
    irq_held: bool,
    /// Whether Famicom Jump II's RAM's to be read and written.
    pub ram_on: bool,
    /// Its EEPROM, if it has one, and Datach's second; their bytes in the
    /// cartridge's RAM, the first's first.
    eeprom: Option<Eeprom>,
    extra: Option<Eeprom>,
}

impl Bandai {
    /// Mapper `number`'s, 16, 157 or 159, with `submapper`: whether it has
    /// `battery` backed RAM, and how much, `nvram` bytes, as a NES 2.0
    /// header says, if it's `told` as one is.
    pub fn new(number: u16, submapper: u8, battery: bool, nvram: usize, told: bool) -> Bandai {
        let board = match (number, submapper) {
            (16, 4) => Board::Fcg,
            (16, 5) | (159, _) => Board::Lz93d50,
            (16, _) => Board::Either,
            (153, _) => Board::Jump2,
            _ => Board::Datach,
        };
        let (eeprom, extra) = match (number, submapper) {
            (159, _) => (Some(Eeprom::new(Chip::C01)), None),
            (157, _) => {
                let extra = !told || battery && nvram != 256 || nvram == 128;
                (
                    Some(Eeprom::new(Chip::C02)),
                    extra.then(|| Eeprom::new(Chip::C01)),
                )
            }
            (16, 0) => (Some(Eeprom::new(Chip::C02)), None),
            (16, 5) if battery || nvram == 256 => (Some(Eeprom::new(Chip::C02)), None),
            _ => (None, None),
        };
        Bandai {
            number,
            board,
            chr: [0; 8],
            prg: 0,
            irq_enabled: false,
            irq_counter: 0,
            irq_latch: 0,
            irq_held: false,
            ram_on: false,
            eeprom,
            extra,
        }
    }

    /// The bytes its EEPROMs keep, in the cartridge's RAM, saved with a
    /// battery's: none, if it has none.
    pub fn eeprom_bytes(&self) -> usize {
        [&self.eeprom, &self.extra]
            .into_iter()
            .flatten()
            .map(|eeprom| eeprom.chip.size())
            .sum()
    }

    /// Whether a write to `addr` is to its registers.
    pub fn registers_at(&self, addr: u16) -> bool {
        match self.board {
            Board::Fcg => (0x6000..0x8000).contains(&addr),
            Board::Lz93d50 | Board::Datach | Board::Jump2 => addr >= 0x8000,
            Board::Either => addr >= 0x6000,
        }
    }

    /// Which 16K of its PRG ROM `addr`, $8000 up, is in, of `banks`: with
    /// 512K, or on Famicom Jump II's, its fifth bit the CHR registers' bit 0s.
    pub fn prg_bank(&self, addr: u16, banks: usize) -> usize {
        let high = if banks >= 0x20 || self.board == Board::Jump2 {
            self.chr
                .iter()
                .fold(0, |high, &bank| high | (bank as usize & 1) << 4)
        } else {
            0
        };
        let bank = if addr < 0xC000 {
            self.prg as usize
        } else {
            0x0F
        };
        bank | high
    }

    /// Which 1K of its CHR ROM `addr` is in; Datach's and Famicom Jump II's,
    /// their CHR RAM's, as it is.
    pub fn chr_offset(&self, addr: u16) -> usize {
        if matches!(self.board, Board::Datach | Board::Jump2) {
            return addr as usize;
        }
        self.chr[addr as usize >> 10 & 7] as usize * 0x400 + (addr as usize & 0x3FF)
    }

    /// A read of $6000 to $7FFF: its EEPROMs' output in bit 4, both of them
    /// saying so if there are two; the rest open bus but bit 3, the barcode
    /// reader's, low.
    pub fn read(&self, open_bus: u8) -> u8 {
        let said = match (&self.eeprom, &self.extra) {
            (Some(eeprom), Some(extra)) => eeprom.output & extra.output,
            (Some(eeprom), None) => eeprom.output,
            _ => 0,
        };
        said << 4 | open_bus & 0xE7
    }

    /// A write to its registers, its EEPROMs' bytes `ram` (the first's
    /// first): the nametables, if they're switched; and whether a byte of
    /// the EEPROMs was written.
    pub fn write(&mut self, addr: u16, value: u8, ram: &mut [u8]) -> (Option<Mirroring>, bool) {
        let fcg = self.board == Board::Fcg;
        let mut mirroring = None;
        let mut written = false;
        match addr & 0x0F {
            bank @ 0..=7 => {
                self.chr[bank as usize] = value;
                if bank <= 3 && self.board == Board::Datach {
                    if let Some(extra) = &mut self.extra {
                        let at = self.eeprom.as_ref().map_or(0, |eeprom| eeprom.chip.size());
                        written |= extra.write(value >> 3 & 1, extra.data_line, &mut ram[at..]);
                    }
                }
            }
            0x8 => self.prg = value & 0x0F,
            0x9 => {
                mirroring = Some(match value & 3 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleLower,
                    _ => Mirroring::SingleUpper,
                })
            }
            0xA => {
                self.irq_enabled = value & 1 != 0;
                if !fcg {
                    self.irq_counter = self.irq_latch;
                }
                self.irq_held = false;
            }
            0xB | 0xC => {
                let shift = if addr & 0x0F == 0xB { 0 } else { 8 };
                let target = if fcg {
                    &mut self.irq_counter
                } else {
                    &mut self.irq_latch
                };
                *target = (*target & !(0xFF << shift)) | (value as u16) << shift;
            }
            0xD if self.board == Board::Jump2 => self.ram_on = value & 0x20 != 0,
            0xD => {
                let (clock, data) = (value >> 5 & 1, value >> 6 & 1);
                let size = self.eeprom.as_ref().map_or(0, |eeprom| eeprom.chip.size());
                if let Some(eeprom) = &mut self.eeprom {
                    written |= eeprom.write(clock, data, &mut ram[..size]);
                }
                if let Some(extra) = &mut self.extra {
                    written |= extra.write(extra.clock_line, data, &mut ram[size..]);
                }
            }
            _ => {}
        }
        (mirroring, written)
    }

    /// A CPU cycle: its IRQ counter, if it's enabled, counting down, and at
    /// 0 holding the line. (Looked at before it's counted, as Mesen finds
    /// both Famicom Jump II and Magical Taruruuto-kun 2 want.)
    pub fn cycle(&mut self) {
        if self.irq_enabled {
            self.irq_held |= self.irq_counter == 0;
            self.irq_counter = self.irq_counter.wrapping_sub(1);
        }
    }

    pub fn irq(&self) -> bool {
        self.irq_held
    }
}

/// Which EEPROM: the 24C01's 128 bytes, addressed by 7 bits sent after the
/// start, least significant first; or the 24C02's 256, after a byte naming
/// the chip, most significant first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Chip {
    C01,
    C02,
}

impl Chip {
    pub fn size(self) -> usize {
        match self {
            Chip::C01 => 128,
            Chip::C02 => 256,
        }
    }
}

/// What a serial EEPROM's doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Mode {
    Idle,
    ChipAddress,
    Address,
    Read,
    Write,
    SendAck,
    WaitAck,
}

/// A serial EEPROM, an I²C one, talked to on its clock and data lines: a
/// start, the data line falling while the clock's high; a stop, rising; a
/// bit taken as the clock rises, and what it says sent then too.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eeprom {
    chip: Chip,
    mode: Mode,
    next: Mode,
    chip_address: u8,
    address: u8,
    data: u8,
    bits: u8,
    /// What it says on the data line, 1 when it's saying nothing.
    output: u8,
    clock_line: u8,
    data_line: u8,
}

impl Eeprom {
    fn new(chip: Chip) -> Eeprom {
        Eeprom {
            chip,
            mode: Mode::Idle,
            next: Mode::Idle,
            chip_address: 0,
            address: 0,
            data: 0,
            bits: 0,
            output: 1,
            clock_line: 0,
            data_line: 0,
        }
    }

    /// A bit taken into `byte`, in the chip's order.
    fn take_bit(chip: Chip, bits: &mut u8, byte: &mut u8, bit: u8) {
        if *bits < 8 {
            let at = if chip == Chip::C01 { *bits } else { 7 - *bits };
            *byte = (*byte & !(1 << at)) | bit << at;
            *bits += 1;
        }
    }

    /// The next bit of what's read said.
    fn send_bit(&mut self) {
        if self.bits < 8 {
            let at = if self.chip == Chip::C01 {
                self.bits
            } else {
                7 - self.bits
            };
            self.output = self.data >> at & 1;
            self.bits += 1;
        }
    }

    /// Its lines as they are now, `clock` and `data`, its bytes `ram`:
    /// whether one was written.
    fn write(&mut self, clock: u8, data: u8, ram: &mut [u8]) -> bool {
        let mut written = false;
        let held_high = self.clock_line == 1 && clock == 1;
        if held_high && data < self.data_line {
            // Start.
            self.mode = if self.chip == Chip::C01 {
                self.address = 0;
                Mode::Address
            } else {
                Mode::ChipAddress
            };
            self.bits = 0;
            self.output = 1;
        } else if held_high && data > self.data_line {
            // Stop.
            self.mode = Mode::Idle;
            self.output = 1;
        } else if clock > self.clock_line {
            self.rising(data, ram);
        } else if clock < self.clock_line {
            written = self.falling(ram);
        }
        self.clock_line = clock;
        self.data_line = data;
        written
    }

    /// The clock rising, with `data` on the data line.
    fn rising(&mut self, data: u8, ram: &[u8]) {
        let chip = self.chip;
        match self.mode {
            Mode::ChipAddress => Self::take_bit(chip, &mut self.bits, &mut self.chip_address, data),
            // The 24C01's: 7 bits of address, then whether it's to read.
            Mode::Address if chip == Chip::C01 && self.bits == 7 => {
                self.bits = 8;
                self.next = if data == 1 {
                    self.data = ram[self.address as usize & 0x7F];
                    Mode::Read
                } else {
                    Mode::Write
                };
            }
            Mode::Address => Self::take_bit(chip, &mut self.bits, &mut self.address, data),
            Mode::Read => self.send_bit(),
            Mode::Write => Self::take_bit(chip, &mut self.bits, &mut self.data, data),
            Mode::SendAck => self.output = 0,
            // Acknowledged, the 24C02 reads on, and the 24C01 stops (as
            // Mesen has it).
            Mode::WaitAck if data == 0 && chip == Chip::C02 => {
                self.next = Mode::Read;
                self.data = ram[self.address as usize];
            }
            Mode::WaitAck if data == 0 => self.next = Mode::Idle,
            _ => {}
        }
    }

    /// The clock falling: a byte done, its acknowledgement, and what's next;
    /// whether a byte was written.
    fn falling(&mut self, ram: &mut [u8]) -> bool {
        let done = self.bits == 8;
        let size = self.chip.size();
        match (self.mode, self.chip) {
            (Mode::ChipAddress, _) if done => {
                self.bits = 0;
                self.output = 1;
                if self.chip_address & 0xA0 == 0xA0 {
                    self.mode = Mode::SendAck;
                    // Its last bit, to read from where it is, or else to be
                    // told where.
                    self.next = if self.chip_address & 1 != 0 {
                        Mode::Read
                    } else {
                        Mode::Address
                    };
                    if self.next == Mode::Read {
                        self.data = ram[self.address as usize];
                    }
                } else {
                    self.mode = Mode::Idle;
                }
            }
            (Mode::Address, Chip::C01) if done => {
                self.mode = Mode::SendAck;
                self.output = 1;
            }
            (Mode::Address, Chip::C02) if done => {
                self.bits = 0;
                self.mode = Mode::SendAck;
                self.next = Mode::Write;
                self.output = 1;
            }
            (Mode::Read, _) if done => {
                self.mode = Mode::WaitAck;
                self.address = ((self.address as usize + 1) % size) as u8;
            }
            (Mode::Write, _) if done => {
                self.bits = 0;
                self.mode = Mode::SendAck;
                self.next = if self.chip == Chip::C01 {
                    Mode::Idle
                } else {
                    Mode::Write
                };
                ram[self.address as usize % size] = self.data;
                self.address = ((self.address as usize + 1) % size) as u8;
                return true;
            }
            (Mode::SendAck, _) | (Mode::WaitAck, Chip::C02) => {
                self.mode = self.next;
                self.bits = 0;
                self.output = 1;
            }
            _ => {}
        }
        false
    }
}
