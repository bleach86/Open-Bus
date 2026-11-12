//! What Stella's game properties say of a game, by its ROM's MD5: those that
//! say anything, in stella.txt.

use std::collections::HashMap;
use std::sync::OnceLock;

const DATABASE: &str = include_str!("stella.txt");

/// A controller, as the properties name it: a joystick, paddles (turned by
/// the other axis, or that and the other way round), a driving controller,
/// a keypad, or something else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Controller {
    Joystick,
    Paddles,
    PaddlesOtherAxis,
    PaddlesOtherAxisReversed,
    Driving,
    Keypad,
    Other,
}

impl Controller {
    fn parse(name: &str) -> Option<Controller> {
        Some(match name {
            "" => return None,
            "JOYSTICK" | "GENESIS" | "BOOSTERGRIP" => Controller::Joystick,
            "PADDLES" => Controller::Paddles,
            "PADDLES_IAXIS" => Controller::PaddlesOtherAxis,
            "PADDLES_IAXDR" => Controller::PaddlesOtherAxisReversed,
            "DRIVING" => Controller::Driving,
            "KEYBOARD" => Controller::Keypad,
            _ => Controller::Other,
        })
    }
}

/// A game's properties: its first bank, its cartridge's type, its
/// difficulties (A), its ports swapped, its controllers, its paddles
/// swapped, its display format, and its phosphor and how much.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Known {
    pub start_bank: Option<u16>,
    pub kind: &'static str,
    pub difficulty_a: [Option<bool>; 2],
    pub swap_ports: bool,
    pub controllers: [Option<Controller>; 2],
    pub swap_paddles: bool,
    pub format: &'static str,
    pub phosphor: bool,
    pub blend: Option<u8>,
}

/// The game whose ROM's MD5 is `md5`, if it's known.
pub fn find(md5: &str) -> Option<&'static Known> {
    static GAMES: OnceLock<HashMap<&'static str, Known>> = OnceLock::new();
    GAMES
        .get_or_init(|| DATABASE.lines().filter_map(game).collect())
        .get(md5)
}

fn game(line: &'static str) -> Option<(&'static str, Known)> {
    if line.starts_with('#') {
        return None;
    }
    let fields: Vec<&str> = line.split('\t').collect();
    let [
        md5,
        bank,
        kind,
        left,
        right,
        swap,
        left_c,
        right_c,
        swap_p,
        format,
        phosphor,
        blend,
    ] = fields[..]
    else {
        return None;
    };
    let difficulty = |said: &str| match said {
        "A" => Some(true),
        "B" => Some(false),
        _ => None,
    };
    Some((
        md5,
        Known {
            start_bank: bank.parse().ok(),
            kind,
            difficulty_a: [difficulty(left), difficulty(right)],
            swap_ports: swap == "YES",
            controllers: [Controller::parse(left_c), Controller::parse(right_c)],
            swap_paddles: swap_p == "YES",
            format,
            phosphor: phosphor == "YES",
            blend: blend.parse().ok(),
        },
    ))
}
