//! Konami's VRCs: VRC2 and VRC4, mappers 21, 22, 23 and 25; VRC1, 75;
//! VRC3, 73; and VRC6, 24 and 26, with its sound. As Mesen has them.
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
//! VRC6's, 26's with A0 and A1 the other way round: $8000, the 16K at
//! $8000; $C000, the 8K at $C000, the last fixed at $E000. $D000 to $E003,
//! its eight CHR registers; $B003, how they're banked: bits 0 and 1, eight
//! 1K banks, four 2K, or four 1K and two 2K; bit 5, a 2K bank's two halves
//! its register's number's even and odd, or both its number. Then the
//! nametables, each a 1K page by registers 4 to 7: bits 2 and 3 which, as
//! vertical, horizontal, or one screen or the other with bit 5 set; and
//! with bit 4, those pages of CHR ROM, or without it, the PPU's RAM, the
//! pages' bit 0. Bit 7, its PRG RAM's enable. Its IRQ as VRC4's: $F000 its
//! latch, $F001 its control, $F002 acknowledging.
//!
//! VRC6's sound: two pulses ($9000 to $9002, $A000 to $A002), their
//! volume, their duty in 16ths (or bit 7, always on), a 12-bit period and
//! an enable; and a sawtooth ($B000 to $B002), a rate its accumulator's
//! added every other step of 14, a period and an enable. Each counts its
//! period down every CPU cycle, a step on going past 0. $9003: bit 0 halts
//! them all, bits 1 and 2 shift their periods 4 or 8 bits right. The
//! pulses' volumes and the sawtooth's accumulator's top 5 bits are added.
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

impl Irq {
    /// Its control written: bit 1 enabling it, the counter then reloaded;
    /// bit 2 counting CPU cycles, not lines; bit 0, its enabling again
    /// when acknowledged.
    fn control(&mut self, value: u8) {
        self.enabled_on_acknowledging = value & 0x01 != 0;
        self.enabled = value & 0x02 != 0;
        self.counting_cycles = value & 0x04 != 0;
        if self.enabled {
            self.counter = self.latch;
            self.prescaler = 341;
        }
        self.held = false;
    }

    fn acknowledge(&mut self) {
        self.enabled = self.enabled_on_acknowledging;
        self.held = false;
    }

    /// A CPU cycle: it or a line of them counted.
    fn cycle(&mut self) {
        if !self.enabled {
            return;
        }
        self.prescaler -= 3;
        if self.counting_cycles || self.prescaler <= 0 {
            if self.counter == 0xFF {
                self.counter = self.latch;
                self.held = true;
            } else {
                self.counter += 1;
            }
            self.prescaler += 341;
        }
    }
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
            0xF002 => self.irq.control(value),
            0xF003 => self.irq.acknowledge(),
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

    /// A CPU cycle: VRC4's IRQ counting.
    pub fn cycle(&mut self) {
        self.irq.cycle();
    }

    /// Whether its IRQ line's held.
    pub fn irq(&self) -> bool {
        self.irq.held
    }
}

/// VRC6's registers: whether A0 and A1 are the other way round, 26's; its
/// 16K and 8K banks, its CHR registers, how they're banked; its IRQ, and
/// its sound.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vrc6 {
    swapped: bool,
    prg: [u8; 2],
    chr: [u8; 8],
    mode: u8,
    irq: Irq,
    sound: Sound,
}

/// VRC6's sound: its two pulses and its sawtooth, whether they're halted,
/// and how far right their periods are shifted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Sound {
    pulses: [Pulse; 2],
    saw: Saw,
    halted: bool,
    shift: u8,
}

/// A pulse: its volume and duty, or always on; its period, enable, and
/// where it is in it; its step of 16.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Pulse {
    volume: u8,
    duty: u8,
    always: bool,
    period: u16,
    enabled: bool,
    timer: u16,
    step: u8,
}

/// The sawtooth: its rate, its accumulator; its period, enable, and where
/// it is in it; its step of 14.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Saw {
    rate: u8,
    accumulator: u8,
    period: u16,
    enabled: bool,
    timer: u16,
    step: u8,
}

/// How a period's counted: down every cycle, a step on going past 0, and
/// reloaded, shifted `shift` bits right. Whether it stepped.
fn count(timer: &mut u16, period: u16, shift: u8) -> bool {
    if *timer == 0 {
        *timer = period >> shift;
        true
    } else {
        *timer -= 1;
        false
    }
}

impl Pulse {
    /// Register `register`, 0 to 2, written.
    fn write(&mut self, register: u16, value: u8) {
        match register {
            0 => {
                self.volume = value & 0x0F;
                self.duty = (value >> 4) & 7;
                self.always = value & 0x80 != 0;
            }
            1 => self.period = (self.period & 0x0F00) | value as u16,
            _ => {
                self.period = (self.period & 0x00FF) | ((value & 0x0F) as u16) << 8;
                self.enabled = value & 0x80 != 0;
                if !self.enabled {
                    self.step = 0;
                }
            }
        }
    }

    fn cycle(&mut self, shift: u8) {
        if self.enabled && count(&mut self.timer, self.period, shift) {
            self.step = (self.step + 1) & 0x0F;
        }
    }

    fn output(&self) -> u8 {
        if self.enabled && (self.always || self.step <= self.duty) {
            self.volume
        } else {
            0
        }
    }
}

impl Saw {
    /// Register `register`, 0 to 2, written: disabled, its accumulator
    /// cleared and its steps started over.
    fn write(&mut self, register: u16, value: u8) {
        match register {
            0 => self.rate = value & 0x3F,
            1 => self.period = (self.period & 0x0F00) | value as u16,
            _ => {
                self.period = (self.period & 0x00FF) | ((value & 0x0F) as u16) << 8;
                self.enabled = value & 0x80 != 0;
                if !self.enabled {
                    (self.accumulator, self.step) = (0, 0);
                }
            }
        }
    }

    /// A cycle: at a step, the accumulator cleared at the first, its rate
    /// added at every other.
    fn cycle(&mut self, shift: u8) {
        if self.enabled && count(&mut self.timer, self.period, shift) {
            self.step = (self.step + 1) % 14;
            if self.step == 0 {
                self.accumulator = 0;
            } else if self.step & 1 == 0 {
                self.accumulator = self.accumulator.wrapping_add(self.rate);
            }
        }
    }

    fn output(&self) -> u8 {
        if self.enabled {
            self.accumulator >> 3
        } else {
            0
        }
    }
}

impl Vrc6 {
    /// 24's, or with A0 and A1 `swapped`, 26's.
    pub fn new(swapped: bool) -> Vrc6 {
        Vrc6 {
            swapped,
            ..Vrc6::default()
        }
    }

    /// Whether it's 26's.
    pub fn swapped(&self) -> bool {
        self.swapped
    }

    /// A write to `addr`, $8000 up.
    pub fn write(&mut self, addr: u16, value: u8) {
        let addr = if self.swapped {
            (addr & 0xFFFC) | (addr & 1) << 1 | (addr & 2) >> 1
        } else {
            addr
        };
        let register = addr & 3;
        match addr & 0xF003 {
            0x8000..=0x8003 => self.prg[0] = value & 0x0F,
            0x9003 => {
                self.sound.halted = value & 0x01 != 0;
                self.sound.shift = if value & 0x04 != 0 {
                    8
                } else if value & 0x02 != 0 {
                    4
                } else {
                    0
                };
            }
            0x9000..=0x9002 => self.sound.pulses[0].write(register, value),
            0xA000..=0xA002 => self.sound.pulses[1].write(register, value),
            0xB000..=0xB002 => self.sound.saw.write(register, value),
            0xB003 => self.mode = value,
            0xC000..=0xC003 => self.prg[1] = value & 0x1F,
            0xD000..=0xE003 => {
                self.chr[((addr >> 12) as usize - 0xD) * 4 + register as usize] = value;
            }
            0xF000 => self.irq.latch = value,
            0xF001 => self.irq.control(value),
            0xF002 => self.irq.acknowledge(),
            _ => {}
        }
    }

    /// Which 8K of its PRG ROM `addr`, $8000 up, is in, of `banks`.
    pub fn prg_bank(&self, addr: u16, banks: usize) -> usize {
        match addr {
            0x8000..=0xBFFF => (self.prg[0] as usize) << 1 | (addr as usize >> 13 & 1),
            0xC000..=0xDFFF => self.prg[1] as usize,
            _ => banks - 1,
        }
    }

    /// Which 1K of its CHR pattern table address `addr` is in.
    pub fn chr_bank(&self, addr: u16) -> usize {
        let slot = addr as usize >> 10 & 7;
        let chr = |register: usize| self.chr[register] as usize;
        // A 2K bank's halves: its number's even and odd, or both it.
        let half = |register: usize, odd: usize| {
            if self.mode & 0x20 != 0 {
                (chr(register) & !1) | odd
            } else {
                chr(register)
            }
        };
        match (self.mode & 3, slot) {
            (0, _) => chr(slot),
            (1, _) => half(slot >> 1, slot & 1),
            (_, 0..=3) => chr(slot),
            _ => half(4 + ((slot - 4) >> 1), slot & 1),
        }
    }

    /// Whether its PRG RAM's enabled.
    pub fn ram_on(&self) -> bool {
        self.mode & 0x80 != 0
    }

    /// Whether its nametables are CHR ROM's.
    pub fn rom_tables(&self) -> bool {
        self.mode & 0x10 != 0
    }

    /// The 1K page nametable address `addr` is: of CHR ROM, or of the
    /// PPU's RAM, its bit 0.
    pub fn table(&self, addr: u16) -> usize {
        let slot = (addr as usize >> 10) & 3;
        let chr = |register: usize| self.chr[register] as usize;
        let (even_6, even_7) = (chr(6) & !1, chr(7) & !1);
        match self.mode & 0x2F {
            // Vertical, horizontal, one screen or the other.
            0x20 | 0x27 => [even_6, even_6 | 1, even_7, even_7 | 1][slot],
            0x23 | 0x24 => [even_6, even_7, even_6 | 1, even_7 | 1][slot],
            0x28 | 0x2F => [even_6, even_6, even_7, even_7][slot],
            0x2B | 0x2C => [even_6 | 1, even_7 | 1, even_6 | 1, even_7 | 1][slot],
            _ => match self.mode & 7 {
                0 | 6 | 7 => [chr(6), chr(6), chr(7), chr(7)][slot],
                1 | 5 => [chr(4), chr(5), chr(6), chr(7)][slot],
                _ => [chr(6), chr(7), chr(6), chr(7)][slot],
            },
        }
    }

    /// A CPU cycle: its IRQ counting, and its sound, unless it's halted.
    pub fn cycle(&mut self) {
        self.irq.cycle();
        let sound = &mut self.sound;
        if !sound.halted {
            sound.pulses[0].cycle(sound.shift);
            sound.pulses[1].cycle(sound.shift);
            sound.saw.cycle(sound.shift);
        }
    }

    /// Whether its IRQ line's held.
    pub fn irq(&self) -> bool {
        self.irq.held
    }

    /// Its sound's output, as a share of the NES's own, as Mesen mixes it:
    /// 0 to about .9.
    pub fn output(&self) -> f32 {
        let sound = &self.sound;
        let level = sound.pulses[0].output() + sound.pulses[1].output() + sound.saw.output();
        level as f32 * 0.015
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
