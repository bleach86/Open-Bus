//! The TIA's sound, as Stella has it: two channels, each a volume (AUDV), a
//! frequency divider (AUDF, 5 bits) and a control (AUDC, 4 bits) choosing
//! its wave, made by a 4-bit pulse counter and a 5-bit noise counter shifted
//! with feedback as AUDC says. The divider's clocked twice a line, at color
//! clocks 9 and 81 (phase 0, its count and the noise's feedback) and 37 and
//! 149 (phase 1, the counters shifted when the divider's come round); the
//! channel's output, its volume while the pulse counter's bit 0 is set.
//!
//! The two mixed as the TIA's output does, through its shared resistor:
//! not added, but each the less the louder the other is.

use serde::{Deserialize, Serialize};

/// A channel: its registers, its divider's count and whether it's come
/// round, and its pulse and noise counters.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    audc: u8,
    audf: u8,
    audv: u8,
    clock_enable: bool,
    noise_feedback: bool,
    noise_bit_4: bool,
    pulse_hold: bool,
    divider: u8,
    pulse: u8,
    noise: u8,
}

impl Channel {
    pub fn audc(&mut self, value: u8) {
        self.audc = value & 0x0F;
    }

    pub fn audf(&mut self, value: u8) {
        self.audf = value & 0x1F;
    }

    pub fn audv(&mut self, value: u8) {
        self.audv = value & 0x0F;
    }

    fn phase_0(&mut self) {
        if self.clock_enable {
            self.noise_bit_4 = self.noise & 0x01 != 0;
            self.pulse_hold = match self.audc & 0x03 {
                0 | 1 => false,
                2 => self.noise & 0x1E != 0x02,
                _ => !self.noise_bit_4,
            };
            self.noise_feedback = if self.audc & 0x03 == 0 {
                (self.pulse ^ self.noise) & 0x01 != 0
                    || !(self.noise != 0 || self.pulse != 0x0A)
                    || self.audc & 0x0C == 0
            } else {
                ((self.noise & 0x04 != 0) ^ (self.noise & 0x01 != 0)) || self.noise == 0
            };
        }
        self.clock_enable = self.divider == self.audf;
        if self.divider == self.audf || self.divider == 0x1F {
            self.divider = 0;
        } else {
            self.divider += 1;
        }
    }

    fn phase_1(&mut self) {
        if !self.clock_enable {
            return;
        }
        let pulse = self.pulse;
        let feedback = match self.audc >> 2 {
            0 => {
                ((pulse & 0x02 != 0) ^ (pulse & 0x01 != 0))
                    && pulse != 0x0A
                    && self.audc & 0x03 != 0
            }
            1 => pulse & 0x08 == 0,
            2 => !self.noise_bit_4,
            _ => !(pulse & 0x02 != 0 || pulse & 0x0E == 0),
        };
        self.noise >>= 1;
        if self.noise_feedback {
            self.noise |= 0x10;
        }
        if !self.pulse_hold {
            self.pulse = !(pulse >> 1) & 0x07;
            if feedback {
                self.pulse |= 0x08;
            }
        }
    }

    /// Its output now, 0 to 15.
    pub fn volume(&self) -> u8 {
        (self.pulse & 0x01) * self.audv
    }
}

/// The two channels, and the color clock of the line they're at, which
/// runs on whatever RSYNC does to the TIA's own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sound {
    pub channels: [Channel; 2],
    clock: u8,
}

/// The mixing resistor's, of the volume steps': how far the louder the
/// sum is, the less each step adds.
const MIXING: f32 = 30.0;

impl Sound {
    /// A color clock: the channels clocked at their phases.
    pub fn tick(&mut self) {
        self.clock += 1;
        match self.clock {
            10 | 82 => self.channels.iter_mut().for_each(Channel::phase_0),
            38 | 150 => self.channels.iter_mut().for_each(Channel::phase_1),
            228 => self.clock = 0,
            _ => {}
        }
    }

    /// The two mixed, 0 to 1.
    pub fn output(&self) -> f32 {
        let sum = (self.channels[0].volume() + self.channels[1].volume()) as f32;
        let most = 30.0;
        sum / most * (MIXING + most) / (MIXING + sum)
    }
}
