//! A machine's sound, its signal a cycle at a time made into the host's:
//! samples at SAMPLE_RATE, and the filters on the way.

use crate::host::display::SAMPLE_RATE;
use std::f32::consts::PI;

/// `N` signals, a value each a cycle of a clock of `clock_hz`, averaged
/// over each sample's cycles.
pub struct Sampler<const N: usize> {
    clock_hz: u32,
    /// How far into the sample it is, in parts of the clock.
    phase: u32,
    cycles: u32,
    sums: [f32; N],
}

impl<const N: usize> Sampler<N> {
    pub fn new(clock_hz: u32) -> Self {
        Sampler {
            clock_hz,
            phase: 0,
            cycles: 0,
            sums: [0.0; N],
        }
    }

    /// A cycle's values: the sample's averages, if that was its last.
    pub fn cycle(&mut self, values: [f32; N]) -> Option<[f32; N]> {
        for (sum, value) in self.sums.iter_mut().zip(values) {
            *sum += value;
        }
        self.cycles += 1;
        self.phase += SAMPLE_RATE;
        if self.phase < self.clock_hz {
            return None;
        }
        self.phase -= self.clock_hz;
        let cycles = self.cycles as f32;
        let averages = self.sums.map(|sum| sum / cycles);
        self.sums = [0.0; N];
        self.cycles = 0;
        Some(averages)
    }
}

/// A first-order low-pass.
pub struct LowPass {
    share: f32,
    last: f32,
}

impl LowPass {
    /// At `corner` Hz, on a signal at `rate` values a second.
    pub fn new(corner: f32, rate: u32) -> Self {
        let rc = 1.0 / (2.0 * PI * corner);
        let dt = 1.0 / rate as f32;
        LowPass {
            share: dt / (rc + dt),
            last: 0.0,
        }
    }

    pub fn filter(&mut self, value: f32) -> f32 {
        self.last += self.share * (value - self.last);
        self.last
    }
}

/// A first-order high-pass: what's left of a level as it settles away, a
/// share `kept` of it each value.
pub struct HighPass {
    kept: f32,
    last_in: f32,
    last_out: f32,
}

impl HighPass {
    /// At `corner` Hz, on a signal at `rate` values a second.
    pub fn new(corner: f32, rate: u32) -> Self {
        let rc = 1.0 / (2.0 * PI * corner);
        let dt = 1.0 / rate as f32;
        HighPass::settled(rc / (rc + dt), 0.0)
    }

    /// Keeping `kept` of its output each value, settled at `level`.
    pub fn settled(kept: f32, level: f32) -> Self {
        HighPass {
            kept,
            last_in: level,
            last_out: 0.0,
        }
    }

    pub fn filter(&mut self, value: f32) -> f32 {
        self.last_out = self.kept * (self.last_out + value - self.last_in);
        self.last_in = value;
        self.last_out
    }
}
