//! What's plugged into each port: a joystick (and its fire button, INPT4 or
//! INPT5); a pair of paddles, each a 1 MΩ pot read by the TIA (INPT0 and 1,
//! or 2 and 3) and a button on the joystick's pins (the left port's bits 7
//! and 6, the right's 3 and 2); or a driving controller, its wheel's gray
//! code on the up and down pins, its fire button the joystick's.
//!
//! Which, as Stella's properties say, or as Stella finds it: paddles if the
//! game's code reads their inputs as only a paddle game does.
//!
//! A paddle's turned by the stick or the D-pad, at a speed, and stays where
//! it's left; or by the pointer, as far across the picture as it is.

use super::database::Controller as Named;

/// A port's controller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Controller {
    #[default]
    Joystick,
    /// Turned by the stick's other axis, up and down, and the other way
    /// round, if they're `reversed`.
    Paddles {
        other_axis: bool,
        reversed: bool,
    },
    Driving,
}

impl Controller {
    /// As Stella's properties name it; a keypad and the rest, a joystick, its
    /// pins the nearest.
    pub fn named(named: Named) -> Controller {
        match named {
            Named::Paddles => Controller::Paddles {
                other_axis: false,
                reversed: false,
            },
            Named::PaddlesOtherAxis => Controller::Paddles {
                other_axis: true,
                reversed: false,
            },
            Named::PaddlesOtherAxisReversed => Controller::Paddles {
                other_axis: true,
                reversed: true,
            },
            Named::Driving => Controller::Driving,
            _ => Controller::Joystick,
        }
    }

    /// The controller in the left port, `right` the right's, as Stella finds
    /// it in `image`: paddles if their inputs are read as only a paddle
    /// game reads them, even if it reads the joystick's button too.
    pub fn found(image: &[u8], right: bool) -> Controller {
        let paddles = if right {
            right_paddles(image)
        } else {
            left_paddles(image)
        };
        if paddles {
            Controller::Paddles {
                other_axis: false,
                reversed: false,
            }
        } else {
            Controller::Joystick
        }
    }
}

fn has(image: &[u8], pattern: &[u8]) -> bool {
    image.windows(pattern.len()).any(|window| window == pattern)
}

fn any(image: &[u8], patterns: &[&[u8]]) -> bool {
    patterns.iter().any(|pattern| has(image, pattern))
}

fn left_paddles(image: &[u8]) -> bool {
    any(
        image,
        &[
            &[0xA5, 0x08, 0x10],
            &[0xA5, 0x08, 0x30],
            &[0xB5, 0x08, 0x30],
            &[0x24, 0x38, 0x10],
            &[0x24, 0x38, 0x30],
            &[0xA5, 0x38, 0x10],
            &[0xA5, 0x38, 0x30],
            &[0xB5, 0x38, 0x10],
            &[0xB5, 0x38, 0x30],
            &[0x68, 0x48, 0x10],
            &[0xA5, 0x08, 0x4C],
            &[0xA4, 0x38, 0x30],
            &[0xB9, 0x08, 0x00, 0x30],
            &[0xB9, 0x38, 0x00, 0x30],
            &[0xB9, 0x08, 0x00, 0x10],
            &[0x24, 0x08, 0x30, 0x02],
            &[0xB5, 0x38, 0x29, 0x80, 0xD0],
            &[0x24, 0x38, 0x85, 0x08, 0x10],
            &[0xB5, 0x38, 0x49, 0xFF, 0x0A],
            &[0xB1, 0xF2, 0x30, 0x02, 0xE6],
        ],
    )
}

fn right_paddles(image: &[u8]) -> bool {
    any(
        image,
        &[
            &[0x24, 0x0A, 0x10],
            &[0x24, 0x0A, 0x30],
            &[0xA5, 0x0A, 0x10],
            &[0xA5, 0x0A, 0x30],
            &[0xB5, 0x0A, 0x30],
            &[0xB5, 0x08, 0x10],
            &[0xB5, 0x08, 0x30],
            &[0x24, 0x3A, 0x10],
            &[0x24, 0x3A, 0x30],
            &[0xA5, 0x3A, 0x10],
            &[0xA5, 0x3A, 0x30],
            &[0xB5, 0x3A, 0x10],
            &[0xB5, 0x3A, 0x30],
            &[0xB5, 0x38, 0x10],
            &[0xB5, 0x38, 0x30],
            &[0xA4, 0x3A, 0x30],
            &[0xA5, 0x3B, 0x30],
            &[0xB9, 0x38, 0x00, 0x30],
            &[0xB5, 0x38, 0x29, 0x80, 0xD0],
            &[0x24, 0x38, 0x85, 0x08, 0x10],
            &[0xB5, 0x38, 0x49, 0xFF, 0x0A],
        ],
    )
}

/// How far a paddle's turned each frame the stick's held all the way: across
/// its range in a second and a half.
const PADDLE_SPEED: f32 = 1.0 / 90.0;

/// A paddle: how far it's turned, 0 (all the way left, its pot's least) to
/// 1; and where the pointer was, to tell when it's moved.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Paddle {
    pub turned: f32,
    pointer: Option<f32>,
}

impl Paddle {
    pub fn new() -> Paddle {
        Paddle {
            turned: 0.5,
            pointer: None,
        }
    }

    /// A frame's turning: by `stick`, -1 to 1, at a speed; or by `pointer`,
    /// 0 to 1 across the picture, if it's moved.
    pub fn turn(&mut self, stick: f32, pointer: Option<f32>) {
        if let Some(at) = pointer.filter(|&at| Some(at) != self.pointer) {
            self.turned = at;
        } else {
            self.turned += stick * PADDLE_SPEED;
        }
        self.pointer = pointer;
        self.turned = self.turned.clamp(0.0, 1.0);
    }

    /// Its pot's resistance, in ohms: a paddle turned right, the less.
    pub fn resistance(&self, reversed: bool) -> u32 {
        let turned = if reversed {
            self.turned
        } else {
            1.0 - self.turned
        };
        (turned * 1_000_000.0) as u32
    }
}

/// A driving controller's wheel: which of its 4 gray code steps it's at,
/// turned a step every few frames the stick's held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wheel {
    step: u8,
    frames: u8,
}

/// The gray code's steps, as the up and down pins have them.
const GRAY: [u8; 4] = [0b11, 0b01, 0b00, 0b10];

impl Wheel {
    /// A frame's turning, left or right.
    pub fn turn(&mut self, left: bool, right: bool) {
        if left == right {
            self.frames = 0;
            return;
        }
        self.frames += 1;
        if self.frames >= 2 {
            self.frames = 0;
            self.step = if right {
                (self.step + 1) & 3
            } else {
                (self.step + 3) & 3
            };
        }
    }

    /// The up and down pins, bits 0 and 1.
    pub fn pins(&self) -> u8 {
        GRAY[self.step as usize]
    }
}
