//! What's known of a game's file without running it, whichever machine's it
//! is, for a library to keep: a line of its CRC-32, its board (a NES's
//! mapper, "MAPPER 4 MMC3"; a 2600's cartridge, "ATARI 2600 F8 8K"; a
//! 7800's, "ATARI 7800 SUPERGAME 128K POKEY"),
//! whether it's a Famicom's and a Zapper's, why it won't load if it won't,
//! and its path, a tab between each, as `cpu_emu facts` says it.
//!
//! Whose it is, by its file: a 7800's an .a78, a 2600's an .a26, or a zip of
//! one; a .bin either, as its bytes say; the rest, a NES's.

use crate::machines::{atari2600, atari7800, nes};
use std::path::Path;

/// The machines whose games a library has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Nes,
    Atari2600,
    Atari7800,
}

/// Whose game a file's `name` says it is, if it says: a 7800's, or a 2600's
/// (a .bin's, unless its bytes say a 7800's).
fn named(name: &str) -> Option<System> {
    let name = name.to_lowercase();
    if name.ends_with(".a78") {
        Some(System::Atari7800)
    } else if name.ends_with(".a26") || name.ends_with(".bin") {
        Some(System::Atari2600)
    } else {
        None
    }
}

/// Whose game the file at `path` is, its `bytes`: a zip's by what's in it.
pub fn system_of(path: &Path, bytes: &[u8]) -> System {
    let name = path.to_string_lossy();
    let zipped = name.to_lowercase().ends_with(".zip");
    let names = if zipped {
        crate::host::zip::names(bytes).unwrap_or_default()
    } else {
        vec![name.into_owned()]
    };
    match names.iter().find_map(|name| Some((name, named(name)?))) {
        Some((name, System::Atari2600))
            if name.to_lowercase().ends_with(".bin")
                && atari7800::image_of(path, bytes.to_vec())
                    .is_ok_and(|image| atari7800::is_7800(&image)) =>
        {
            System::Atari7800
        }
        Some((_, system)) => system,
        None => System::Nes,
    }
}

/// The facts of the game at `path`, its file's `bytes`.
pub fn facts_of(path: &Path, bytes: Vec<u8>) -> String {
    match system_of(path, &bytes) {
        System::Nes => nes::facts_of(path, nes::image_of(path, bytes)),
        System::Atari2600 => atari2600::facts_of(path, atari2600::image_of(path, bytes)),
        System::Atari7800 => atari7800::facts_of(path, atari7800::image_of(path, bytes)),
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
