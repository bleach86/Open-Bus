/// The host's sound: frames a second, for every machine; each frame its
/// channels' samples, left then right.
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// Where a machine's picture and sound go, and its keys and joystick come
/// from: the host's window, or its terminal.
pub trait Display {
    /// A finished frame: `width` by `height` pixels, 0xRRGGBB each, a line
    /// at a time from the top.
    fn show(&mut self, pixels: &[u32], width: usize, height: usize);

    /// Whether it shows text alone, a machine's text screen, not a picture;
    /// and plays nothing.
    fn text_only(&self) -> bool {
        false
    }

    /// A machine's text screen, with its escape sequences.
    fn text(&mut self, _text: &str) {}

    /// The next key pressed, if there is one; the machine makes of it what
    /// its own keyboard would.
    fn key(&mut self) -> Option<Key>;

    /// The joystick as it is now; centred and its buttons up, if there is
    /// none.
    fn joystick(&self) -> Joystick {
        Joystick::CENTRED
    }

    /// The gamepad as it is now, a console's; none of it held, if there is
    /// none.
    fn gamepad(&self) -> Gamepad {
        Gamepad::default()
    }

    /// Whether a key is held down, any but the modifiers and the function
    /// keys.
    fn key_down(&self) -> bool {
        false
    }

    /// Whether the machine's Reset key is held.
    fn reset_held(&self) -> bool {
        false
    }

    /// Whether the window has been closed: the machine to put itself away
    /// and turn off.
    fn closed(&self) -> bool {
        false
    }

    /// The next function key pressed, by its number, F1 to F10, for the
    /// machine's own uses (F11 is the window's fullscreen, F12 the
    /// machine's Reset key, held).
    fn function_key(&mut self) -> Option<u8> {
        None
    }

    /// The window's title: the machine's say, what disks are in it.
    fn title(&mut self, _title: &str) {}

    /// Sound to play: frames at SAMPLE_RATE, each a left sample then a
    /// right, -1 to 1, following on from the last.
    fn play(&mut self, _samples: &[f32]) {}
}

/// A joystick as the host has it: its stick, -1 to 1 each way, right and
/// down positive; and its buttons, pressed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Joystick {
    pub x: f32,
    pub y: f32,
    pub buttons: [bool; 3],
}

impl Joystick {
    pub const CENTRED: Joystick = Joystick { x: 0.0, y: 0.0, buttons: [false; 3] };
}

/// A gamepad as a console has it: its D-pad, its A and B, its Select and
/// Start, each held or not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gamepad {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub a: bool,
    pub b: bool,
    pub select: bool,
    pub start: bool,
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
