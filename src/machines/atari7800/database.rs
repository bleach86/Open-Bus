//! What ProSystem's database says of a game, by its ROM's MD5 (without an
//! .a78 header): its cartridge's board, its POKEY, its controllers and its
//! region, in prosystem.txt. And what a7800 has that ProSystem doesn't.

use super::cartridge::Kind;
use std::collections::HashMap;
use std::sync::OnceLock;

const DATABASE: &str = include_str!("prosystem.txt");

/// Rescue on Fractalus, whose RAM at $4000, every other page the same, a7800
/// has; ProSystem has it plain.
const MIRROR_RAM: &str = "8f7eb10ad0bd75474abf0c6c36c08486";

/// A game: its board, its POKEY's address, its controllers (1 a
/// joystick, 2 or 3 a light gun), whether it's PAL, and its title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Known {
    pub kind: Kind,
    pub pokey: Option<u16>,
    pub controllers: [u8; 2],
    pub pal: bool,
    pub title: &'static str,
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
    let [md5, kind, pokey, left, right, region, title] = fields[..] else {
        return None;
    };
    let kind = if md5 == MIRROR_RAM {
        Kind::MirrorRam
    } else {
        Kind::numbered(kind.parse().ok()?).ok()?
    };
    Some((
        md5,
        Known {
            kind,
            pokey: u16::from_str_radix(pokey, 16).ok(),
            controllers: [left.parse().ok()?, right.parse().ok()?],
            pal: region == "1",
            title,
        },
    ))
}
