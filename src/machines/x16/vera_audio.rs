//! VERA's sound, as its Programmer's Reference has it and x16emu (BSD-2)
//! does: a sample each 512 of its 25 MHz clocks, 48,828 a second, of its
//! PSG and its PCM, left and right, signed 16 bits.
//!
//! The PSG's 16 voices, each 4 registers at `$1F9C0` on: a frequency word,
//! added to its 17-bit phase each sample; its volume (64 steps, a log
//! curve) and whether it's left, right or both, or neither, silent and its
//! phase held at 0; its waveform, a pulse of a width, a sawtooth, a
//! triangle (each XORed with the width's complement), or noise, a new
//! value of a 16-bit LFSR's each time the phase wraps. The LFSR steps
//! once a voice, so two voices' noise differs.
//!
//! The PCM's FIFO of 4,095 bytes, filled by AUDIO_DATA, read as its rate
//! (AUDIO_RATE, 128 a sample each sample) carries its phase past bit 7: a
//! sample, 8 or 16 bits, mono or stereo, as AUDIO_CTRL has it, all of it
//! read at once, or, if it isn't all there, the FIFO emptied; empty, it's
//! silent. Its volume's 16 steps, a log curve. AFLOW's while it holds less
//! than 1,024.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// The PSG's volumes, 0 to 63.
#[rustfmt::skip]
const PSG_VOLUMES: [u16; 64] = [
      0,   4,   8,  12,  16,  17,  18,  20,  21,  22,  23,  25,  26,  28,  30,  31,
     33,  35,  37,  40,  42,  45,  47,  50,  53,  56,  60,  63,  67,  71,  75,  80,
     85,  90,  95, 101, 107, 113, 120, 127, 135, 143, 151, 160, 170, 180, 191, 202,
    214, 227, 241, 255, 270, 286, 303, 321, 341, 361, 382, 405, 429, 455, 482, 511,
];

/// The PCM's volumes, 0 to 15, in 64ths.
const PCM_VOLUMES: [i32; 16] = [0, 1, 2, 3, 4, 5, 6, 8, 11, 14, 18, 23, 30, 38, 49, 64];

/// The FIFO's bytes, and fewer than this many, AFLOW.
const FIFO_SIZE: usize = 4095;
const FIFO_LOW: usize = 1024;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Voice {
    frequency: u16,
    volume: u16,
    left: bool,
    right: bool,
    width: u8,
    waveform: u8,
    noise: u8,
    phase: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Psg {
    voices: [Voice; 16],
    lfsr: u16,
}

impl Default for Psg {
    fn default() -> Psg {
        Psg::new()
    }
}

impl Psg {
    pub fn new() -> Psg {
        Psg {
            voices: [Voice::default(); 16],
            lfsr: 1,
        }
    }

    /// Register `reg`, 0 to 63, written.
    pub fn write(&mut self, reg: usize, value: u8) {
        let voice = &mut self.voices[(reg >> 2) & 15];
        match reg & 3 {
            0 => voice.frequency = voice.frequency & 0xFF00 | value as u16,
            1 => voice.frequency = voice.frequency & 0x00FF | (value as u16) << 8,
            2 => {
                voice.right = value & 0x80 != 0;
                voice.left = value & 0x40 != 0;
                voice.volume = PSG_VOLUMES[(value & 0x3F) as usize];
            }
            _ => {
                voice.width = value & 0x3F;
                voice.waveform = value >> 6;
            }
        }
    }

    /// A sample, left and right.
    pub fn sample(&mut self) -> (i16, i16) {
        let (mut left, mut right) = (0i16, 0i16);
        for voice in &mut self.voices {
            let lfsr = self.lfsr;
            let bit = (lfsr >> 1 ^ lfsr >> 2 ^ lfsr >> 4 ^ lfsr >> 15) & 1;
            self.lfsr = lfsr << 1 | bit;
            let phase = if voice.left || voice.right {
                (voice.phase + voice.frequency as u32) & 0x1FFFF
            } else {
                0
            };
            if voice.phase & 0x10000 != 0 && phase & 0x10000 == 0 {
                voice.noise = (self.lfsr >> 1) as u8 & 0x3F;
            }
            voice.phase = phase;
            let xor = !voice.width & 0x3F;
            let level = match voice.waveform {
                0 => ((phase >> 10) as u8 <= voice.width) as u8 * 0x3F,
                1 => (phase >> 11) as u8 ^ xor,
                2 => {
                    let up = (phase >> 10) as u8 & 0x3F;
                    (if phase & 0x10000 != 0 { !up & 0x3F } else { up }) ^ xor
                }
                _ => voice.noise,
            };
            // 6 bits, offset to signed.
            let signed = ((level ^ 0x20) << 2) as i8 as i16 >> 2;
            let value = signed.wrapping_mul(voice.volume as i16) >> 3;
            if voice.left {
                left = left.wrapping_add(value);
            }
            if voice.right {
                right = right.wrapping_add(value);
            }
        }
        (left, right)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pcm {
    fifo: VecDeque<u8>,
    /// AUDIO_CTRL's bits 5 to 0: 16 bits, stereo, volume.
    ctrl: u8,
    rate: u8,
    phase: u8,
    current: (i16, i16),
}

impl Default for Pcm {
    fn default() -> Pcm {
        Pcm::new()
    }
}

impl Pcm {
    pub fn new() -> Pcm {
        Pcm {
            fifo: VecDeque::with_capacity(FIFO_SIZE),
            ctrl: 0,
            rate: 0,
            phase: 0,
            current: (0, 0),
        }
    }

    /// AUDIO_CTRL: its FIFO full and empty, bits 7 and 6.
    pub fn ctrl(&self) -> u8 {
        let full = (self.fifo.len() == FIFO_SIZE) as u8;
        self.ctrl | full << 7 | (self.fifo.is_empty() as u8) << 6
    }

    pub fn write_ctrl(&mut self, value: u8) {
        if value & 0x80 != 0 {
            self.fifo.clear();
        }
        self.ctrl = value & 0x3F;
    }

    pub fn rate(&self) -> u8 {
        self.rate
    }

    pub fn write_rate(&mut self, value: u8) {
        self.rate = value;
    }

    /// AUDIO_DATA: a byte into the FIFO, unless it's full.
    pub fn write_data(&mut self, value: u8) {
        if self.fifo.len() < FIFO_SIZE {
            self.fifo.push_back(value);
        }
    }

    /// Whether it's AFLOW, its FIFO less than a quarter full.
    pub fn low(&self) -> bool {
        self.fifo.len() < FIFO_LOW
    }

    /// A sample, left and right: the next from the FIFO, as the rate's
    /// due one.
    pub fn sample(&mut self) -> (i16, i16) {
        let before = self.phase;
        self.phase = self.phase.wrapping_add(self.rate);
        if (before ^ self.phase) & 0x80 != 0 {
            self.next();
        }
        let volume = PCM_VOLUMES[(self.ctrl & 0x0F) as usize];
        let scaled = |value: i16| (value as i32 * volume / 64) as i16;
        (scaled(self.current.0), scaled(self.current.1))
    }

    /// The FIFO's next sample, all its bytes, as its format has them.
    fn next(&mut self) {
        if self.fifo.is_empty() {
            self.current = (0, 0);
            return;
        }
        let (sixteen, stereo) = (self.ctrl & 0x20 != 0, self.ctrl & 0x10 != 0);
        let bytes = (1 + sixteen as usize) * (1 + stereo as usize);
        if self.fifo.len() < bytes {
            self.fifo.clear();
            return;
        }
        let mut take = || {
            let low = self.fifo.pop_front().unwrap();
            if sixteen {
                i16::from_le_bytes([low, self.fifo.pop_front().unwrap()])
            } else {
                (low as i16) << 8
            }
        };
        let left = take();
        let right = if stereo { take() } else { left };
        self.current = (left, right);
    }
}
