//! The X16's keyboard, a PS/2 one, its keys as the SMC has them: IBM's key
//! numbers, sent as a key goes down, and with bit 7 set as it's let go.
//! The host gives characters typed, not keys: each is the keys a US
//! keyboard types it with, pressed and let go, Shift or Ctrl with it if it
//! takes one.

use crate::host::display::Key;

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
