/// The host's sound: samples a second, for every machine.
pub const SAMPLE_RATE: u32 = 48_000;

/// Where a machine's picture and sound go, and its keys and joystick come
/// from: the host's window. A machine without one (its tests) shows and
/// plays nothing.
pub trait Display {
    /// A finished frame: `width` by `height` pixels, 0xRRGGBB each, a line
    /// at a time from the top.
    fn show(&mut self, pixels: &[u32], width: usize, height: usize);

    /// The next key pressed, if there is one; the machine makes of it what
    /// its own keyboard would.
    fn key(&mut self) -> Option<Key>;

    /// The joystick as it is now; centred and its buttons up, if there is
    /// none.
    fn joystick(&self) -> Joystick {
        Joystick::CENTRED
    }

    /// Sound to play: samples at SAMPLE_RATE, -1 to 1, following on from
    /// the last.
    fn play(&mut self, _samples: &[f32]) {}
}

/// A joystick as the host has it: its stick, -1 to 1 each way, right and
/// down positive; and its buttons, pressed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joystick {
    pub x: f32,
    pub y: f32,
    pub buttons: [bool; 3],
}

impl Joystick {
    pub const CENTRED: Joystick = Joystick { x: 0.0, y: 0.0, buttons: [false; 3] };
}

/// A key as the host has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    /// A character typed, as ASCII, shift applied; control keys too: Return
    /// 13, Backspace 8, Escape 27, Ctrl and a letter 1 to 26.
    Char(u8),
    Left,
    Right,
    Up,
    Down,
}
