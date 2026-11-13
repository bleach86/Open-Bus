//! POKEY, Atari's C012294, as a7800 has it (from MAME's, GPL-2 or later, its
//! polynomials after Altirra's): the sound of the 7800's cartridges that
//! have one, and in time the Atari computers' keyboard, serial port and
//! paddles too, which aren't here yet.
//!
//! Its four channels, each a divider (AUDF) counted down by its clock and a
//! control (AUDC): a volume, and what its output is each time the divider
//! comes round: a pure tone, toggled; or the bit of a polynomial counter,
//! 4, 9 or 17 bits (AUDCTL's bit 7 says 9 or 17), each gated by the 5-bit
//! one or not; or with VOLUME_ONLY, the volume, steady. AUDCTL says each
//! channel's clock: 64 kHz, or 15 kHz, or for channels 1 and 3 the CPU's
//! 1.79 MHz; and channels joined, 2 counted by 1 and 4 by 3, as 16 bits;
//! and channels 1 and 2 high-passed, sampled as 3 and 4 come round. A
//! counter's borrow takes its cycles to come through: 4 at 1.79 MHz, 7
//! joined, 1 otherwise, the new divider taken only then.
//!
//! SKCTL's two low bits 0 hold it in reset, its counters and polynomials
//! stopped. Timers 1, 2 and 4 raise IRQST's bits, as IRQEN lets them;
//! RANDOM is the polynomial's top 8 bits.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// AUDC's bits.
const NOT_POLY5: u8 = 0x80;
const POLY4: u8 = 0x40;
const PURE: u8 = 0x20;
const VOLUME_ONLY: u8 = 0x10;
/// AUDCTL's bits.
const POLY9: u8 = 0x80;
const CH1_FAST: u8 = 0x40;
const CH3_FAST: u8 = 0x20;
const CH12_JOINED: u8 = 0x10;
const CH34_JOINED: u8 = 0x08;
const CH1_FILTER: u8 = 0x04;
const CH2_FILTER: u8 = 0x02;
const CLOCK_15KHZ: u8 = 0x01;
/// SKCTL's: two-tone mode, and the two bits that, both 0, hold it in reset.
const TWO_TONE: u8 = 0x08;
const NOT_RESET: u8 = 0x03;
/// The 1.79 MHz clock divided for 64 kHz and for 15 kHz.
const DIVISORS: [u32; 3] = [1, 28, 114];
/// The timers' IRQ bits, each channel's: channel 3's none.
const TIMER_IRQS: [u8; 4] = [0x01, 0x02, 0x00, 0x04];
/// IRQST's serial output complete bit, set from the start.
const SERIAL_OUT_COMPLETE: u8 = 0x08;
/// The output's gain, as a7800 has it: a sum of the four volumes, 0 to 60,
/// as 0 to 1, louder than 60's share and clipped.
const GAIN: f32 = 11.0 * 4.0;
/// The output for each sum of the volumes: worked out once, not each cycle.
const LEVELS: [f32; 61] = {
    let mut levels = [0.0; 61];
    let mut sum = 0;
    while sum < 61 {
        levels[sum] = (sum as f32 / GAIN).min(1.0);
        sum += 1;
    }
    levels
};

/// The polynomial counters' sequences: 4 and 5 bits, as Altirra has them;
/// 9 and 17, as MAME's, RANDOM in the 17's bits 8 to 15.
struct Polys {
    poly4: Vec<u8>,
    poly5: Vec<u8>,
    poly9: Vec<u32>,
    poly17: Vec<u32>,
}

fn polys() -> &'static Polys {
    static POLYS: OnceLock<Polys> = OnceLock::new();
    POLYS.get_or_init(|| {
        let short = |size: u32, tap: u32| {
            let mask = (1 << size) - 1;
            let mut lfsr: u32 = 0;
            (0..mask)
                .map(|_| {
                    lfsr = (lfsr << 1) + (!((lfsr >> 2) ^ (lfsr >> tap)) & 1);
                    (lfsr & mask) as u8
                })
                .collect()
        };
        let mut lfsr: u32 = 0x1FF;
        let poly9 = (0..0x1FF)
            .map(|_| {
                let bit = (lfsr & 1) ^ ((lfsr >> 5) & 1);
                lfsr = (bit << 8) | (lfsr >> 1);
                lfsr
            })
            .collect();
        let mut lfsr: u32 = 0x1FFFF;
        let poly17 = (0..0x1FFFF)
            .map(|_| {
                let bit_8 = ((lfsr >> 8) & 1) ^ ((lfsr >> 13) & 1);
                let bit = lfsr & 1;
                lfsr >>= 1;
                lfsr = (lfsr & 0xFF7F) | (bit_8 << 7);
                lfsr |= bit << 16;
                lfsr
            })
            .collect();
        Polys {
            poly4: short(4, 3),
            poly5: short(5, 4),
            poly9,
            poly17,
        }
    })
}

/// A polynomial counter's place, `at`, a clock on: round again after
/// `length`.
fn step(at: u32, length: u32) -> u32 {
    if at + 1 == length { 0 } else { at + 1 }
}

/// The clocks counted to a tick, a clock on: whether it's come, each
/// `divisor` of them.
fn tick(clocks: &mut u32, divisor: u32) -> bool {
    *clocks += 1;
    if *clocks >= divisor {
        *clocks = 0;
        return true;
    }
    false
}

/// The volumes' sum not yet worked out, as a state's just been loaded.
fn unknown() -> u8 {
    UNKNOWN
}
const UNKNOWN: u8 = 0xFF;

/// A channel: its registers, its 8-bit counter counting up to its borrow,
/// the borrow's cycles still to come through, its output and its
/// high-pass filter's sample.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Channel {
    audf: u8,
    audc: u8,
    counter: u8,
    borrow: u8,
    output: bool,
    filter_sample: bool,
}

impl Channel {
    /// Counted from its divider again.
    fn reset(&mut self) {
        self.counter = self.audf ^ 0xFF;
        self.borrow = 0;
    }

    /// Counted on a clock: whether it's come round, its borrow to come
    /// through in `cycles`.
    fn count(&mut self, cycles: u8) -> bool {
        self.counter = self.counter.wrapping_add(1);
        if self.counter == 0 && self.borrow == 0 {
            self.borrow = cycles;
            return true;
        }
        false
    }

    /// Whether its borrow's come through, this clock.
    fn borrowed(&mut self) -> bool {
        if self.borrow == 0 {
            return false;
        }
        self.borrow -= 1;
        self.borrow == 0
    }

    /// Its volume, 0 to 15, if it's sounding.
    fn volume(&self) -> u8 {
        if (self.output ^ self.filter_sample) || self.audc & VOLUME_ONLY != 0 {
            self.audc & 0x0F
        } else {
            0
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pokey {
    channels: [Channel; 4],
    audctl: u8,
    skctl: u8,
    irqen: u8,
    /// IRQST's bits, set high here, though it reads them low.
    irqst: u8,
    /// The clocks counted to the 1.79 MHz, 64 kHz and 15 kHz ticks: the
    /// first's each clock, its count always 0.
    clocks: [u32; 3],
    /// Where each polynomial counter is.
    p4: u32,
    p5: u32,
    p9: u32,
    p17: u32,
    /// The channels' volumes summed, kept as they change, not each clock:
    /// UNKNOWN till they do, a state loaded.
    #[serde(skip, default = "unknown")]
    volumes: u8,
}

impl Pokey {
    pub fn new() -> Pokey {
        polys();
        let channel = Channel {
            audf: 0,
            audc: 0xB0,
            counter: 0,
            borrow: 0,
            output: false,
            filter_sample: false,
        };
        Pokey {
            channels: std::array::from_fn(|_| channel.clone()),
            audctl: 0,
            skctl: 0,
            irqen: 0,
            irqst: SERIAL_OUT_COMPLETE,
            clocks: [0; 3],
            p4: 0,
            p5: 0,
            p9: 0,
            p17: 0,
            volumes: 0,
        }
    }

    /// Its channels' volumes summed, 0 to 60.
    fn volumes(&self) -> u8 {
        self.channels.iter().map(Channel::volume).sum()
    }

    /// Channel `n` counted on a clock, its timer's IRQ raised if it's come
    /// round and IRQEN lets it.
    fn count(&mut self, n: usize, cycles: u8) {
        if self.channels[n].count(cycles) && self.irqen & TIMER_IRQS[n] != 0 {
            self.irqst |= TIMER_IRQS[n];
        }
    }

    /// Channel `n`'s output, as it comes round: toggled, or a polynomial's
    /// bit, if the 5-bit one lets it.
    fn come_round(&mut self, n: usize) {
        let polys = polys();
        let audc = self.channels[n].audc;
        if audc & NOT_POLY5 == 0 && polys.poly5[self.p5 as usize] & 1 == 0 {
            return;
        }
        let channel = &mut self.channels[n];
        channel.output = if audc & PURE != 0 {
            !channel.output
        } else if audc & POLY4 != 0 {
            polys.poly4[self.p4 as usize] & 1 != 0
        } else if self.audctl & POLY9 != 0 {
            polys.poly9[self.p9 as usize] & 1 != 0
        } else {
            polys.poly17[self.p17 as usize] & 1 != 0
        };
    }

    /// A clock of its 1.79 MHz.
    pub fn clock(&mut self) {
        let audctl = self.audctl;
        let (joined_12, joined_34) = (audctl & CH12_JOINED != 0, audctl & CH34_JOINED != 0);
        if self.skctl & NOT_RESET != 0 {
            let khz64 = tick(&mut self.clocks[1], DIVISORS[1]);
            let khz15 = tick(&mut self.clocks[2], DIVISORS[2]);
            let base = if audctl & CLOCK_15KHZ != 0 {
                khz15
            } else {
                khz64
            };
            self.p4 = step(self.p4, 0xF);
            self.p5 = step(self.p5, 0x1F);
            self.p9 = step(self.p9, 0x1FF);
            self.p17 = step(self.p17, 0x1FFFF);
            for (n, fast, joined) in [(0, CH1_FAST, joined_12), (2, CH3_FAST, joined_34)] {
                if audctl & fast != 0 {
                    self.count(n, if joined { 7 } else { 4 });
                } else if base {
                    self.count(n, 1);
                }
            }
            if base {
                if !joined_12 {
                    self.count(1, 1);
                }
                if !joined_34 {
                    self.count(3, 1);
                }
            }
        }

        // Most clocks, no borrow's coming through.
        if self.channels.iter().all(|channel| channel.borrow == 0) {
            return;
        }
        let mut changed = false;
        if self.channels[2].borrowed() {
            changed = true;
            if joined_34 {
                self.count(3, 1);
            } else {
                self.channels[2].reset();
            }
            self.come_round(2);
            let first = &mut self.channels[0];
            first.filter_sample = audctl & CH1_FILTER == 0 || first.output;
        }
        if self.channels[3].borrowed() {
            changed = true;
            if joined_34 {
                self.channels[2].reset();
            }
            self.channels[3].reset();
            self.come_round(3);
            let second = &mut self.channels[1];
            second.filter_sample = audctl & CH2_FILTER == 0 || second.output;
        }
        if self.skctl & TWO_TONE != 0 && self.channels[1].borrow == 1 {
            self.channels[0].reset();
        }
        if self.channels[0].borrowed() {
            changed = true;
            if joined_12 {
                self.count(1, 1);
            } else {
                self.channels[0].reset();
            }
            self.come_round(0);
        }
        if self.channels[1].borrowed() {
            changed = true;
            if joined_12 {
                self.channels[0].reset();
            }
            self.channels[1].reset();
            self.come_round(1);
        }
        if changed {
            self.volumes = self.volumes();
        }
    }

    /// Its output, its four channels' volumes summed, 0 to 1.
    pub fn output(&self) -> f32 {
        let sum = match self.volumes {
            UNKNOWN => self.volumes(),
            volumes => volumes,
        };
        LEVELS[sum as usize]
    }

    /// Whether its IRQ line's held: a timer come round, as IRQEN lets it.
    pub fn irq(&self) -> bool {
        self.irqst & self.irqen != 0
    }

    /// A read of register `register`, $0 to $F: RANDOM, IRQST (its bits
    /// low), or what isn't here yet, $FF.
    pub fn read(&self, register: u8) -> u8 {
        match register & 0x0F {
            0x0A => {
                let polys = polys();
                if self.audctl & POLY9 != 0 {
                    polys.poly9[self.p9 as usize] as u8
                } else {
                    (polys.poly17[self.p17 as usize] >> 8) as u8
                }
            }
            0x0E => self.irqst ^ 0xFF,
            _ => 0xFF,
        }
    }

    /// A write of register `register`, $0 to $F.
    pub fn write(&mut self, register: u8, value: u8) {
        match register & 0x0F {
            n @ 0x0..=0x7 => {
                let channel = &mut self.channels[n as usize / 2];
                if n % 2 == 0 {
                    channel.audf = value;
                } else {
                    channel.audc = value;
                    self.volumes = self.volumes();
                }
            }
            0x8 => self.audctl = value,
            // STIMER: the counters from their dividers again, outputs low.
            0x9 => {
                for (n, channel) in self.channels.iter_mut().enumerate() {
                    channel.reset();
                    channel.output = false;
                    channel.filter_sample = n < 2;
                }
                self.volumes = self.volumes();
            }
            // IRQEN: what it masks cleared, serial output complete aside.
            0xE => {
                self.irqst &= SERIAL_OUT_COMPLETE | value;
                self.irqen = value;
            }
            0xF if value != self.skctl => {
                self.skctl = value;
                if value & NOT_RESET == 0 {
                    self.irqst &= SERIAL_OUT_COMPLETE;
                    self.irqen = 0;
                    (self.p4, self.p5, self.p9, self.p17) = (0, 0, 0, 0);
                    self.clocks = [0; 3];
                }
            }
            _ => {}
        }
    }
}

impl Default for Pokey {
    fn default() -> Pokey {
        Pokey::new()
    }
}
