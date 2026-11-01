//! The Apple's save states: F2 saves one, F4 loads it back; a file beside
//! the machine's first disk image (or folder, or playlist), or its ROM if
//! it has none, its name with .state after. A state has all of the machine
//! but its ROMs, the host's side, and the hard disk's volumes, which are
//! written to their images as they go: those are the machine's it's loaded
//! into, which must be the same Apple, with the same ROMs, a Disk II if it
//! had one, and the same volumes. The disks in the drives are in it, bit by
//! bit: loaded, they're as they were when it was saved, and saved to their
//! images when next written, as ever.

use super::{Apple2, NOTICE_FRAMES};
use crate::cpu::CPU;
use crate::machines::save_state;
use std::mem::take;
use std::path::{Path, PathBuf};

/// A save state to be saved, or loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wanted {
    Save,
    Load,
}

/// Where the state of a machine run with `first`, its first disk, or its
/// ROM, is kept: beside it, its name with .state after.
pub fn path(first: &Path) -> PathBuf {
    let name = first.file_name().map_or_else(|| "apple2".into(), |name| name.to_string_lossy().into_owned());
    first.with_file_name(format!("{name}.state"))
}

/// The machine, as a state's header names it: which Apple, its ROMs (a
/// checksum of them, the Disk II's too), and the hard disk's volumes.
fn machine(apple: &Apple2) -> String {
    let mut roms = vec![&apple.rom[..]];
    if let Some(iie) = &apple.iie {
        roms.extend([&iie.internal_rom[..], &iie.characters[..]]);
    }
    if let Some(card) = &apple.disk2 {
        roms.extend(card.roms());
    }
    let name = if apple.iie.is_some() { "Apple //e" } else { "Apple ][" };
    let mut machine = format!("{name}, ROMs {:08X}", checksum(&roms));
    if apple.disk2.is_some() {
        machine += ", a Disk II";
    }
    for volume in apple.hard_disk.iter().flat_map(|card| card.units.iter().flatten()) {
        let path = std::fs::canonicalize(&volume.path).unwrap_or_else(|_| volume.path.clone());
        machine += &format!(", the hard disk's {}", path.display());
    }
    machine
}

/// FNV-1a's 32 bits, of `parts` in turn.
fn checksum(parts: &[&[u8]]) -> u32 {
    parts.iter().flat_map(|part| part.iter()).fold(0x811C_9DC5, |hash, &byte| (hash ^ byte as u32).wrapping_mul(0x0100_0193))
}

/// The machine's state.
pub fn state(cpu: &CPU<Apple2>) -> Result<Vec<u8>, String> {
    save_state::save(cpu, &machine(&cpu.memory))
}

/// The machine put as it was in `state`, if it's one of this machine's,
/// what a state leaves out its own.
pub fn restore(cpu: &mut CPU<Apple2>, state: &[u8]) -> Result<(), String> {
    let mut loaded: CPU<Apple2> = save_state::load(state, &machine(&cpu.memory))?;
    loaded.memory.keep(&mut cpu.memory);
    *cpu = loaded;
    Ok(())
}

/// The state wanted, if one is, saved to `path` or loaded from it, and a
/// notice of it shown. Saved to a file beside it, then put in its place, so
/// a state is never half saved.
pub fn wanted(cpu: &mut CPU<Apple2>, path: &Path) {
    let notice = match cpu.memory.state_wanted.take() {
        None => return,
        Some(Wanted::Save) => {
            let saving = path.with_extension("state.saving");
            let saved = state(cpu).and_then(|state| {
                std::fs::write(&saving, state).and_then(|_| std::fs::rename(&saving, path)).map_err(|e| e.to_string())
            });
            match saved {
                Ok(()) => "State saved",
                Err(e) => {
                    eprintln!("{}: not saved: {e}", path.display());
                    "State not saved"
                }
            }
        }
        Some(Wanted::Load) if !path.exists() => "No state saved",
        Some(Wanted::Load) => match std::fs::read(path).map_err(|e| e.to_string()).and_then(|state| restore(cpu, &state)) {
            Ok(()) => "State loaded",
            Err(e) => {
                eprintln!("{}: not loaded: {e}", path.display());
                "State not loaded"
            }
        },
    };
    cpu.memory.notice = Some((notice.to_string(), NOTICE_FRAMES));
}

impl Apple2 {
    /// What a save state leaves out, taken from `running`, the machine
    /// it's loaded into: its ROMs, the host's side, the hard disk's
    /// volumes; and what's in the drives, said again.
    fn keep(&mut self, running: &mut Apple2) {
        self.rom = take(&mut running.rom);
        self.terminal = running.terminal.take();
        self.display = running.display.take();
        self.picture = take(&mut running.picture);
        self.sound = take(&mut running.sound);
        self.monitor = running.monitor;
        if let (Some(iie), Some(running)) = (&mut self.iie, &mut running.iie) {
            iie.internal_rom = take(&mut running.internal_rom);
            iie.characters = take(&mut running.characters);
        }
        if let (Some(card), Some(running)) = (&mut self.disk2, &mut running.disk2) {
            card.take_roms(running);
        }
        if let (Some(card), Some(running)) = (&mut self.hard_disk, &mut running.hard_disk) {
            card.units = take(&mut running.units);
        }
        if let (Some(card), Some(running)) = (&mut self.clock, &running.clock) {
            card.fixed = running.fixed;
        }
        self.show_disks();
    }
}
