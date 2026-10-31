//! The host's terminal, for the machine's terminal ports: the keys typed,
//! a key at a time, for the machine to take; and F12, the machine's Reset
//! key, pressed.

use std::collections::VecDeque;
use std::io::{IsTerminal, Read, stdin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

/// F12, as the terminal sends it.
const F12: &[u8] = b"\x1B[24~";

/// F12 pressed, and not yet taken.
static RESET_PRESSED: AtomicBool = AtomicBool::new(false);

#[derive(Default, Clone, Debug)]
pub struct Terminal {
    rx_buffer: Arc<Mutex<VecDeque<u8>>>,
    /// The first Terminal: it set up the host terminal, and puts it back.
    first: bool,
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
                    let stdin = stdin();
                    for byte in stdin.bytes() {
                        if let Ok(b) = byte {
                            if b == 0x04 {
                                // Ctrl-D: off
                                stty(&["icanon", "echo", "isig"]);
                                std::process::exit(0);
                            }
                            let mut rx = rx_clone.lock().unwrap();
                            rx.push_back(b);
                            // F12: the Reset key, not keys typed.
                            if rx.len() >= F12.len() && rx.iter().rev().take(F12.len()).eq(F12.iter().rev()) {
                                let typed = rx.len() - F12.len();
                                rx.truncate(typed);
                                RESET_PRESSED.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                });

                rx_buffer
            })
            .clone();

        Self { rx_buffer, first }
    }

    /// The next key typed, if there is one.
    pub fn key(&self) -> Option<u8> {
        self.rx_buffer.lock().unwrap().pop_front()
    }

    /// Whether the Reset key has been pressed since this was last asked.
    pub fn reset_pressed(&self) -> bool {
        RESET_PRESSED.swap(false, Ordering::Relaxed)
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
