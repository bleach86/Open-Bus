use crate::host::controls::Controls;

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

    /// A frame gone by with nothing to show: what's come in still taken in.
    fn no_picture(&mut self) {}

    /// The second player's, as the first's.
    fn gamepad_2(&self) -> Gamepad {
        Gamepad::default()
    }

    /// The pointer as it is now, a mouse's or a finger's, over the picture
    /// or not.
    fn pointer(&self) -> Pointer {
        Pointer::default()
    }

    /// Whether the machine's to be aimed at, as a light gun is: the pointer
    /// shown always, as cross hairs, and its clicks the machine's, none of
    /// them the display's own (a double click its fullscreen).
    fn aiming(&mut self, _on: bool) {}

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

    /// The next thing pressed, as a binding, if it's looking: a key, or a
    /// button of either gamepad. For the host, to bind.
    fn pressed(&mut self) -> Option<String> {
        None
    }

    /// Whether it can be closed by what's shown in it, as a window can; and
    /// closed, as if by its close button.
    fn can_close(&self) -> bool {
        false
    }

    fn close(&mut self) {}

    /// Whether it's to look for that, or for the controls as they are.
    fn capture(&mut self, _on: bool) {}

    /// Its controls, if it's a display whose can be changed.
    fn controls(&mut self) -> Option<&mut Controls> {
        None
    }

    /// What's to be said of it as a game starts, if anything is.
    fn hint(&self) -> Option<&'static str> {
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
    pub const CENTRED: Joystick = Joystick {
        x: 0.0,
        y: 0.0,
        buttons: [false; 3],
    };
}

/// A pointer as the host has it, a mouse's or a finger's: where it is on
/// the picture, in the picture's own pixels as it was last shown, if it's
/// over it; and its two buttons, held, the first and the second (a mouse's
/// left and right).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pointer {
    pub at: Option<(f32, f32)>,
    pub buttons: [bool; 2],
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
