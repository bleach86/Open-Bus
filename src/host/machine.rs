//! What a machine is to the host: its bus, for the CPU, handing the run
//! loop back as each frame ends (`Bus::hand_back`); and this, what the host
//! needs of it between frames. The host (`host::run`) has the window, or
//! the terminal; the machine only emulates its hardware, and has what the
//! host gives it: the keys, the controls, the Reset button.

use crate::cpu::bus::Bus;
use crate::host::display::{Gamepad, Joystick, Key, Pointer};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A frame to show: `width` by `height` pixels, 0xRRGGBB each, a line at a
/// time from the top.
pub struct Picture<'a> {
    pub pixels: &'a [u32],
    pub width: usize,
    pub height: usize,
}

/// The controls as they are this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub joystick: Joystick,
    pub gamepad: Gamepad,
    /// The second player's.
    pub gamepad_2: Gamepad,
    /// The pointer, a light gun's.
    pub pointer: Pointer,
    /// Whether a key is held down, any but the modifiers and the function
    /// keys.
    pub key_down: bool,
}

pub trait Machine: Bus + Serialize + DeserializeOwned {
    /// The frame just ended, as it's seen.
    fn picture(&mut self) -> Picture<'_>;

    /// Its screen as text, for a terminal, with its escape sequences; if
    /// it has one.
    fn text_screen(&self) -> Option<String> {
        None
    }

    /// What the window's called: which machine, and what's in it.
    fn title(&self) -> String;

    /// Its sound made from now on: not without a host to play it, a cost
    /// the tests needn't pay.
    fn sound_on(&mut self);

    /// The samples made since they were last taken, at SAMPLE_RATE, each a
    /// left then a right.
    fn sound(&mut self) -> &mut Vec<f32>;

    /// What it has to say of itself on the debug panel, a line each.
    fn debug(&self) -> Vec<String> {
        Vec::new()
    }

    /// A key typed.
    fn key(&mut self, _key: Key) {}

    /// Whether it has a keyboard, for what's pasted to be typed on.
    fn keyboard(&self) -> bool {
        false
    }

    /// `text` typed on its keyboard, as it's pasted: a key at a time, as
    /// the program takes them.
    fn type_text(&mut self, _text: &str) {}

    /// The text on its screen, to be copied: a line each, as shown, without
    /// the blank lines above and below; if it has any showing.
    fn screen_text(&self) -> Option<String> {
        None
    }

    /// The controls, a frame at a time.
    fn input(&mut self, input: Input);

    /// Whether it can be rewound: its states small enough, and quick enough
    /// to make, for a ring of them to be kept as it runs. A console's are.
    fn rewinds(&self) -> bool {
        false
    }

    /// Whether it's aimed at, as a light gun is: the pointer's, then.
    fn aimed_at(&self) -> bool {
        false
    }

    /// Whether it has a mouse: the host's captured for it, the pointer's
    /// moves its, past the picture's edges too.
    fn has_mouse(&self) -> bool {
        false
    }

    /// A function key for the machine's own uses (F2 and F4 are the
    /// host's): a notice of what it did, if it did anything.
    fn function_key(&mut self, _key: u8) -> Option<String> {
        None
    }

    /// Whether a disk drive's busy this frame, for its light: its motor
    /// running, or a block just read or written. Asked once a frame, it can
    /// forget an access it's told of.
    fn light(&mut self) -> bool {
        false
    }

    /// What it has of its own for the menu over it: each a name, and the
    /// function key it's as.
    fn menu(&self) -> Vec<(&'static str, u8)> {
        Vec::new()
    }

    /// This coming frame's picture left out or not, for time, if it can be:
    /// whether it is. It runs the same either way.
    fn leave_out_picture(&mut self, _leave_out: bool) -> bool {
        false
    }

    /// The Reset button, as it is this frame: held, or not.
    fn reset_button(&mut self, held: bool);

    /// What it keeps, its disks' or its battery's, saved if it's due; and
    /// all of it, `closing`.
    fn keep_saved(&mut self, _closing: bool) {}

    /// The machine, as a save state's header names it: its make, its ROMs,
    /// whatever a state of it leaves out, for a state to be refused by
    /// another.
    fn identity(&self) -> String;

    /// What a save state leaves out, taken from `running`, the machine it's
    /// loaded into.
    fn keep(&mut self, running: &mut Self);
}

/// What a machine's RAM holds at power-on. A real chip's comes up however
/// it does, the same each time on one console and not on another; a game
/// that clears its RAM first doesn't care, and a few don't. All 1s, $FF,
/// unless it's said; all 0s; or as random as a real one's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PowerOnRam {
    #[default]
    Ones,
    Zeros,
    Random,
}

impl PowerOnRam {
    /// As `--ram=` names it.
    pub fn parse(name: &str) -> Option<PowerOnRam> {
        match name {
            "ones" => Some(PowerOnRam::Ones),
            "zeros" => Some(PowerOnRam::Zeros),
            "random" => Some(PowerOnRam::Random),
            _ => None,
        }
    }

    /// `ram` as it'd be at power-on.
    pub fn fill(self, ram: &mut [u8]) {
        match self {
            PowerOnRam::Ones => ram.fill(0xFF),
            PowerOnRam::Zeros => ram.fill(0),
            PowerOnRam::Random => {
                use std::hash::{BuildHasher, Hasher};
                // A seed of the system's own randomness, then xorshift.
                let mut seed = std::collections::hash_map::RandomState::new()
                    .build_hasher()
                    .finish()
                    | 1;
                for byte in ram {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    *byte = (seed >> 32) as u8;
                }
            }
        }
    }
}
