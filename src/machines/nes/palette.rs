//! The NES's colours, as a TV shows them. The PPU makes no RGB of
//! its own: each of its 64 colours is a square wave on the video signal,
//! between two of its voltages, a level's low and high; in phase with the
//! colour burst by its hue, 1 to 12 (0 is the high voltage alone, a grey;
//! 13 the low, 14 and 15 black). Its emphasis bits, red, green and blue,
//! dim the signal while the wave is in the phase of each. A TV decodes
//! the signal to Y, I and Q, a brightness and two colours, then RGB: each
//! colour here is that, the signal sampled at 12 phases. Turned up, as a
//! TV's colour is: to look as the NES does on one.
//!
//! A PAL NES's is the same signal: only its colours' axis is half a hue
//! on, its hue mirrored about 2.5 line by line, not about 2 (so its burst,
//! hue 7, is PAL's 135 degrees), which a PAL TV mirrors back; and its
//! emphasis bits' red and green are the other way round.

use super::region::Region;
use std::f32::consts::PI;

/// The signal's voltages: its levels' lows, then their highs; black's and
/// white's; and an emphasis's dimming.
const LEVELS: [f32; 8] = [0.350, 0.518, 0.962, 1.550, 1.094, 1.506, 1.962, 1.962];
const BLACK: f32 = 0.518;
const WHITE: f32 = 1.962;
const ATTENUATION: f32 = 0.746;

/// The colour burst's phase, of the 12, against the decoder's, an NTSC
/// NES's and a PAL's; and the colour turned up.
const HUE: f32 = 4.0;
const PAL_HUE: f32 = 4.5;
const SATURATION: f32 = 1.5;

/// The 512 colours, 0xRRGGBB: a colour's 6 bits, then the emphasis's 3.
pub fn palette(region: Region) -> Vec<u32> {
    (0..512).map(|pixel| rgb(pixel, region)).collect()
}

fn rgb(pixel: usize, region: Region) -> u32 {
    let (hue, emphasis) = (pixel & 0x0F, pixel >> 6);
    let (emphasis, burst) = match region {
        Region::Ntsc => (emphasis, HUE),
        Region::Pal => (emphasis & 4 | (emphasis & 1) << 1 | (emphasis & 2) >> 1, PAL_HUE),
    };
    let level = if hue > 13 { 1 } else { (pixel >> 4) & 3 };
    let (mut low, mut high) = (LEVELS[level], LEVELS[4 + level]);
    if hue == 0 {
        low = high;
    }
    if hue > 12 {
        high = low;
    }
    let (mut y, mut i, mut q) = (0.0, 0.0, 0.0);
    for phase in 0..12 {
        let in_phase = |hue: usize| (hue + phase) % 12 < 6;
        let mut signal = if in_phase(hue) { high } else { low };
        if (emphasis & 1 != 0 && in_phase(0))
            || (emphasis & 2 != 0 && in_phase(4))
            || (emphasis & 4 != 0 && in_phase(8))
        {
            signal *= ATTENUATION;
        }
        let level = (signal - BLACK) / (WHITE - BLACK) / 12.0;
        let angle = PI * (phase as f32 + burst) / 6.0;
        y += level;
        i += level * angle.cos() * SATURATION;
        q += level * angle.sin() * SATURATION;
    }
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    let r = channel(y + 0.946_882 * i + 0.623_557 * q);
    let g = channel(y - 0.274_788 * i - 0.635_691 * q);
    let b = channel(y - 1.108_545 * i + 1.709_007 * q);
    r << 16 | g << 8 | b
}
