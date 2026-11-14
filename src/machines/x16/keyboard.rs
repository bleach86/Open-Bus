//! The X16's keyboard, a PS/2 one, its keys as the SMC has them: IBM's key
//! numbers, sent as a key goes down (again as it repeats), and with bit 7
//! set as it's let go. The host's keys are each the one in its place, as
//! x16emu has them (`number`). What's pasted is characters, not keys: each
//! is the keys a US keyboard types it with, pressed and let go, Shift or
//! Ctrl with it if it takes one (`codes`).

use crate::host::display::{Key, Scancode};

const LEFT_SHIFT: u8 = 44;
const LEFT_CTRL: u8 = 58;
const UP: u8 = 83;
const DOWN: u8 = 84;
const LEFT: u8 = 79;
const RIGHT: u8 = 89;
/// A key let go, its number's bit 7 set.
const UP_BIT: u8 = 0x80;

/// The key a character's on, and whether it's shifted there, on a US
/// keyboard.
fn key_of(c: u8) -> Option<(u8, bool)> {
    const DIGITS: &[u8; 10] = b"1234567890";
    const SHIFTED_DIGITS: &[u8; 10] = b"!@#$%^&*()";
    // The letters' keys, A to Z.
    const LETTERS: [u8; 26] = [
        31, 50, 48, 33, 19, 34, 35, 36, 24, 37, 38, 39, 52, 51, 25, 26, 17, 20, 32, 21, 23, 49, 18,
        47, 22, 46,
    ];
    // The rest: each character, unshifted and shifted, and its key.
    const OTHERS: [(u8, u8, u8); 11] = [
        (b'`', b'~', 1),
        (b'-', b'_', 12),
        (b'=', b'+', 13),
        (b'[', b'{', 27),
        (b']', b'}', 28),
        (b'\\', b'|', 29),
        (b';', b':', 40),
        (b'\'', b'"', 41),
        (b',', b'<', 53),
        (b'.', b'>', 54),
        (b'/', b'?', 55),
    ];
    match c {
        b'a'..=b'z' => Some((LETTERS[(c - b'a') as usize], false)),
        b'A'..=b'Z' => Some((LETTERS[(c - b'A') as usize], true)),
        b' ' => Some((61, false)),
        13 => Some((43, false)),
        8 => Some((15, false)),
        9 => Some((16, false)),
        27 => Some((110, false)),
        _ => {
            if let Some(n) = DIGITS.iter().position(|&d| d == c) {
                return Some((n as u8 + 2, false));
            }
            if let Some(n) = SHIFTED_DIGITS.iter().position(|&d| d == c) {
                return Some((n as u8 + 2, true));
            }
            OTHERS.iter().find_map(|&(plain, shifted, key)| {
                (c == plain || c == shifted).then_some((key, c == shifted))
            })
        }
    }
}

/// The X16's key in the host's key's place, by its USB HID usage, if it
/// has one there.
pub fn number(key: Scancode) -> Option<u8> {
    // The letters', A to Z, and the digits', 1 to 9 then 0, in HID's order.
    const LETTERS: [u8; 26] = [
        31, 50, 48, 33, 19, 34, 35, 36, 24, 37, 38, 39, 52, 51, 25, 26, 17, 20, 32, 21, 23, 49, 18,
        47, 22, 46,
    ];
    let usage = key.0;
    Some(match usage {
        0x04..=0x1D => LETTERS[(usage - 0x04) as usize],
        0x1E..=0x27 => (usage - 0x1E) as u8 + 2,
        0x28 => 43,                                // Return
        0x29 => 110,                               // Escape
        0x2A => 15,                                // Backspace
        0x2B => 16,                                // Tab
        0x2C => 61,                                // Space
        0x2D => 12,                                // -
        0x2E => 13,                                // =
        0x2F => 27,                                // [
        0x30 => 28,                                // ]
        0x31 => 29,                                // \
        0x33 => 40,                                // ;
        0x34 => 41,                                // '
        0x35 => 1,                                 // `
        0x36 => 53,                                // ,
        0x37 => 54,                                // .
        0x38 => 55,                                // /
        0x39 => 30,                                // Caps Lock
        0x3A..=0x45 => (usage - 0x3A) as u8 + 112, // F1 to F12
        0x47 => 125,                               // Scroll Lock
        0x48 => 126,                               // Pause
        0x49 => 75,                                // Insert
        0x4A => 80,                                // Home
        0x4B => 85,                                // Page Up
        0x4C => 76,                                // Delete
        0x4D => 81,                                // End
        0x4E => 86,                                // Page Down
        0x4F => RIGHT,
        0x50 => LEFT,
        0x51 => DOWN,
        0x52 => UP,
        0x53 => 90,  // Num Lock
        0x54 => 95,  // keypad /
        0x55 => 100, // keypad *
        0x56 => 105, // keypad -
        0x57 => 106, // keypad +
        0x58 => 108, // keypad Enter
        0x59 => 93,  // keypad 1, and on
        0x5A => 98,
        0x5B => 103,
        0x5C => 92,
        0x5D => 97,
        0x5E => 102,
        0x5F => 91,
        0x60 => 96,
        0x61 => 101,
        0x62 => 99,  // keypad 0
        0x63 => 104, // keypad .
        0x64 => 45,  // the key beside Left Shift, off US keyboards
        0x65 => 65,  // Menu
        0x87 => 56,  // International 1
        0xE0 => LEFT_CTRL,
        0xE1 => LEFT_SHIFT,
        0xE2 => 60, // Left Alt
        0xE3 => 59, // Left GUI
        0xE4 => 64, // Right Ctrl
        0xE5 => 57, // Right Shift
        0xE6 => 62, // Right Alt
        0xE7 => 63, // Right GUI
        _ => return None,
    })
}

/// The code for the key in `key`'s place, pressed (`down`) or let go.
pub fn change(key: Scancode, down: bool) -> Option<u8> {
    number(key).map(|number| if down { number } else { number | UP_BIT })
}

/// The codes `key` is typed with: its key down and up, and Shift's or
/// Ctrl's around it if it takes one; none if it's not on the keyboard.
pub fn codes(key: Key) -> Vec<u8> {
    let (number, held) = match key {
        Key::Up => (UP, None),
        Key::Down => (DOWN, None),
        Key::Left => (LEFT, None),
        Key::Right => (RIGHT, None),
        // Ctrl and a letter, but for those that are keys of their own.
        Key::Char(c @ 1..=26) if !matches!(c, 8 | 9 | 13) => {
            (key_of(c + b'a' - 1).unwrap().0, Some(LEFT_CTRL))
        }
        Key::Char(c) => match key_of(c) {
            Some((number, shifted)) => (number, shifted.then_some(LEFT_SHIFT)),
            None => return Vec::new(),
        },
    };
    match held {
        Some(held) => vec![held, number, number | UP_BIT, held | UP_BIT],
        None => vec![number, number | UP_BIT],
    }
}
