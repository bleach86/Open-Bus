//! Input, `$FF20`: the gamepad's buttons as bits, the keyboard's keys one at
//! a time, the keys held as pad buttons (KEYPAD, for games), and the reset
//! button. The host side (`screen.rs`, `gamepad.rs`)
//! fills it in; what a button means is up to the machine's software.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};

pub const PAD_A: u16 = 1 << 0;
pub const PAD_B: u16 = 1 << 1;
pub const PAD_X: u16 = 1 << 2;
pub const PAD_Y: u16 = 1 << 3;
pub const PAD_L1: u16 = 1 << 4;
pub const PAD_R1: u16 = 1 << 5;
pub const PAD_L2: u16 = 1 << 6;
pub const PAD_R2: u16 = 1 << 7;
pub const PAD_SELECT: u16 = 1 << 8;
pub const PAD_START: u16 = 1 << 9;
pub const PAD_L3: u16 = 1 << 10;
pub const PAD_R3: u16 = 1 << 11;
pub const PAD_UP: u16 = 1 << 12;
pub const PAD_DOWN: u16 = 1 << 13;
pub const PAD_LEFT: u16 = 1 << 14;
pub const PAD_RIGHT: u16 = 1 << 15;

/// Key codes beyond ASCII.
pub const KEY_UP: u8 = 0x80;
pub const KEY_DOWN: u8 = 0x81;
pub const KEY_LEFT: u8 = 0x82;
pub const KEY_RIGHT: u8 = 0x83;

const PAD: u16 = 0xFF20;
const PAD_HI: u16 = 0xFF21;
const KEY_CODE: u16 = 0xFF22;
const KEY_STATUS: u16 = 0xFF23;
const KEY_WAITING: u8 = 0x01;
const KEYPAD: u16 = 0xFF24;
const KEYPAD_HI: u16 = 0xFF25;

/// What the host has to say, shared with the machine.
#[derive(Debug, Default)]
pub struct Input {
    /// Buttons held.
    pub pad: AtomicU16,
    /// Keys held, as the pad buttons they stand for (screen.rs maps them).
    pub keypad: AtomicU16,
    keys: Mutex<VecDeque<u8>>,
    /// The queue's length, so the machine can poll it without locking.
    pending: AtomicUsize,
    /// The reset button was pressed.
    pub reset: AtomicBool,
}

impl Input {
    pub fn press_key(&self, key: u8) {
        let mut keys = self.keys.lock().unwrap();
        keys.push_back(key);
        self.pending.store(keys.len(), Ordering::Release);
    }

    /// Drop the keys not yet taken, e.g. at reset.
    pub fn clear_keys(&self) {
        self.keys.lock().unwrap().clear();
        self.pending.store(0, Ordering::Release);
    }

    /// Keys typed that the machine has not taken yet.
    #[cfg(test)]
    pub fn keys_waiting(&self) -> bool {
        self.pending.load(Ordering::Acquire) != 0
    }

    pub fn take_reset(&self) -> bool {
        self.reset.swap(false, Ordering::AcqRel)
    }

    fn pop_key(&self) -> Option<u8> {
        if self.pending.load(Ordering::Acquire) == 0 {
            return None;
        }
        let mut keys = self.keys.lock().unwrap();
        let key = keys.pop_front();
        self.pending.store(keys.len(), Ordering::Release);
        key
    }
}

/// The machine's side: the key it holds until the software takes it.
#[derive(Debug, Default)]
pub struct InputPort {
    key: Option<u8>,
}

impl InputPort {
    pub fn read(&mut self, input: &Input, addr: u16) -> u8 {
        if self.key.is_none() {
            self.key = input.pop_key();
        }
        let pad = input.pad.load(Ordering::Relaxed).to_le_bytes();
        let keypad = input.keypad.load(Ordering::Relaxed).to_le_bytes();
        match addr {
            PAD => pad[0],
            PAD_HI => pad[1],
            KEYPAD => keypad[0],
            KEYPAD_HI => keypad[1],
            KEY_CODE => self.key.unwrap_or(0),
            KEY_STATUS => self.key.map_or(0, |_| KEY_WAITING),
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16) {
        if addr == KEY_STATUS {
            self.key = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_wait_until_taken_and_buttons_are_bits() {
        let input = Input::default();
        let mut port = InputPort::default();
        assert_eq!(port.read(&input, KEY_STATUS), 0);
        input.press_key(b'a');
        input.press_key(b'b');
        assert_eq!(port.read(&input, KEY_STATUS), KEY_WAITING);
        assert_eq!(port.read(&input, KEY_CODE), b'a');
        assert_eq!(port.read(&input, KEY_CODE), b'a', "held until taken");
        port.write(KEY_STATUS);
        assert_eq!(port.read(&input, KEY_CODE), b'b');
        port.write(KEY_STATUS);
        assert_eq!(port.read(&input, KEY_STATUS), 0);

        input.pad.store(PAD_A | PAD_START | PAD_LEFT, Ordering::Relaxed);
        assert_eq!(port.read(&input, PAD), PAD_A as u8);
        assert_eq!(port.read(&input, PAD_HI), ((PAD_START | PAD_LEFT) >> 8) as u8);

        input.keypad.store(PAD_B | PAD_RIGHT, Ordering::Relaxed);
        assert_eq!([port.read(&input, KEYPAD), port.read(&input, KEYPAD_HI)], [PAD_B as u8, (PAD_RIGHT >> 8) as u8]);
        assert_eq!(port.read(&input, PAD), PAD_A as u8, "the pad, apart");
    }
}
