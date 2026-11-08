//! The Zapper, the NES's light gun, in its second port (a Famicom's in its
//! expansion port, read at the same $4017): bit 4 its trigger, pulled; bit
//! 3 its light, 0 when it sees some. A game blanks the screen, or draws its
//! targets white on black, a frame each, and looks.
//!
//! Its photodiode sees the TV where it's aimed, lit as the beam draws it
//! and a little while after, as Mesen has it: lit if the PPU's drawn a
//! bright dot there (its red, green and blue adding up to 85 or more, of
//! 765) within the last 20 lines.

use super::ppu::{HEIGHT, WIDTH};
use crate::host::display::Pointer;

/// How many lines a dot drawn is seen for, and how bright it's to be.
const LINES_SEEN: u16 = 20;
const BRIGHT: u32 = 85;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Zapper {
    /// Where it's aimed, the dot and the line, if it's at the picture.
    pub at: Option<(u16, u16)>,
    pub trigger: bool,
}

impl Zapper {
    /// Aimed as `pointer` is, on a picture with `overscan` dots hidden at
    /// each edge: its first button the trigger, and its second the trigger
    /// pulled aimed away from the TV, as some games have it to reload.
    pub fn aim(&mut self, pointer: Pointer, overscan: usize) {
        let [first, second] = pointer.buttons;
        self.trigger = first || second;
        self.at = pointer.at.filter(|_| !second).and_then(|(x, y)| {
            let (x, y) = (
                x.floor() as i32 + overscan as i32,
                y.floor() as i32 + overscan as i32,
            );
            let on = (0..WIDTH as i32).contains(&x) && (0..HEIGHT as i32).contains(&y);
            on.then_some((x as u16, y as u16))
        });
    }

    /// Its bits at $4017, the PPU at `dot` of `line`, the frame it's drawn
    /// so far `frame`, its colours `colours`.
    pub fn read(&self, line: u16, dot: u16, frame: &[u16], colours: &[u32]) -> u8 {
        let lit = self.at.is_some_and(|(x, y)| {
            let rgb = colours[frame[y as usize * WIDTH + x as usize] as usize];
            let brightness = (rgb >> 16 & 0xFF) + (rgb >> 8 & 0xFF) + (rgb & 0xFF);
            // Drawn by now (dot x + 1 draws x), and not long ago.
            let drawn = line > y || line == y && dot > x;
            drawn && line - y <= LINES_SEEN && brightness >= BRIGHT
        });
        (if lit { 0 } else { 0x08 }) | if self.trigger { 0x10 } else { 0 }
    }
}
