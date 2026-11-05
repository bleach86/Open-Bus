//! The Apple's ROMs, wherever they are and whatever they're called. Each is
//! known by its CRC-32, not its name: in a file by itself, or a file of
//! several, a IIe's two halves together, or the video ROM twice over; in a
//! zip, as MAME keeps them; or a folder of any of those. The II+'s comes as
//! a whole or as its six 2K chips, and they're put together.
//!
//! They come out under the one name each, as the machines look for them:
//! `Sorted::bytes`. The ROMs themselves aren't here, only their checksums.

use super::iie;
use crate::host::files;
use crate::host::zip::{self, crc32};
use std::path::Path;

/// The II+'s whole ROM, by the name it's given.
pub const PLUS_ROM: &str = "Apple2_Plus.rom";
/// The Disk II's two.
pub const DISK_II_P5: &str = "disk2-p5-341-0027.bin";
pub const DISK_II_P6: &str = "disk2-p6-341-0028.bin";

/// The most a file's looked in; a zip's, and the most of what's in one:
/// past them, a disk or a game.
const MOST: usize = 0x1_0000;
const MOST_ZIP: usize = 0x40_0000;

/// A ROM known: its CRC-32, its size, and what it is.
#[derive(Clone, Copy)]
pub struct Rom {
    crc: u32,
    size: usize,
    is: Is,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Is {
    /// A ROM by itself, by its name.
    Named(&'static str),
    /// The II+'s chip for this many 2K from $D000.
    PlusChip(usize),
}

pub const fn rom(crc: u32, size: usize, is: Is) -> Rom {
    Rom { crc, size, is }
}

const KNOWN: [Rom; 15] = [
    rom(0x443A_A7C4, 0x2000, Is::Named(iie::ENHANCED_ROMS[0])),
    rom(0x95E1_0034, 0x2000, Is::Named(iie::ENHANCED_ROMS[1])),
    rom(0x2651_014D, 0x1000, Is::Named(iie::ENHANCED_ROMS[2])),
    rom(0xE248_835E, 0x2000, Is::Named(iie::ROMS[0])),
    rom(0xFC3D_59D8, 0x2000, Is::Named(iie::ROMS[1])),
    rom(0x816A_86F1, 0x1000, Is::Named(iie::ROMS[2])),
    // Another revision of it, as MAME has it: the same but from $60 up.
    rom(0xB081_DF66, 0x1000, Is::Named(iie::ROMS[2])),
    rom(0xCE71_44F6, 0x100, Is::Named(DISK_II_P5)),
    rom(0xB72A_2C70, 0x100, Is::Named(DISK_II_P6)),
    rom(0x6F05_F949, 0x800, Is::PlusChip(0)),
    rom(0x1F08_087C, 0x800, Is::PlusChip(1)),
    rom(0x2B8D_9A89, 0x800, Is::PlusChip(2)),
    rom(0x5719_871A, 0x800, Is::PlusChip(3)),
    rom(0x9A04_EECF, 0x800, Is::PlusChip(4)),
    rom(0x0795_89C4, 0x800, Is::PlusChip(5)),
];

/// What's been found.
#[derive(Default)]
pub struct Sorted {
    roms: Vec<(&'static str, Vec<u8>)>,
    plus_chips: [Option<Vec<u8>>; 6],
}

impl Sorted {
    pub fn new() -> Self {
        Sorted::default()
    }

    /// `bytes`, the file `name`, looked through: whether there were any
    /// ROMs in it.
    pub fn add(&mut self, name: &str, bytes: &[u8]) -> bool {
        self.add_of(&KNOWN, name, bytes)
    }

    pub fn add_of(&mut self, known: &[Rom], name: &str, bytes: &[u8]) -> bool {
        if name.to_lowercase().ends_with(".zip") {
            let Ok(members) = zip::small_files(bytes, MOST) else { return false };
            return members.iter().fold(false, |found, (_, member)| self.look_in(known, member) | found);
        }
        self.look_in(known, bytes)
    }

    /// Each ROM of `known` looked for at each place it could start, the
    /// biggest first: the IIe has Applesoft too, so a piece of a ROM of its
    /// is the same as a II+'s chip, which that isn't.
    fn look_in(&mut self, known: &[Rom], bytes: &[u8]) -> bool {
        if bytes.len() > MOST {
            return false;
        }
        let mut found = false;
        for size in [0x2000, 0x1000, 0x800, 0x100] {
            if bytes.len() < size || (found && size == 0x800) {
                continue;
            }
            let sized: Vec<&Rom> = known.iter().filter(|rom| rom.size == size).collect();
            for piece in bytes.chunks_exact(size) {
                let crc = crc32(piece);
                if let Some(rom) = sized.iter().find(|rom| rom.crc == crc) {
                    self.found(rom.is, piece);
                    found = true;
                }
            }
        }
        found
    }

    fn found(&mut self, is: Is, piece: &[u8]) {
        match is {
            Is::PlusChip(chip) => self.plus_chips[chip] = Some(piece.to_vec()),
            Is::Named(name) if !self.roms.iter().any(|(known, _)| *known == name) => self.roms.push((name, piece.to_vec())),
            Is::Named(_) => {}
        }
    }

    /// The II+'s ROM, if all of its chips are here.
    fn plus_rom(&self) -> Option<Vec<u8>> {
        let chips: Option<Vec<&Vec<u8>>> = self.plus_chips.iter().map(Option::as_ref).collect();
        Some(chips?.into_iter().flatten().copied().collect())
    }

    /// How many of the II+'s chips are here, of the 6, to tell of some.
    pub fn plus_chips(&self) -> usize {
        self.plus_chips.iter().flatten().count()
    }

    /// The names of the ROMs found, a whole II+'s among them.
    pub fn names(&self) -> Vec<String> {
        let plus = self.plus_rom().map(|_| PLUS_ROM.to_string());
        self.roms.iter().map(|(name, _)| name.to_string()).chain(plus).collect()
    }

    /// The ROM of that name, if it's been found.
    pub fn bytes(&self, name: &str) -> Option<Vec<u8>> {
        if name == PLUS_ROM {
            return self.plus_rom();
        }
        self.roms.iter().find(|(known, _)| *known == name).map(|(_, bytes)| bytes.clone())
    }
}

/// The ROMs in `path`: a folder of them, or a file or zip of them and what's
/// beside it.
pub fn scan(path: &Path) -> Sorted {
    let mut sorted = Sorted::new();
    let mut look = |path: &Path| {
        let most = if path.extension().is_some_and(|kind| kind.eq_ignore_ascii_case("zip")) { MOST_ZIP } else { MOST };
        if files::size(path).is_some_and(|size| size as usize <= most) {
            if let Ok(bytes) = files::read(path) {
                sorted.add(&path.to_string_lossy(), &bytes);
            }
        }
    };
    let folder = if files::is_folder(path) {
        path
    } else {
        look(path);
        path.parent().unwrap_or(Path::new("."))
    };
    for file in files::list(folder).unwrap_or_default() {
        look(&file);
    }
    sorted
}
