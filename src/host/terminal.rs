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
use std::time::{Duration, Instant};

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

const CTRL_C: u8 = 0x03;

/// Whether `byte`, typed after `last`, is a key: not the line feed or NUL
/// a telnet client sends after a return as the end of a line, which would
/// be a second key and sit waiting where a machine looks for Ctrl-C.
pub fn typed(last: u8, byte: u8) -> bool {
    !(last == b'\r' && matches!(byte, b'\n' | 0))
}

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
                // Ctrl-D turns it off. Return as a return, not a line feed.
                stty(&["-icanon", "-echo", "-isig", "-icrnl"]);

                let rx_buffer = Arc::new(Mutex::new(VecDeque::new()));
                let rx_clone = Arc::clone(&rx_buffer);

                // Spawn a thread to read stdin asynchronously
                thread::spawn(move || {
                    let mut last = 0;
                    for b in stdin().lock().bytes().map_while(Result::ok) {
                        if b == 0x04 {
                            // Ctrl-D: off
                            power_off();
                        }
                        let is_key = typed(last, b);
                        last = b;
                        if !is_key {
                            continue;
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

    /// Whether Ctrl-C's been typed since it was last asked: the keys typed
    /// before it, and it, taken, as a break throws away what's typed ahead.
    /// For a machine that looks for Ctrl-C only in the key waiting, which a
    /// key typed before it would otherwise hide.
    pub fn take_break(&self) -> bool {
        let mut rx = self.rx_buffer.lock().unwrap();
        let Some(at) = rx.iter().position(|&b| b == CTRL_C) else {
            return false;
        };
        rx.drain(..=at);
        true
    }

    /// Ctrl-C typed, as the reader of stdin notes it.
    #[cfg(test)]
    pub fn type_break(&self) {
        self.rx_buffer.lock().unwrap().push_back(CTRL_C);
    }

    /// A key typed, for tests.
    #[cfg(test)]
    pub fn type_key(&self, key: u8) {
        self.rx_buffer.lock().unwrap().push_back(key);
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
            stty(&RESTORE);
        }
    }
}

/// The machine turned off, at once, even with output held up (a door's
/// caller behind): the terminal put back without waiting for it to drain.
pub fn power_off() -> ! {
    stty_soon(&RESTORE);
    std::process::exit(0)
}

/// What the machine writes, sent as it is: its own CR LF ends its lines,
/// the terminal adding no CR to the LF. A CR too many is a blank line to a
/// terminal that takes CR as a new line, as ROMTERM does, behind a BBS that
/// makes a door's LF CR LF itself.
pub fn raw_output() {
    stty(&["-opost"]);
}

/// The host terminal's modes as they were.
const RESTORE: [&str; 5] = ["icanon", "echo", "isig", "icrnl", "opost"];

/// Set the host terminal's modes; nothing to set when stdin isn't one.
fn stty(settings: &[&str]) {
    if stdin().is_terminal() {
        let _ = std::process::Command::new("stty").args(settings).status();
    }
}

/// Set them, waiting a fifth of a second at most: stty waits for what's
/// been written to be sent, which may not be soon.
fn stty_soon(settings: &[&str]) {
    if !stdin().is_terminal() {
        return;
    }
    let Ok(mut child) = std::process::Command::new("stty").args(settings).spawn() else {
        return;
    };
    let until = Instant::now() + Duration::from_millis(200);
    while Instant::now() < until {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
}
