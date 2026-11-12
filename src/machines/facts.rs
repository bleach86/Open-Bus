//! What's known of a game's file without running it, whichever machine's it
//! is, for a library to keep: a line of its CRC-32, its board (a NES's
//! mapper, "MAPPER 4 MMC3"; a 2600's cartridge, "ATARI 2600 F8 8K"),
//! whether it's a Famicom's and a Zapper's, why it won't load if it won't,
//! and its path, a tab between each, as `cpu_emu facts` says it.
//!
//! Whose it is, by its file: a 2600's an .a26 or .bin, or a zip of one; the
//! rest, a NES's.

use crate::machines::{atari2600, nes};
use std::path::Path;

/// The machines whose games a library has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Nes,
    Atari2600,
}

/// Whether a file's `name` is a 2600's game.
fn atari_name(name: &str) -> bool {
    let name = name.to_lowercase();
    name.ends_with(".a26") || name.ends_with(".bin")
}

/// Whose game the file at `path` is, its `bytes`: a zip's by what's in it.
pub fn system_of(path: &Path, bytes: &[u8]) -> System {
    let name = path.to_string_lossy();
    let zipped = name.to_lowercase().ends_with(".zip");
    let atari = if zipped {
        crate::host::zip::names(bytes).is_ok_and(|names| names.iter().any(|name| atari_name(name)))
    } else {
        atari_name(&name)
    };
    if atari {
        System::Atari2600
    } else {
        System::Nes
    }
}

/// The facts of the game at `path`, its file's `bytes`.
pub fn facts_of(path: &Path, bytes: Vec<u8>) -> String {
    match system_of(path, &bytes) {
        System::Nes => nes::facts_of(path, nes::image_of(path, bytes)),
        System::Atari2600 => atari2600::facts_of(path, atari2600::image_of(path, bytes)),
    }
}

/// The facts of the game at `path`.
pub fn facts(path: &Path) -> String {
    match crate::host::files::read(path) {
        Ok(bytes) => facts_of(path, bytes),
        Err(problem) => nes::facts_of(path, Err(problem.to_string())),
    }
}

/// `cpu_emu facts`: `facts` of each file named on standard input, a line
/// each, as soon as it's known.
#[cfg(not(target_arch = "wasm32"))]
pub fn report() {
    use std::io::{BufRead, Write};
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        if writeln!(out, "{}", facts(Path::new(&line)))
            .and_then(|_| out.flush())
            .is_err()
        {
            return;
        }
    }
}
