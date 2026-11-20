use crate::host::controls::{Controls, said};

/// The host's sound: frames a second, for every machine; each frame its
/// channels' samples, left then right.
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// What's pasted typed on the machine's keyboard; and its screen's text
/// copied: as a function key would be, past F1 to F12, for the host's
/// own. Ctrl+Shift+V or Shift+Insert, and Ctrl+Shift+C or Ctrl+Insert, as
/// a terminal has them: Ctrl+C's the machine's.
pub const PASTE_KEY: u8 = 21;
pub const COPY_KEY: u8 = 22;

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

    /// Whether the machine takes the keyboard's keys as they're pressed and
    /// let go (`key_change`), not just the characters they type: none of
    /// them the gamepads' then.
    fn raw_keys(&mut self, _on: bool) {}

    /// The next key pressed or let go, as `raw_keys` has them, pressed
    /// again as it repeats; but those the display keeps for its own uses.
    fn key_change(&mut self) -> Option<(Scancode, bool)> {
        None
    }

    /// Whether the machine has a mouse: the host's captured for it once
    /// it's clicked on the picture, its cursor gone and its moves and
    /// clicks the machine's, till it's let go; not captured, it's none of
    /// the machine's. Told again as it changes: no longer, let go.
    fn mouse(&mut self, _on: bool) {}

    /// Whether a key is held down, any but the modifiers and the function
    /// keys.
    fn key_down(&self) -> bool {
        false
    }

    /// Whether what's bound to fast-forward is held.
    fn fast_forward(&self) -> bool {
        false
    }

    /// Whether what's bound to rewind is held.
    fn rewinding(&self) -> bool {
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

    /// What's said of a binding on the screen: a gamepad's face buttons, by
    /// what's printed on them, if it knows the pad.
    fn said(&self, source: &str) -> String {
        said(source)
    }

    /// What's to be said of it as a game starts, if anything is.
    fn hint(&self) -> Option<&'static str> {
        None
    }

    /// The window's title: the machine's say, what disks are in it.
    fn title(&mut self, _title: &str) {}

    /// Whether it has the computer's clipboard, for the menu to paste from
    /// and copy to.
    fn clipboard(&self) -> bool {
        false
    }

    /// What's to be pasted, as PASTE_KEY's pressed: what's on the
    /// clipboard, or what the page was given.
    fn clipboard_text(&mut self) -> Option<String> {
        None
    }

    /// `text` copied, to the clipboard, or for the page to put there.
    fn copy_text(&mut self, _text: &str) {}

    /// Sound to play: frames at SAMPLE_RATE, each a left sample then a
    /// right, -1 to 1, following on from the last.
    fn play(&mut self, _samples: &[f32]) {}

    /// The display's refresh rate, a second, if pictures can be shown in
    /// step with it.
    fn refresh_rate(&self) -> Option<f64> {
        None
    }

    /// Pictures shown in step with the display's refresh, each waiting for
    /// it, or not.
    fn keep_in_step(&mut self, _on: bool) {}

    /// The seconds spent waiting for the display's refresh since last asked:
    /// time the host's idle, not working.
    fn waited(&mut self) -> f64 {
        0.0
    }
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
/// left and right). A mouse captured for a machine that has one isn't
/// stopped by the picture's edges: it's where its moves have taken it from
/// where it was captured, however far.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pointer {
    pub at: Option<(f32, f32)>,
    pub buttons: [bool; 2],
}

/// A key on the host's keyboard, by where it is, not what it types: its
/// USB HID usage, the keyboard page's, as SDL's scancodes have it and a web
/// page's `KeyboardEvent.code` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Scancode(pub u16);

impl Scancode {
    /// F1 to F12, `n` 1 to 12.
    pub fn function(n: u16) -> Scancode {
        Scancode(0x39 + n)
    }
}

/// A gamepad as a console has it: its D-pad, its A and B, its Select and
/// Start, each held or not; and its turbo A and B, held, which the host
/// makes A and B pressed and let go by turns, as a turbo controller's are.
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
    pub turbo_a: bool,
    pub turbo_b: bool,
    /// The menus' choosing and going back, as they're bound: the first
    /// player's, for the menu over a game.
    pub accept: bool,
    pub back: bool,
}

impl Gamepad {
    /// As a machine's to have it, `frame` the host's count of them: its
    /// turbo pressing A and B for TURBO_FRAMES, and letting go for as many.
    pub fn turbo(self, frame: u32) -> Gamepad {
        let pressed = (frame / TURBO_FRAMES).is_multiple_of(2);
        Gamepad {
            a: self.a || self.turbo_a && pressed,
            b: self.b || self.turbo_b && pressed,
            turbo_a: false,
            turbo_b: false,
            ..self
        }
    }
}

/// How long turbo holds a button, and lets it go: 15 presses a second, each
/// seen by a game that reads its controller once a frame, and each letting
/// go too.
pub const TURBO_FRAMES: u32 = 2;

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
