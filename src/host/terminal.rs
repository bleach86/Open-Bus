//! The host's terminal, for the machine's terminal ports: the keys typed,
//! a key at a time, for the machine to take.

use std::collections::VecDeque;
use std::io::{IsTerminal, Read, stdin};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

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
                            rx_clone.lock().unwrap().push_back(b);
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
