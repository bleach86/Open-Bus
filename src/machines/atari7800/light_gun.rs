//! The 7800's light gun, Atari's XG-1 or XE's (in the left port, or the
//! right, as the game has it): its trigger on the joystick's up (pin 1),
//! high while it's pulled; its light on the joystick's fire (pin 6, INPT4,
//! or INPT5), low while it sees some. A game turns the screen white as the
//! trigger's pulled and races the beam, looking for the moment the light
//! comes on, as a 2600's does.
//!
//! Its photodiode sees a circle of the TV where it's aimed, 8 of the
//! TIA's colour clocks (16 of MARIA's) every way: as a colour clock's
//! about two lines' height wide, 16 lines up and down. It says it's seen
//! the beam there a while after it has, a bright dot drawn there 74 of
//! MARIA's clocks before, and goes on saying so for 48 more: lit from then
//! till the first look a game's loop takes.
//!
//! The games, made for a real one, count on all of that, and it's from
//! them that it's known. With a7800's numbers here, a circle 8 colour
//! clocks across but 8 lines down, 28 colour clocks late, their shots land
//! some 22 dots left of where it's aimed and 7 lines below. With these,
//! Barnyard Blaster's bottles, 16 dots across, are hit aimed at from a
//! latency of 68 to 76; Meltdown's passkey arrows, 15 lines high, with
//! from 15 to 18 lines down.

use crate::host::display::Pointer;
use crate::machines::light_gun::bright;

/// The circle it sees, its radius, in colour clocks (two of MARIA's dots,
/// two lines down); how long after the beam it says so, and goes on saying
/// so, in MARIA's clocks.
const RADIUS: f32 = 8.0;
const LATENCY: i32 = 74;
const HOLD: i32 = 48;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightGun {
    /// Its port, 0 the left; where it's aimed, the dot (of 320) and the
    /// line shown, if it's at the picture; its trigger, pulled.
    pub port: usize,
    pub at: Option<(u16, u16)>,
    pub trigger: bool,
}

impl LightGun {
    /// One in port `port`.
    pub fn new(port: usize) -> LightGun {
        LightGun {
            port,
            ..LightGun::default()
        }
    }

    /// Aimed as `pointer` is, on a picture `width` dots across and `height`
    /// lines: its first button the trigger, and its second the trigger
    /// pulled aimed away from the TV.
    pub fn aim(&mut self, pointer: Pointer, width: usize, height: usize) {
        let [first, second] = pointer.buttons;
        self.trigger = first || second;
        self.at = pointer.at.filter(|_| !second).and_then(|(x, y)| {
            let (x, y) = (x.floor() as i32, y.floor() as i32);
            let on = (0..width as i32).contains(&x) && (0..height as i32).contains(&y);
            on.then_some((x as u16, y as u16))
        });
    }

    /// Its joystick's directions, as SWCHA's 4 bits of its port have them:
    /// up, its trigger, high while it's pulled; the rest high.
    pub fn stick(&self) -> u8 {
        0x0E | self.trigger as u8
    }

    /// Whether it sees light, the beam at `clock` of the picture's line
    /// `row` (as MARIA's clocks from its first dot; a line before the
    /// picture's below 0), a line `line` clocks, the picture drawn so far
    /// `drawn`, `width` dots across, its colours `colours`.
    pub fn sees(
        &self,
        row: i32,
        clock: i32,
        line: i32,
        drawn: &[u8],
        width: usize,
        colours: &[u32],
    ) -> bool {
        let Some((x, y)) = self.at else {
            return false;
        };
        // The dots it's saying it's seen: drawn a while ago, as long ago as
        // it holds the light.
        (clock - LATENCY - HOLD..=clock - LATENCY).any(|dot| {
            // Drawn on the line before, if it's before this one's first.
            let (row, dot) = if dot < 0 {
                (row - 1, dot + line)
            } else {
                (row, dot)
            };
            if row < 0 || !(0..width as i32).contains(&dot) {
                return false;
            }
            let across = (dot - x as i32) as f32 / 2.0;
            let down = (row - y as i32) as f32 / 2.0;
            let near = across * across + down * down < RADIUS * RADIUS;
            let colour = drawn.get(row as usize * width + dot as usize);
            near && colour.is_some_and(|&colour| bright(colours[colour as usize]))
        })
    }
}
