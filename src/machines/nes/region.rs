//! Where a NES is from: an NTSC one (America's and Japan's, the 2A03 and
//! 2C02) or a PAL one (Europe's and Australia's, the 2A07 and 2C07). The
//! same machine at a different speed: the PAL crystal's divided by 16 for
//! the CPU and 5 for the PPU, not 12 and 4, so 3.2 dots a CPU cycle, not 3;
//! and 312 lines a frame, not 262, the PPU's frame never a dot short, 70
//! lines of vertical blank to NTSC's 20. So 50 frames a second, not 60, the
//! APU's tables its own, to keep its notes in tune.
//!
//! A game's region: what a NES 2.0 header says, or an old one, on the rare
//! ones that say; else its file's name, "(Europe)" and the like, every
//! Europe-only game of a set being named so with a header saying NTSC. A
//! game of both, "(USA, Europe)", is one ROM for them, and runs as NTSC.
//! One whose name says Japan alone, "(J)" or "(Japan)", is a Famicom's.

pub use crate::machines::region::{Region, famicoms, japans_alone};

/// The CPU's clock, a PAL crystal's 26,601,712 Hz, a sixteenth.
pub const PAL_CLOCK_HZ: u32 = 1_662_607;

impl Region {
    /// The CPU's cycles a second.
    pub fn clock_hz(self) -> u32 {
        match self {
            Region::Ntsc => super::CLOCK_HZ,
            Region::Pal => PAL_CLOCK_HZ,
        }
    }

    /// The PPU's dots in CPU cycle `cycle`, from 1: on PAL, 16 dots every 5
    /// cycles, an extra one the fifth's.
    pub fn dots(self, cycle: u64) -> u64 {
        match self {
            Region::Ntsc => 3,
            Region::Pal => cycle * 16 / 5 - (cycle - 1) * 16 / 5,
        }
    }

    /// Lines a frame.
    pub fn lines(self) -> u16 {
        match self {
            Region::Ntsc => 262,
            Region::Pal => 312,
        }
    }

    /// Frames a second.
    pub fn frames_per_second(self) -> f64 {
        match self {
            Region::Ntsc => self.clock_hz() as f64 * 3.0 / (341.0 * 262.0 - 0.5),
            Region::Pal => self.clock_hz() as f64 * 3.2 / (341.0 * 312.0),
        }
    }

    /// A dot's width, of its height, as a TV shows it.
    pub fn dot_shape(self) -> f32 {
        match self {
            Region::Ntsc => 8.0 / 7.0,
            Region::Pal => 1.386,
        }
    }
}
