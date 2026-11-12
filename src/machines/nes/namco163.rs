//! Namco's 163, mapper 19, with its sound; and its lesser kin, the 175 and
//! 340, mapper 210 (submapper 1 and 2). As Mesen has them.
//!
//! PRG: $E000, $E800 and $F000, the 8K banks at $8000, $A000 and $C000,
//! their low six bits; the last fixed at $E000. CHR: $8000 to $B800, each
//! 2K of the CPU's addresses a register, the eight 1K banks of the pattern
//! tables. On the 163, a bank of $E0 up is the PPU's own nametable RAM, its
//! bit 0 which 1K, unless $E800's bit 6 (for the low four) or bit 7 (the
//! high four) says it's CHR ROM after all; and $C000 to $D800, the four 1K
//! nametables, each CHR ROM, or, $E0 up, the nametable RAM. Until one's
//! written, it's as the board's wired.
//!
//! The 175's $C000 bit 0 lets its RAM, at $6000 up, be written. The 340 has
//! none; its $E000's top two bits are its nametables: one screen or the
//! other, vertical, or horizontal.
//!
//! The 163's RAM is written only with $F800's bit 6 set, each 2K of it
//! unless its own bit, 0 to 3, is. And it has 128 bytes of its own, for its
//! sound, and saved with the battery after the RAM: $F800, the address of
//! the byte $4800 reads and writes, bit 7 moving it on after each. Its IRQ
//! counter, 15 bits, at $5000 and $5800, read back; $5800's bit 7 enabling
//! it. Enabled, it counts CPU cycles up to $7FFF, and there holds the IRQ
//! line, until either's written.
//!
//! A board the database doesn't name, which of the three it is is guessed
//! at from what its game writes, as Mesen does: the 163's registers make it
//! a 163, $C000 alone a 175, $E000's top bits a 340, unless it's written
//! RAM, which a 340 hasn't.
//!
//! Its sound: up to eight channels of waves, of 4-bit samples in its 128
//! bytes, each channel's frequency, phase, length, where its wave starts
//! and its volume in the top 64 of them (channel 7's at $78 up, 6's below
//! and so on), $7F's bits 4 to 6 how many there are, less 1. One channel
//! moves on every 15 CPU cycles, the highest first, in turn; what's heard
//! is all of their latest, shared out. $E000's bit 6 silences it.

use super::cartridge::Mirroring;
use serde::{Deserialize, Serialize};

/// The 128 bytes of its own, after its RAM; and how they're addressed.
pub const CHIP_RAM: usize = 0x80;

/// Which of the three it is: or not known yet, if the board's not named.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Chip {
    N163,
    N175,
    N340,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Namco163 {
    /// Its mapper's number, 19 or 210; and which chip it is.
    pub number: u16,
    pub chip: Chip,
    /// Whether which it is is guessed at, from what's written.
    guessing: bool,
    /// Whether its RAM's been written to, which the 340 hasn't.
    not_340: bool,
    prg: [u8; 3],
    chr: [u8; 8],
    /// The nametables, each as written; none, as the board's wired.
    nametables: [Option<u8>; 4],
    /// Whether the low four pattern banks, and the high four, are CHR ROM
    /// at $E0 up, not the nametable RAM.
    rom_above_e0: [bool; 2],
    write_protect: u8,
    irq_counter: u16,
    /// Whether the IRQ line's held: the counter's counted to the top since
    /// it was last written.
    irq_held: bool,
    sound: Sound,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Sound {
    /// The byte $4800 reads and writes, and whether it moves on after.
    address: u8,
    increment: bool,
    silent: bool,
    /// Cycles since a channel moved on, and which is next.
    ticks: u8,
    channel: u8,
    /// Each channel's latest, and theirs shared out.
    outputs: [i16; 8],
    output: i16,
}

impl Namco163 {
    /// Mapper `number`'s, 19 or 210: its `submapper`, or, on 19, its
    /// `board` as the database names it, saying which, if they do.
    pub fn new(number: u16, submapper: u8, board: &str) -> Namco163 {
        let (chip, guessing) = match (number, submapper, board) {
            (19, _, "NAMCOT-175") => (Chip::N175, false),
            (19, _, "NAMCOT-340") => (Chip::N340, false),
            (19, _, "NAMCOT-163") => (Chip::N163, false),
            (19, _, _) => (Chip::N163, true),
            (_, 1, _) => (Chip::N175, false),
            (_, 2, _) => (Chip::N340, false),
            _ => (Chip::Unknown, true),
        };
        Namco163 {
            number,
            chip,
            guessing,
            not_340: false,
            prg: [0; 3],
            chr: [0; 8],
            nametables: [None; 4],
            rom_above_e0: [false; 2],
            write_protect: 0,
            irq_counter: 0,
            irq_held: false,
            sound: Sound {
                address: 0,
                increment: false,
                silent: false,
                ticks: 0,
                channel: 7,
                outputs: [0; 8],
                output: 0,
            },
        }
    }

    /// What's written says it's `chip`, if which is being guessed at; but
    /// not a 340, once it's written its RAM.
    fn seems(&mut self, chip: Chip) {
        if self.guessing && !(self.not_340 && chip == Chip::N340) {
            self.chip = chip;
        }
    }

    /// Which 8K of its PRG ROM `addr`, $8000 up, is in, of `banks`.
    pub fn prg_bank(&self, addr: u16, banks: usize) -> usize {
        match (addr - 0x8000) / 0x2000 {
            3 => banks - 1,
            slot => self.prg[slot as usize] as usize,
        }
    }

    /// Which 1K of its CHR ROM pattern table address `addr` is in.
    pub fn chr_bank(&self, addr: u16) -> usize {
        self.chr[addr as usize >> 10 & 7] as usize
    }

    /// Which 1K of the PPU's nametable RAM pattern table address `addr` is
    /// in, if it's there: a 163's bank of $E0 up.
    #[inline]
    pub fn chr_nametable(&self, addr: u16) -> Option<usize> {
        let bank = self.chr[addr as usize >> 10 & 7];
        let rom = self.rom_above_e0[addr as usize >> 12 & 1];
        (self.chip == Chip::N163 && bank >= 0xE0 && !rom).then_some(bank as usize & 1)
    }

    /// Where nametable address `addr` is: in the nametable RAM, which 1K of
    /// it, or in CHR ROM, which 1K; none, as the board's wired.
    pub fn nametable(&self, addr: u16) -> Option<Result<usize, usize>> {
        let bank = self.nametables[addr as usize >> 10 & 3]?;
        Some(if bank >= 0xE0 {
            Ok(bank as usize & 1)
        } else {
            Err(bank as usize)
        })
    }

    /// Whether any pattern table's in the nametable RAM, or any nametable in
    /// CHR ROM: the PPU then to fetch each, not take them as it can.
    pub fn moves_tables(&self) -> bool {
        (0..8).any(|bank| self.chr_nametable(bank * 0x400).is_some())
            || self.nametables.iter().flatten().any(|&bank| bank < 0xE0)
    }

    /// Where `addr`, $6000 to $7FFF, is in its RAM, of `size`, if it's there
    /// to read, or `writing`, to write.
    pub fn ram_offset(&self, addr: u16, size: usize, writing: bool) -> Option<usize> {
        let at = addr as usize - 0x6000;
        let writable = match self.chip {
            _ if size == 0 => return None,
            Chip::N163 => {
                self.write_protect & 0x40 != 0 && self.write_protect >> (at / 0x800) & 1 == 0
            }
            Chip::N175 => self.write_protect & 1 != 0,
            // A 340's none; nor one not known yet, till it's known a 175.
            Chip::N340 | Chip::Unknown => return None,
        };
        (writable || !writing).then_some(at % size)
    }

    /// Its RAM written: not a 340's, then.
    pub fn ram_written(&mut self) {
        self.not_340 = true;
        if self.chip == Chip::N340 {
            self.seems(Chip::Unknown);
        }
    }

    /// A read of $4800 to $5FFF: its own RAM's byte, or the IRQ counter.
    pub fn read(&mut self, addr: u16, chip_ram: &[u8]) -> Option<u8> {
        match addr & 0xF800 {
            0x4800 => {
                let value = chip_ram[self.sound.address as usize];
                if self.sound.increment {
                    self.sound.address = (self.sound.address + 1) & 0x7F;
                }
                Some(value)
            }
            0x5000 => Some(self.irq_counter as u8),
            0x5800 => Some((self.irq_counter >> 8) as u8),
            _ => None,
        }
    }

    /// A write to $4800 up, its own RAM `chip_ram`: the nametables, if a
    /// 340's are switched.
    pub fn write(&mut self, addr: u16, value: u8, chip_ram: &mut [u8]) -> Option<Mirroring> {
        let addr = addr & 0xF800;
        match addr {
            0x4800 => {
                self.seems(Chip::N163);
                chip_ram[self.sound.address as usize] = value;
                if self.sound.increment {
                    self.sound.address = (self.sound.address + 1) & 0x7F;
                }
            }
            0x5000 => {
                self.seems(Chip::N163);
                self.irq_counter = (self.irq_counter & 0xFF00) | value as u16;
                self.irq_held = false;
            }
            0x5800 => {
                self.seems(Chip::N163);
                self.irq_counter = (self.irq_counter & 0x00FF) | (value as u16) << 8;
                self.irq_held = false;
            }
            0x8000..=0xB800 => self.chr[(addr as usize - 0x8000) >> 11] = value,
            0xC000..=0xD800 => {
                if addr >= 0xC800 {
                    self.seems(Chip::N163);
                } else if self.chip != Chip::N163 {
                    self.seems(Chip::N175);
                }
                if self.chip == Chip::N175 {
                    self.write_protect = value;
                } else {
                    self.nametables[(addr as usize - 0xC000) >> 11] = Some(value);
                }
            }
            0xE000 => {
                if value & 0x80 != 0 || (value & 0x40 != 0 && self.chip != Chip::N163) {
                    self.seems(Chip::N340);
                }
                self.prg[0] = value & 0x3F;
                match self.chip {
                    Chip::N340 => {
                        self.nametables = [None; 4];
                        return Some(match value >> 6 {
                            0 => Mirroring::SingleLower,
                            1 => Mirroring::Vertical,
                            2 => Mirroring::Horizontal,
                            _ => Mirroring::SingleUpper,
                        });
                    }
                    Chip::N163 => self.sound.silent = value & 0x40 != 0,
                    _ => {}
                }
            }
            0xE800 => {
                self.prg[1] = value & 0x3F;
                if self.chip == Chip::N163 {
                    self.rom_above_e0 = [value & 0x40 != 0, value & 0x80 != 0];
                }
            }
            0xF000 => self.prg[2] = value & 0x3F,
            0xF800 => {
                self.seems(Chip::N163);
                if self.chip == Chip::N163 {
                    self.write_protect = value;
                    self.sound.address = value & 0x7F;
                    self.sound.increment = value & 0x80 != 0;
                }
            }
            _ => {}
        }
        None
    }

    /// A CPU cycle: its IRQ counter, if it's enabled, and short of the top;
    /// and a 163's sound, with its own RAM `chip_ram`.
    pub fn cycle(&mut self, chip_ram: &mut [u8]) {
        if self.irq_counter & 0x8000 != 0 && self.irq_counter & 0x7FFF != 0x7FFF {
            self.irq_counter += 1;
            self.irq_held |= self.irq_counter & 0x7FFF == 0x7FFF;
        }
        if self.chip == Chip::N163 && !self.sound.silent {
            self.sound.ticks += 1;
            if self.sound.ticks == 15 {
                self.sound.ticks = 0;
                self.sound.update(chip_ram);
            }
        }
    }

    /// Whether its IRQ line's held.
    pub fn irq(&self) -> bool {
        self.irq_held
    }

    /// The sound's output, as a share of the NES's own, as the Disk
    /// System's is.
    pub fn output(&self) -> f32 {
        self.sound.output as f32 * 0.004
    }
}

impl Sound {
    /// The next channel moved on through its wave, the output then; and
    /// on to the one before it, the highest again after the lowest.
    fn update(&mut self, ram: &mut [u8]) {
        let channels = (ram[0x7F] >> 4 & 7) as usize;
        let channel = self.channel as usize;
        let at = 0x40 + channel * 8;
        let frequency = (ram[at + 4] as u32 & 3) << 16 | (ram[at + 2] as u32) << 8 | ram[at] as u32;
        let phase = (ram[at + 5] as u32) << 16 | (ram[at + 3] as u32) << 8 | ram[at + 1] as u32;
        let length = 256 - (ram[at + 4] as u32 & 0xFC);
        let phase = (phase + frequency) % (length << 16);
        let place = ((phase >> 16) as usize + ram[at + 6] as usize) & 0xFF;
        let byte = ram[place / 2];
        let sample = if place & 1 != 0 {
            byte >> 4
        } else {
            byte & 0x0F
        };
        self.outputs[channel] = (sample as i16 - 8) * (ram[at + 7] & 0x0F) as i16;
        let sum: i16 = self.outputs[7 - channels..].iter().sum();
        self.output = sum / (channels as i16 + 1);
        [ram[at + 5], ram[at + 3], ram[at + 1]] =
            [(phase >> 16) as u8, (phase >> 8) as u8, phase as u8];
        self.channel = if channel <= 7 - channels {
            7
        } else {
            channel as u8 - 1
        };
    }
}
