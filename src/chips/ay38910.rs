//! The General Instrument AY-3-8910: three square-wave tone channels, a
//! noise generator, a mixer, and each channel's volume, fixed or the
//! envelope's; its 16 registers reached by its bus, a register's number
//! latched, then its byte written or read.
//!
//! Its clock divided by 8 steps it. Each tone's counter counts to its
//! period (12 bits; 0 as 1), then flips its square wave: a tone of clock /
//! (16 x period). The noise's counts to its period (5 bits), and every
//! other time shifts a 17-bit shift register, bits 0 and 3 XORed in at the
//! top: bit 0 the noise. The envelope's counts to twice its period (16
//! bits), then takes a step of its 16: a cycle of clock / (256 x period),
//! its shape from register 13's four bits, continue, attack, alternate and
//! hold. A channel sounds where the mixer lets its tone and its noise
//! through (each enabled by a 0), or has either off: its volume, or the
//! envelope's, the level of its DAC, 3 dB a step.

use serde::{Deserialize, Serialize};

/// Each register's bits.
const MASKS: [u8; 16] = [
    0xFF, 0x0F, 0xFF, 0x0F, 0xFF, 0x0F, 0x1F, 0xFF, 0x1F, 0x1F, 0x1F, 0xFF, 0xFF, 0x0F, 0xFF, 0xFF,
];

const MIXER: usize = 7;
const ENVELOPE_SHAPE: usize = 13;

/// The DAC's 16 levels, as parts of its most: 3 dB apart, a factor of the
/// square root of 2; 0, nothing.
pub const LEVELS: [f32; 16] = [
    0.0,
    0.007_812_5,
    0.011_048_5,
    0.015_625,
    0.022_097,
    0.031_25,
    0.044_194,
    0.062_5,
    0.088_388,
    0.125,
    0.176_777,
    0.25,
    0.353_553,
    0.5,
    0.707_107,
    1.0,
];

#[derive(Serialize, Deserialize)]
pub struct Ay38910 {
    pub regs: [u8; 16],
    /// The register latched, for the bus's next write or read: over 15,
    /// none of its own.
    address: u8,
    /// Its clock, to its next step, an eighth of it.
    prescale: u8,
    tone_count: [u16; 3],
    tone_high: [bool; 3],
    noise_count: u8,
    noise_shift: bool,
    noise: u32,
    // The envelope: its count to its next step; its step, 15 down to 0;
    // what its steps are XORed with, 15 for an attack; whether at its end
    // it holds, alternates, or holds already.
    envelope_count: u32,
    envelope_step: i8,
    attack: u8,
    hold: bool,
    alternate: bool,
    holding: bool,
}

impl Ay38910 {
    pub fn new() -> Self {
        let mut chip = Ay38910 {
            regs: [0; 16],
            address: 0,
            prescale: 0,
            tone_count: [0; 3],
            tone_high: [false; 3],
            noise_count: 0,
            noise_shift: false,
            noise: 1,
            envelope_count: 0,
            envelope_step: 0,
            attack: 0,
            hold: false,
            alternate: false,
            holding: false,
        };
        chip.reset();
        chip
    }

    /// Its /RESET pin low: every register 0.
    pub fn reset(&mut self) {
        for reg in 0..16 {
            self.write(reg, 0);
        }
        self.address = 0;
    }

    /// Its bus, by its pins BDIR and BC1, and its data lines: BDIR and BC1
    /// both, a register's number latched; BDIR alone, the register written;
    /// BC1 alone, read, the register's byte on the data lines; neither,
    /// the bus let alone.
    pub fn bus(&mut self, bdir: bool, bc1: bool, data: u8) -> Option<u8> {
        match (bdir, bc1) {
            (true, true) => self.address = data,
            (true, false) if self.address < 16 => self.write(self.address as usize, data),
            (false, true) if self.address < 16 => return Some(self.regs[self.address as usize]),
            _ => {}
        }
        None
    }

    fn write(&mut self, reg: usize, value: u8) {
        self.regs[reg] = value & MASKS[reg];
        // A shape: the envelope started over. Not continuing, it holds at
        // its end, at 0: as if continuing, holding, and alternating if an
        // attack.
        if reg == ENVELOPE_SHAPE {
            let shape = self.regs[ENVELOPE_SHAPE];
            self.attack = if shape & 0x04 != 0 { 0x0F } else { 0x00 };
            if shape & 0x08 == 0 {
                self.hold = true;
                self.alternate = self.attack != 0;
            } else {
                self.hold = shape & 0x01 != 0;
                self.alternate = shape & 0x02 != 0;
            }
            self.envelope_step = 15;
            self.envelope_count = 0;
            self.holding = false;
        }
    }

    fn tone_period(&self, channel: usize) -> u16 {
        let period = self.regs[2 * channel] as u16 | (self.regs[2 * channel + 1] as u16) << 8;
        period.max(1)
    }

    /// A cycle of its clock: each eighth, a step.
    pub fn tick(&mut self) {
        self.prescale += 1;
        if self.prescale < 8 {
            return;
        }
        self.prescale = 0;

        for channel in 0..3 {
            self.tone_count[channel] += 1;
            if self.tone_count[channel] >= self.tone_period(channel) {
                self.tone_count[channel] = 0;
                self.tone_high[channel] = !self.tone_high[channel];
            }
        }

        self.noise_count += 1;
        if self.noise_count >= self.regs[6].max(1) {
            self.noise_count = 0;
            self.noise_shift = !self.noise_shift;
            if !self.noise_shift {
                let bit = (self.noise ^ (self.noise >> 3)) & 1;
                self.noise = (self.noise | bit << 17) >> 1;
            }
        }

        if !self.holding {
            self.envelope_count += 1;
            let period = (self.regs[11] as u32 | (self.regs[12] as u32) << 8).max(1);
            if self.envelope_count >= 2 * period {
                self.envelope_count = 0;
                self.envelope_step -= 1;
                // Its end: held, or round again, alternating the other way.
                if self.envelope_step < 0 {
                    if self.hold {
                        if self.alternate {
                            self.attack ^= 0x0F;
                        }
                        self.holding = true;
                        self.envelope_step = 0;
                    } else {
                        if self.alternate {
                            self.attack ^= 0x0F;
                        }
                        self.envelope_step = 15;
                    }
                }
            }
        }
    }

    /// The envelope's volume, 0 to 15.
    pub fn envelope(&self) -> u8 {
        self.envelope_step as u8 ^ self.attack
    }

    /// Channel `channel`'s output, 0 to 1: its level, if the mixer lets it
    /// sound.
    pub fn output(&self, channel: usize) -> f32 {
        let mixer = self.regs[MIXER];
        let tone = self.tone_high[channel] || mixer & (1 << channel) != 0;
        let noise = self.noise & 1 != 0 || mixer & (8 << channel) != 0;
        if !(tone && noise) {
            return 0.0;
        }
        let amplitude = self.regs[8 + channel];
        let level = if amplitude & 0x10 != 0 {
            self.envelope()
        } else {
            amplitude & 0x0F
        };
        LEVELS[level as usize]
    }
}
