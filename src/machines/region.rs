//! A TV standard, and what a game's file name says of it: NTSC (America's
//! and Japan's) or PAL (Europe's and Australia's). The machines run at their
//! own speeds on each, as their own modules say.
//!
//! A file's name, as No-Intro's and GoodTools' sets name them: its tags in
//! brackets, "(Europe)" and the like, or GoodTools' letters, "(E)". A game
//! of both, "(USA, Europe)", is one ROM for them, and runs as NTSC.

/// The first's the default: what a game's silence means, its header's or
/// its name's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Region {
    #[default]
    Ntsc,
    Pal,
}

/// A name's tags, in brackets, for each.
const PAL_TAGS: [&str; 11] = [
    "Europe",
    "Australia",
    "Germany",
    "France",
    "Spain",
    "Italy",
    "Sweden",
    "Netherlands",
    "PAL",
    "E",
    "A",
];
const NTSC_TAGS: [&str; 8] = ["USA", "Japan", "World", "Korea", "NTSC", "U", "J", "JU"];

impl Region {
    /// The region a file's `name` says, if it says one: PAL's tags only if
    /// there's no NTSC's with them.
    pub fn from_name(name: &str) -> Option<Region> {
        let tags = tags(name);
        if tags.iter().any(|tag| NTSC_TAGS.contains(tag)) {
            return Some(Region::Ntsc);
        }
        tags.iter()
            .any(|tag| PAL_TAGS.contains(tag))
            .then_some(Region::Pal)
    }

    /// As `--region=` names it.
    pub fn parse(name: &str) -> Option<Region> {
        match name {
            "ntsc" => Some(Region::Ntsc),
            "pal" => Some(Region::Pal),
            _ => None,
        }
    }
}

/// A name's tags: what's in its brackets, each a comma apart.
fn tags(name: &str) -> Vec<&str> {
    name.split('(')
        .skip(1)
        .filter_map(|group| group.split(')').next())
        .flat_map(|group| group.split(','))
        .map(str::trim)
        .collect()
}

/// Whether a file's `name` says it's a Famicom's game, if it says: Japan
/// alone, it is; another country, with Japan or not, it's the NES's, one
/// ROM of both being on its cartridge as much as the Famicom's.
pub fn famicoms(name: &str) -> Option<bool> {
    if japans_alone(name) {
        return Some(true);
    }
    let abroad = |tag: &&str| !matches!(*tag, "Japan" | "J");
    tags(name)
        .iter()
        .any(|tag| abroad(tag) && (NTSC_TAGS.contains(tag) || PAL_TAGS.contains(tag)))
        .then_some(false)
}

/// Whether a file's `name` says Japan alone, no other country with it: a
/// Famicom's game.
pub fn japans_alone(name: &str) -> bool {
    let japan = |tag: &&str| matches!(*tag, "Japan" | "J");
    let tags = tags(name);
    tags.iter().any(japan)
        && !tags
            .iter()
            .any(|tag| !japan(tag) && (NTSC_TAGS.contains(tag) || PAL_TAGS.contains(tag)))
}
