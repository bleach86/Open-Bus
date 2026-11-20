//! The host's terminal, for the machine's terminal ports: the keys typed,
//! a key at a time, for the machine to take; and the function keys
//! pressed, apart, for the machine's own uses. Or a display for a machine
//! with a text screen: its screen drawn when it changes, the arrows' escape
//! sequences keys, F12 its Reset key.

use crate::host::display::{Display, Key};
use std::collections::VecDeque;
use std::io::{IsTerminal, Read, Write, stdin, stdout};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

/// F1 to F12, as the terminal sends them.
const FUNCTION_KEYS: [&[u8]; 12] = [
    b"\x1BOP",
    b"\x1BOQ",
    b"\x1BOR",
    b"\x1BOS",
    b"\x1B[15~",
    b"\x1B[17~",
    b"\x1B[18~",
    b"\x1B[19~",
    b"\x1B[20~",
    b"\x1B[21~",
    b"\x1B[23~",
    b"\x1B[24~",
];

/// The Reset key pressed, there being no key let go: held 6 frames, a tenth
/// of a second.
const RESET_FRAMES: u32 = 6;

/// The function keys pressed, by number, not yet taken.
static FUNCTION_KEYS_PRESSED: Mutex<VecDeque<u8>> = Mutex::new(VecDeque::new());

#[derive(Default, Clone, Debug)]
pub struct Terminal {
    rx_buffer: Arc<Mutex<VecDeque<u8>>>,
    /// The first Terminal: it set up the host terminal, and puts it back.
    first: bool,
    /// The screen as last drawn; the frames more the Reset key's held.
    shown: String,
    reset_frames: u32,
}

impl Terminal {
    pub fn new() -> Self {
        // One reader of stdin for the process: another Terminal's thread
        // would take this one's keys.
        static RX_BUFFER: OnceLock<Arc<Mutex<VecDeque<u8>>>> = OnceLock::new();

        let mut first = false;
        let rx_buffer = RX_BUFFER
            .get_or_init(|| {
                first = true;

                // A key at a time and not echoed: the machine echoes what it
                // reads. Ctrl-C comes in as a key too, for the machine;
                // Ctrl-D turns it off.
                stty(&["-icanon", "-echo", "-isig"]);

                let rx_buffer = Arc::new(Mutex::new(VecDeque::new()));
                let rx_clone = Arc::clone(&rx_buffer);

                // Spawn a thread to read stdin asynchronously
                thread::spawn(move || {
                    for b in stdin().lock().bytes().map_while(Result::ok) {
                        if b == 0x04 {
                            // Ctrl-D: off
                            stty(&["icanon", "echo", "isig"]);
                            std::process::exit(0);
                        }
                        let mut rx = rx_clone.lock().unwrap();
                        rx.push_back(b);
                        // A function key: not keys typed.
                        for (n, sequence) in FUNCTION_KEYS.iter().enumerate() {
                            if rx.len() >= sequence.len()
                                && rx
                                    .iter()
                                    .rev()
                                    .take(sequence.len())
                                    .eq(sequence.iter().rev())
                            {
                                let typed = rx.len() - sequence.len();
                                rx.truncate(typed);
                                FUNCTION_KEYS_PRESSED.lock().unwrap().push_back(n as u8 + 1);
                            }
                        }
                    }
                });

                rx_buffer
            })
            .clone();

        Self {
            rx_buffer,
            first,
            shown: String::new(),
            reset_frames: 0,
        }
    }

    /// The next key typed, if there is one.
    pub fn key(&self) -> Option<u8> {
        self.rx_buffer.lock().unwrap().pop_front()
    }

    /// The next function key pressed, by its number, 1 to 12.
    pub fn function_key(&self) -> Option<u8> {
        FUNCTION_KEYS_PRESSED.lock().unwrap().pop_front()
    }
}

impl Display for Terminal {
    fn show(&mut self, _pixels: &[u32], _width: usize, _height: usize) {}

    fn text_only(&self) -> bool {
        true
    }

    /// `text` drawn, if it has changed: the screen cleared for it first of
    /// all.
    fn text(&mut self, text: &str) {
        self.reset_frames = self.reset_frames.saturating_sub(1);
        if text == self.shown {
            return;
        }
        if self.shown.is_empty() {
            print!("\x1B[2J");
        }
        print!("{text}");
        let _ = stdout().flush();
        self.shown = text.to_string();
    }

    /// The next key: Enter a return; an arrow's escape sequence, the arrow;
    /// another, nothing; Esc on its own, Esc.
    fn key(&mut self) -> Option<Key> {
        Some(match Terminal::key(self)? {
            b'\n' | b'\r' => Key::Char(13),
            0x1B => match Terminal::key(self) {
                Some(b'[') | Some(b'O') => match Terminal::key(self)? {
                    b'A' => Key::Up,
                    b'B' => Key::Down,
                    b'C' => Key::Right,
                    b'D' => Key::Left,
                    _ => return None,
                },
                Some(_) => return None,
                None => Key::Char(0x1B),
            },
            key => Key::Char(key),
        })
    }

    fn reset_held(&self) -> bool {
        self.reset_frames > 0
    }

    /// The next function key, F12 the Reset key.
    fn function_key(&mut self) -> Option<u8> {
        loop {
            match Terminal::function_key(self)? {
                12 => self.reset_frames = RESET_FRAMES,
                key => return Some(key),
            }
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if self.first {
            stty(&["icanon", "echo", "isig"]);
        }
    }
}

/// Set the host terminal's modes; nothing to set when stdin isn't one.
fn stty(settings: &[&str]) {
    if stdin().is_terminal() {
        let _ = std::process::Command::new("stty").args(settings).status();
    }
}
