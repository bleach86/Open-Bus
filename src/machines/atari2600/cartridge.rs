//! A 2600 cartridge: 4K of the CPU's address space, $1000 to $1FFF, so a
//! bigger game's banks are switched in by touching an address, a hotspot,
//! read or written: the bank switched before the read. The types, as Stella
//! has them and as it tells them apart (by size, and what's in the ROM):
//!
//! 2K, mirrored; 4K. F8, 8K: $1FF8 and $1FF9 the two 4K banks, starting in
//! the second; F6, 16K, $1FF6 to $1FF9, and F4, 32K, $1FF4 to $1FFB, each
//! starting in the first. With a Superchip (their 4K banks each starting
//! with 256 bytes the same 128 twice), 128 bytes of RAM, written at $1000
//! up and read at $1080 up.
//!
//! FA, CBS's RAM Plus, 12K: $1FF8 to $1FFA, starting in the third; 256 bytes
//! of RAM, written at $1000 up and read at $1100 up.
//!
//! E0, Parker Brothers': 8 banks of 1K, three slots of them at $1000, $1400
//! and $1800 switched by $1FE0 to $1FE7, $1FE8 to $1FEF and $1FF0 to $1FF7,
//! the last 1K fixed at $1C00.
//!
//! FE, Activision's: 2 banks of 4K, switched by what a JSR or RTS does: the
//! access after one at $01FE, the stack's, its byte's top 3 bits, flipped.
//!
//! 3F, Tigervision's: banks of 2K at $1000, written at $00 to $3F (the
//! TIA's, which takes the write too), the last fixed at $1800.
//!
//! E7, M Network's: banks of 2K at $1000, $1FE0 to $1FE6, or $1FE7 for its
//! 1K of RAM there instead (written at $1000, read at $1400); the last bank
//! at $1800, but for $1800 to $19FF, a 256-byte bank of RAM (written at
//! $1800, read at $1900), of 4, $1FE8 to $1FEB.
//!
//! DPC, Pitfall II's: F8, and a chip with 2K of graphics ROM of its own, read
//! through 8 counters, its data fetchers. Written at $1040 to $107F, each
//! fetcher's top, bottom, and counter's low and high bits (5 to 7, bit 4
//! of the high its music mode); read at $1000 to $103F, its byte (counting
//! it down), that ANDed with its flag (set at its top, cleared at its
//! bottom), its flag; the first 4's function 0 a random number, stepped at
//! every access of the cartridge, and the last 3's in music mode, counted
//! by its own 20 kHz clock, from their tops round, read together as a
//! volume, its tune.

use serde::{Deserialize, Serialize};

/// The types of board.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    #[default]
    K4,
    F8,
    F6,
    F4,
    Fa,
    E0,
    Fe,
    Tigervision,
    E7,
    Dpc,
}

impl Kind {
    /// The type Stella's properties name `name`, if it's one that's here:
    /// none for none named, an error for one that's not here.
    pub fn named(name: &str) -> Result<Option<Kind>, String> {
        Ok(Some(match name {
            "" => return Ok(None),
            "2K" | "4K" => Kind::K4,
            "F8" | "F8SC" => Kind::F8,
            "F6" | "F6SC" => Kind::F6,
            "F4" | "F4SC" => Kind::F4,
            "FA" => Kind::Fa,
            "E0" => Kind::E0,
            "FE" => Kind::Fe,
            "3F" => Kind::Tigervision,
            "E7" => Kind::E7,
            "DPC" => Kind::Dpc,
            _ => return Err(format!("its cartridge's type, {name}, isn't here yet")),
        }))
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::K4 => "4K",
            Kind::F8 => "F8",
            Kind::F6 => "F6",
            Kind::F4 => "F4",
            Kind::Fa => "FA",
            Kind::E0 => "E0",
            Kind::Fe => "FE",
            Kind::Tigervision => "3F",
            Kind::E7 => "E7",
            Kind::Dpc => "DPC",
        }
    }
}

/// Its ROM aren't in a save state: the machine it's loaded into has it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cartridge {
    #[serde(skip)]
    pub rom: Vec<u8>,
    #[serde(skip)]
    pub kind: Kind,
    /// Whether it has a Superchip.
    #[serde(skip)]
    pub superchip: bool,
    pub ram: Vec<u8>,
    /// Its slots' banks: a 4K bank's, or E0's three 1K, or E7's 2K and its
    /// RAM's 256.
    banks: [u16; 3],
    /// FE's: whether the last access it saw was to $01FE.
    after_01fe: bool,
    /// The bank it starts in, if a game's said to start in another.
    #[serde(skip)]
    pub start_bank: Option<u16>,
    /// DPC's chip.
    dpc: Option<Box<Dpc>>,
}

/// The DPC's oscillator, for its music.
const DPC_CLOCK_HZ: u64 = 20_000;

/// The DPC chip: its data fetchers, their tops, bottoms, counters and
/// flags; the last 3's music modes; its random number; the CPU's cycles, the
/// CPU's clock, the cycles its oscillator was last counted at and what's
/// over of a count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dpc {
    tops: [u8; 8],
    bottoms: [u8; 8],
    counters: [u16; 8],
    flags: [u8; 8],
    music: [bool; 3],
    random: u8,
    cycles: u64,
    clock_hz: u64,
    counted_at: u64,
    remainder: u64,
}

impl Dpc {
    fn new(clock_hz: u32) -> Dpc {
        Dpc {
            tops: [0; 8],
            bottoms: [0; 8],
            counters: [0; 8],
            flags: [0; 8],
            music: [false; 3],
            random: 1,
            cycles: 0,
            clock_hz: clock_hz as u64,
            counted_at: 0,
            remainder: 0,
        }
    }

    fn step_random(&mut self) {
        const BITS: [u8; 16] = [1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1];
        let at = ((self.random >> 3) & 7) | (self.random & 0x80) >> 4;
        self.random = self.random << 1 | BITS[at as usize];
    }

    /// The music's fetchers counted for the oscillator's clocks since they
    /// last were, each down from its top round, its flag by its bottom.
    fn count_music(&mut self) {
        let elapsed = (self.cycles - self.counted_at) * DPC_CLOCK_HZ + self.remainder;
        self.counted_at = self.cycles;
        let clocks = elapsed / self.clock_hz;
        self.remainder = elapsed % self.clock_hz;
        if clocks == 0 {
            return;
        }
        for x in 5..8 {
            if !self.music[x - 5] {
                continue;
            }
            let top = self.tops[x] as i64 + 1;
            let mut low = (self.counters[x] & 0xFF) as i64;
            if self.tops[x] != 0 {
                low -= clocks as i64 % top;
                if low < 0 {
                    low += top;
                }
            } else {
                low = 0;
            }
            if low <= self.bottoms[x] as i64 {
                self.flags[x] = 0x00;
            } else if low <= self.tops[x] as i64 {
                self.flags[x] = 0xFF;
            }
            self.counters[x] = (self.counters[x] & 0x0700) | low as u16;
        }
    }

    /// A read of $1000 to $103F, `graphics` its 2K.
    fn read(&mut self, at: u16, graphics: &[u8]) -> u8 {
        let index = at as usize & 7;
        let function = (at >> 3) & 7;
        let low = (self.counters[index] & 0xFF) as u8;
        if low == self.tops[index] {
            self.flags[index] = 0xFF;
        } else if low == self.bottoms[index] {
            self.flags[index] = 0x00;
        }
        let byte = graphics[2047 - self.counters[index] as usize];
        let value = match function {
            0 if index < 4 => self.random,
            0 => {
                const AMPLITUDES: [u8; 8] = [0x00, 0x04, 0x05, 0x09, 0x06, 0x0A, 0x0B, 0x0F];
                self.count_music();
                let on = (0..3).fold(0, |on, n| {
                    on | ((self.music[n] && self.flags[5 + n] != 0) as usize) << n
                });
                AMPLITUDES[on]
            }
            1 => byte,
            2 => byte & self.flags[index],
            7 => self.flags[index],
            _ => 0,
        };
        if index < 5 || !self.music[index - 5] {
            self.counters[index] = self.counters[index].wrapping_sub(1) & 0x07FF;
        }
        value
    }

    /// A write of $1040 to $107F.
    fn write(&mut self, at: u16, value: u8) {
        let index = at as usize & 7;
        match (at >> 3) & 7 {
            0 => (self.tops[index], self.flags[index]) = (value, 0),
            1 => self.bottoms[index] = value,
            2 => {
                let low = if index >= 5 && self.music[index - 5] {
                    self.tops[index]
                } else {
                    value
                };
                self.counters[index] = (self.counters[index] & 0x0700) | low as u16;
            }
            3 => {
                self.counters[index] = ((value as u16 & 7) << 8) | (self.counters[index] & 0xFF);
                if index >= 5 {
                    self.music[index - 5] = value & 0x10 != 0;
                }
            }
            6 => self.random = 1,
            _ => {}
        }
    }
}

/// Whether `image` has `pattern` in it, `hits` times at least.
fn has(image: &[u8], pattern: &[u8], hits: usize) -> bool {
    image
        .windows(pattern.len())
        .filter(|window| *window == pattern)
        .take(hits)
        .count()
        >= hits
}

/// Whether any of `patterns` is in `image`.
fn has_any(image: &[u8], patterns: &[[u8; 3]]) -> bool {
    patterns.iter().any(|pattern| has(image, pattern, 1))
}

/// Whether each 4K of `image` starts with 128 bytes that are the same as
/// the next: a Superchip's RAM, which the ROM under it can't be read at.
fn superchip(image: &[u8]) -> bool {
    image
        .chunks(0x1000)
        .all(|bank| bank.len() >= 256 && bank[..128] == bank[128..256])
}

fn e0(image: &[u8]) -> bool {
    has_any(
        image,
        &[
            [0x8D, 0xE0, 0x1F],
            [0x8D, 0xE0, 0x5F],
            [0x8D, 0xE9, 0xFF],
            [0x0C, 0xE0, 0x1F],
            [0xAD, 0xE0, 0x1F],
            [0xAD, 0xE9, 0xFF],
            [0xAD, 0xED, 0xFF],
            [0xAD, 0xF3, 0xBF],
        ],
    )
}

fn fe(image: &[u8]) -> bool {
    [
        [0x20, 0x00, 0xD0, 0xC6, 0xC5],
        [0x20, 0xC3, 0xF8, 0xA5, 0x82],
        [0xD0, 0xFB, 0x20, 0x73, 0xFE],
        [0xD0, 0xFB, 0x20, 0x68, 0xFE],
        [0x20, 0x00, 0xF0, 0x84, 0xD6],
    ]
    .iter()
    .any(|pattern| has(image, pattern, 1))
}

/// Tigervision's: STA $3F, twice.
fn tigervision(image: &[u8]) -> bool {
    has(image, &[0x85, 0x3F], 2)
}

fn e7(image: &[u8]) -> bool {
    has_any(
        image,
        &[
            [0xAD, 0xE2, 0xFF],
            [0xAD, 0xE5, 0xFF],
            [0xAD, 0xE5, 0x1F],
            [0xAD, 0xE7, 0x1F],
            [0x0C, 0xE7, 0x1F],
            [0x8D, 0xE7, 0xFF],
            [0x8D, 0xE7, 0x1F],
        ],
    )
}

/// E7's on 8K, its four banks at $1FE4 to $1FE7.
fn e7_8k(image: &[u8]) -> bool {
    has_any(
        image,
        &[[0xAD, 0xE4, 0xFF], [0xAD, 0xE5, 0xFF], [0xAD, 0xE6, 0xFF]],
    )
}

/// What an 8K F8 game does to switch: STA $1FF9, twice.
fn f8(image: &[u8]) -> bool {
    has(image, &[0x8D, 0xF9, 0x1F], 2) || has(image, &[0x8D, 0xF9, 0xFF], 2)
}

impl Cartridge {
    /// The cartridge of `image`, its type as `kind` says, or as its size
    /// and what's in it say; on a console whose CPU's clock is `clock_hz`.
    pub fn new(image: &[u8], kind: Option<Kind>, clock_hz: u32) -> Result<Cartridge, String> {
        let size = image.len();
        let detected = match size {
            0x800 | 0x1000 => Kind::K4,
            0x2000 if image[..0x1000] == image[0x1000..] => Kind::K4,
            0x2000 if superchip(image) => Kind::F8,
            0x2000 if e0(image) => Kind::E0,
            0x2000 if tigervision(image) => Kind::Tigervision,
            0x2000 if fe(image) && !f8(image) => Kind::Fe,
            0x2000 if e7_8k(image) => Kind::E7,
            0x2000 => Kind::F8,
            0x3000 if e7(image) => Kind::E7,
            0x3000 => Kind::Fa,
            0x4000 if !superchip(image) && e7(image) => Kind::E7,
            0x4000 => Kind::F6,
            0x8000 if !superchip(image) && tigervision(image) => Kind::Tigervision,
            0x8000 => Kind::F4,
            0x2800..=0x2900 => Kind::Dpc,
            _ => return Err(format!("a {} byte ROM, of no type that's here", size)),
        };
        let kind = kind.unwrap_or(detected);
        let superchip = matches!(kind, Kind::F8 | Kind::F6 | Kind::F4) && superchip(image);
        let ram = match kind {
            _ if superchip => 128,
            Kind::Fa => 256,
            Kind::E7 => 2048,
            _ => 0,
        };
        // A 2K ROM's the same 2K twice.
        let rom = if size == 0x800 {
            [image, image].concat()
        } else {
            image.to_vec()
        };
        let mut cartridge = Cartridge {
            rom,
            kind,
            superchip,
            ram: vec![0; ram],
            banks: [0; 3],
            after_01fe: false,
            start_bank: None,
            dpc: (kind == Kind::Dpc).then(|| Box::new(Dpc::new(clock_hz))),
        };
        cartridge.reset();
        Ok(cartridge)
    }

    /// Its type, with a Superchip's SC, and its size: "F8SC 8K"; a plain
    /// 2K or 4K's, its size alone.
    pub fn name(&self) -> String {
        let size = self.rom.len() / 1024;
        match self.kind {
            Kind::K4 => format!("{size}K"),
            kind => {
                let superchip = if self.superchip { "SC" } else { "" };
                format!("{}{superchip} {size}K", kind.name())
            }
        }
    }

    /// Its banks of `size`.
    fn bank_count(&self, size: usize) -> u16 {
        (self.rom.len() / size).max(1) as u16
    }

    /// At power-on, or with the console's own reset: its banks as they
    /// start.
    pub fn reset(&mut self) {
        self.banks = match self.kind {
            Kind::F8 | Kind::Dpc => [1, 0, 0],
            Kind::Fa => [2, 0, 0],
            Kind::E0 => [4, 5, 6],
            _ => [0; 3],
        };
        if let Some(bank) = self.start_bank {
            self.banks[0] = bank;
        }
        self.after_01fe = false;
    }

    /// An access of `addr` (13 bits), its data `value`, read or `written`,
    /// for what it watches beyond its own addresses: FE's stack, 3F's TIA.
    pub fn access(&mut self, addr: u16, value: u8, written: bool) {
        match self.kind {
            Kind::Fe if addr & 0x1000 != 0 || (0x01C0..0x0200).contains(&addr) => {
                if self.after_01fe {
                    self.banks[0] = ((value >> 5) ^ 0x07) as u16 % self.bank_count(0x1000);
                    self.after_01fe = false;
                } else {
                    self.after_01fe = addr == 0x01FE;
                }
            }
            Kind::Tigervision if written && addr <= 0x3F => {
                self.banks[0] = value as u16 % self.bank_count(0x800);
            }
            _ => {}
        }
    }

    /// A hotspot at `at`, $000 to $FFF: its bank switched.
    fn hotspot(&mut self, at: u16) {
        let at = at | 0x1000;
        match self.kind {
            Kind::F8 | Kind::Dpc if (0x1FF8..=0x1FF9).contains(&at) => self.banks[0] = at - 0x1FF8,
            Kind::F6 if (0x1FF6..=0x1FF9).contains(&at) => self.banks[0] = at - 0x1FF6,
            Kind::F4 if (0x1FF4..=0x1FFB).contains(&at) => self.banks[0] = at - 0x1FF4,
            Kind::Fa if (0x1FF8..=0x1FFA).contains(&at) => self.banks[0] = at - 0x1FF8,
            Kind::E0 if (0x1FE0..=0x1FF7).contains(&at) => {
                self.banks[(at as usize - 0x1FE0) / 8] = at & 7;
            }
            Kind::E7 => {
                let banks = self.bank_count(0x800);
                match (banks, at) {
                    (4, 0x1FE4..=0x1FE7) => self.banks[0] = at & 3,
                    (6, 0x1FE0..=0x1FE7) => {
                        self.banks[0] = [0, 1, 0, 1, 2, 3, 4, 5][at as usize & 7]
                    }
                    (8, 0x1FE0..=0x1FE7) => self.banks[0] = at & 7,
                    (_, 0x1FE8..=0x1FEB) => self.banks[1] = at & 3,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Its RAM's place for `at`, $000 to $FFF, and whether that's its write
    /// half, if it's RAM there.
    fn ram_at(&self, at: u16) -> Option<(usize, bool)> {
        let at = at as usize;
        match self.kind {
            Kind::F8 | Kind::F6 | Kind::F4 if self.superchip && at < 0x100 => {
                Some((at & 0x7F, at < 0x80))
            }
            Kind::Fa if at < 0x200 => Some((at & 0xFF, at < 0x100)),
            Kind::E7 => {
                let ram_bank = self.bank_count(0x800) - 1;
                if self.banks[0] == ram_bank && at < 0x800 {
                    Some((at & 0x3FF, at < 0x400))
                } else if (0x800..0xA00).contains(&at) {
                    Some((
                        0x400 + (self.banks[1] as usize) * 0x100 + (at & 0xFF),
                        at < 0x900,
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Where `at`, $000 to $FFF, is in its ROM.
    fn rom_at(&self, at: u16) -> usize {
        let at = at as usize;
        let offset = match self.kind {
            Kind::E0 if at < 0xC00 => self.banks[at >> 10] as usize * 0x400 + (at & 0x3FF),
            Kind::E0 => 7 * 0x400 + (at & 0x3FF),
            Kind::Tigervision | Kind::E7 if at < 0x800 => {
                self.banks[0] as usize * 0x800 + (at & 0x7FF)
            }
            Kind::Tigervision | Kind::E7 => self.rom.len() - 0x800 + (at & 0x7FF),
            _ => self.banks[0] as usize * 0x1000 + at,
        };
        match self.kind {
            Kind::Dpc => offset % 0x2000,
            _ => offset % self.rom.len(),
        }
    }

    /// A CPU cycle, for the DPC's oscillator.
    pub fn cycle(&mut self) {
        if let Some(dpc) = &mut self.dpc {
            dpc.cycles += 1;
        }
    }

    /// A read of $1000 to $1FFF: a hotspot's bank switched first. Of RAM's
    /// write half, nothing drives the bus but the RAM's written anyway, with
    /// what's left on it: the address's high byte, an absolute read's last.
    pub fn read(&mut self, addr: u16) -> u8 {
        let at = addr & 0x0FFF;
        if let Some(dpc) = &mut self.dpc {
            dpc.step_random();
            if at < 0x40 {
                return dpc.read(at, &self.rom[0x2000..0x2800]);
            }
        }
        self.hotspot(at);
        match self.ram_at(at) {
            Some((place, false)) => self.ram[place],
            Some((place, true)) => {
                self.ram[place] = (addr >> 8) as u8;
                self.ram[place]
            }
            None => self.rom[self.rom_at(at)],
        }
    }

    /// A write of $1000 to $1FFF: a hotspot's, or its RAM's.
    pub fn write(&mut self, addr: u16, value: u8) {
        let at = addr & 0x0FFF;
        if let Some(dpc) = &mut self.dpc {
            dpc.step_random();
            if (0x40..0x80).contains(&at) {
                dpc.write(at, value);
                return;
            }
        }
        self.hotspot(at);
        if let Some((place, true)) = self.ram_at(at) {
            self.ram[place] = value;
        }
    }

    /// The ROM taken from `running`, the one a state's loaded into.
    pub fn take_rom(&mut self, running: &mut Cartridge) {
        self.rom = std::mem::take(&mut running.rom);
        self.kind = running.kind;
        self.superchip = running.superchip;
        self.start_bank = running.start_bank;
    }
}
