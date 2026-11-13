//! What's plugged into each port: a joystick (and its fire button, INPT4 or
//! INPT5); a pair of paddles, each a 1 MΩ pot read by the TIA (INPT0 and 1,
//! or 2 and 3) and a button on the joystick's pins (the left port's bits 7
//! and 6, the right's 3 and 2); a driving controller, its wheel's gray
//! code on the up and down pins, its fire button the joystick's; or a
//! keypad, 12 keys in 4 rows and 3 columns, the rows the joystick's pins,
//! driven by the game, and the columns the paddles' inputs and the fire
//! button's.
//!
//! Which, as Stella's properties say, or as Stella finds it: paddles if the
//! game's code reads their inputs as only a paddle game does; a keypad if
//! it reads the fire button and both paddles' inputs, but never the
//! joystick.
//!
//! A paddle's turned by the stick or the D-pad, at a speed, and stays where
//! it's left; or by the pointer, as far across the picture as it is. A
//! keypad's keys are the keyboard's: with one keypad its digits, * and #;
//! with two, as Stella has them, the left's 1 2 3, Q W E, A S D, Z X C, and
//! the right's 8 9 0, I O P, K L ;, and , . /.

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
    Keypad,
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
            Named::Keypad => Controller::Keypad,
            _ => Controller::Joystick,
        }
    }

    /// The controller in the left port, `right` the right's, as Stella finds
    /// it in `image`: a keypad if the fire button's read and both paddles'
    /// inputs, but not the joystick; paddles if their inputs are read as
    /// only a paddle game reads them, even if it reads the joystick's button
    /// too.
    pub fn found(image: &[u8], right: bool) -> Controller {
        let (button, keypad, paddles) = if right {
            (
                right_button(image),
                right_keypad(image),
                right_paddles(image),
            )
        } else {
            (left_button(image), left_keypad(image), left_paddles(image))
        };
        if button && keypad && !joystick(image) {
            Controller::Keypad
        } else if paddles {
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

/// Whether the left port's fire button's read, INPT4.
fn left_button(image: &[u8]) -> bool {
    any(
        image,
        &[
            &[0x24, 0x0C, 0x10],
            &[0x24, 0x0C, 0x30],
            &[0xA5, 0x0C, 0x10],
            &[0xA5, 0x0C, 0x30],
            &[0xB5, 0x0C, 0x10],
            &[0xB5, 0x0C, 0x30],
            &[0x24, 0x3C, 0x10],
            &[0x24, 0x3C, 0x30],
            &[0xA5, 0x3C, 0x10],
            &[0xA5, 0x3C, 0x30],
            &[0xB5, 0x3C, 0x10],
            &[0xB5, 0x3C, 0x30],
            &[0xB4, 0x0C, 0x30],
            &[0xA5, 0x3C, 0x2A],
            &[0xA6, 0x3C, 0x8E],
            &[0xA6, 0x0C, 0x86],
            &[0xA6, 0x0C, 0x8E],
            &[0xA4, 0x3C, 0x8C],
            &[0xA5, 0x0C, 0x8D],
            &[0xA4, 0x0C, 0x30],
            &[0xA4, 0x3C, 0x30],
            &[0xA5, 0x0C, 0x25],
            &[0xA6, 0x3C, 0x30],
            &[0xA6, 0x0C, 0x30],
            &[0xA5, 0x0C, 0x0A],
            &[0xB5, 0x0C, 0x4A],
            &[0xB9, 0x0C, 0x00, 0x10],
            &[0xB9, 0x0C, 0x00, 0x30],
            &[0xB9, 0x3C, 0x00, 0x10],
            &[0xB9, 0x3C, 0x00, 0x30],
            &[0xA5, 0x0C, 0x0A, 0xB0],
            &[0xB5, 0x0C, 0x29, 0x80],
            &[0xB5, 0x3C, 0x29, 0x80],
            &[0xA5, 0x0C, 0x29, 0x80],
            &[0xA5, 0x3C, 0x29, 0x80],
            &[0xA5, 0x0C, 0x49, 0x80],
            &[0xB9, 0x0C, 0x00, 0x4A],
            &[0xA5, 0x0C, 0x25, 0x0D, 0x10],
            &[0xA5, 0x0C, 0x25, 0x0D, 0x30],
            &[0xA5, 0x3C, 0x25, 0x3D, 0x10],
            &[0xA5, 0x3C, 0x25, 0x3D, 0x30],
            &[0xB5, 0x38, 0x29, 0x80, 0xD0],
            &[0xA9, 0x80, 0x24, 0x0C, 0xD0],
            &[0xA5, 0x0C, 0x29, 0x80, 0xD0],
            &[0xA5, 0x3C, 0x29, 0x80, 0xD0],
            &[0xAD, 0x0C, 0x00, 0x29, 0x80],
            &[0xB9, 0x0C, 0x00, 0x29, 0x80],
        ],
    )
}

/// Whether the right port's fire button's read, INPT5, or INPT4 indexed.
fn right_button(image: &[u8]) -> bool {
    any(
        image,
        &[
            &[0x24, 0x0D, 0x10],
            &[0x24, 0x0D, 0x30],
            &[0xA5, 0x0D, 0x10],
            &[0xA5, 0x0D, 0x30],
            &[0xB5, 0x0C, 0x10],
            &[0xB5, 0x0C, 0x30],
            &[0x24, 0x3D, 0x10],
            &[0x24, 0x3D, 0x30],
            &[0xA5, 0x3D, 0x10],
            &[0xA5, 0x3D, 0x30],
            &[0xB5, 0x3C, 0x10],
            &[0xB5, 0x3C, 0x30],
            &[0xA4, 0x3D, 0x30],
            &[0xA5, 0x0D, 0x25],
            &[0xA6, 0x3D, 0x30],
            &[0xA6, 0x0D, 0x30],
            &[0xB9, 0x0C, 0x00, 0x10],
            &[0xB9, 0x0C, 0x00, 0x30],
            &[0xB9, 0x3C, 0x00, 0x10],
            &[0xB9, 0x3C, 0x00, 0x30],
            &[0xB5, 0x0C, 0x29, 0x80],
            &[0xB5, 0x3C, 0x29, 0x80],
            &[0xA5, 0x3D, 0x29, 0x80],
            &[0xB5, 0x38, 0x29, 0x80, 0xD0],
            &[0xA9, 0x80, 0x24, 0x0D, 0xD0],
            &[0xAD, 0x0D, 0x00, 0x29, 0x80],
        ],
    )
}

/// Whether the joystick's read, SWCHA, either port's.
fn joystick(image: &[u8]) -> bool {
    any(
        image,
        &[
            &[0xAD, 0x80, 0x02],
            &[0xAE, 0x80, 0x02],
            &[0xAC, 0x80, 0x02],
            &[0x2C, 0x80, 0x02],
            &[0x0D, 0x80, 0x02],
            &[0x2D, 0x80, 0x02],
            &[0x4D, 0x80, 0x02],
            &[0xAD, 0x88, 0x02],
        ],
    )
}

/// Whether a keypad's two columns on the paddles' inputs are read, INPT0
/// and INPT1, the left port's.
fn left_keypad(image: &[u8]) -> bool {
    let indexed: &[u8] = &[0xB5, 0x38, 0x29, 0x80, 0xD0];
    let first = any(
        image,
        &[
            &[0x24, 0x38, 0x30],
            &[0xA5, 0x38, 0x10],
            &[0xA4, 0x38, 0x30],
            &[0xB5, 0x38, 0x30],
            &[0x24, 0x08, 0x30],
            &[0x24, 0x08, 0x10],
            &[0xA6, 0x08, 0x30],
            indexed,
        ],
    );
    first
        && any(
            image,
            &[
                &[0x24, 0x39, 0x10],
                &[0x24, 0x39, 0x30],
                &[0xA5, 0x39, 0x10],
                &[0xA4, 0x39, 0x30],
                &[0xB5, 0x38, 0x30],
                &[0x24, 0x09, 0x30],
                &[0x24, 0x09, 0x10],
                &[0xA6, 0x09, 0x30],
                indexed,
            ],
        )
}

/// Whether a keypad's two columns on the paddles' inputs are read, INPT2
/// and INPT3, the right port's.
fn right_keypad(image: &[u8]) -> bool {
    let indexed: &[u8] = &[0xB5, 0x38, 0x29, 0x80, 0xD0];
    let first = any(
        image,
        &[
            &[0x24, 0x3A, 0x30],
            &[0xA5, 0x3A, 0x10],
            &[0xA4, 0x3A, 0x30],
            &[0x24, 0x0A, 0x30],
            &[0x24, 0x0A, 0x10],
            &[0xA6, 0x0A, 0x30],
            indexed,
        ],
    );
    first
        && any(
            image,
            &[
                &[0x24, 0x3B, 0x30],
                &[0xA5, 0x3B, 0x10],
                &[0xA4, 0x3B, 0x30],
                &[0x24, 0x0B, 0x30],
                &[0x24, 0x0B, 0x10],
                &[0xA6, 0x0B, 0x30],
                indexed,
            ],
        )
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

/// A keypad's column, as the keys pressed and its rows have it: a key
/// pressed in a row driven low grounding it; else one in a row high
/// pulling it up; else open, the keypad's own resistor pulling it up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Ground,
    Supply,
    Open,
}

/// The keypad's own pull-up on each column, in ohms, as Stella has it.
pub const KEYPAD_PULL_UP: u32 = 4_700;

/// A keypad's 3 columns, its keys `pressed` (bit n the nth of 1 to 9, *, 0
/// and #, a row of 3 at a time) and its rows' pins `rows` (bit 0 the top
/// row's, 1, 2 and 3).
pub fn columns(pressed: u16, rows: u8) -> [Column; 3] {
    std::array::from_fn(|column| {
        let keys = (0..4).filter(|row| pressed & 1 << (row * 3 + column) != 0);
        let (mut low, mut high) = (false, false);
        for row in keys {
            if rows & 1 << row == 0 {
                low = true;
            } else {
                high = true;
            }
        }
        match (low, high) {
            (true, _) => Column::Ground,
            (false, true) => Column::Supply,
            _ => Column::Open,
        }
    })
}

/// The keypad key `typed` is, `two` if there are two keypads: its port,
/// left 0 or right 1, and its bit, as `columns` has them.
pub fn keypad_key(typed: char, two: bool) -> Option<(usize, u16)> {
    const ONE: &str = "123456789*0#";
    const LEFT: &str = "123qweasdzxc";
    const RIGHT: &str = "890iopkl;,./";
    let typed = typed.to_ascii_lowercase();
    let at = |keys: &str| keys.find(typed).map(|n| 1 << n);
    if !two {
        return at(ONE).map(|key| (0, key));
    }
    at(LEFT)
        .map(|key| (0, key))
        .or_else(|| at(RIGHT).map(|key| (1, key)))
}
