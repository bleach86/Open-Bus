//! Save states: a machine as it is, between frames, its CPU and all it's
//! wired to, kept to be loaded back and gone on with as it was. A state is
//! a header, then the CPU, the machine within it, in postcard's encoding
//! of their serde. The header says what it is: a state, the version of
//! what's saved, and the machine it's of, so it's loaded only into the
//! same machine, whose own (its ROMs, the host's side) the state can leave
//! out.

use crate::cpu::CPU;
use crate::host::machine::Machine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAGIC: &str = "cpu_emu save state";
/// What's saved: one more each time it changes, so an older state isn't
/// loaded as this one.
const VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct Header {
    magic: String,
    version: u32,
    machine: String,
}

/// Where the state of a machine run with `first`, its first disk or its
/// game, or its ROM, is kept: beside it, its name with .state after.
pub fn path(first: &Path) -> PathBuf {
    let name = first.file_name().map_or_else(
        || "machine".into(),
        |name| name.to_string_lossy().into_owned(),
    );
    first.with_file_name(format!("{name}.state"))
}

/// The state of `cpu` and its machine.
pub fn state<M: Machine>(cpu: &CPU<M>) -> Result<Vec<u8>, String> {
    let header = Header {
        magic: MAGIC.to_string(),
        version: VERSION,
        machine: cpu.memory.identity(),
    };
    let state = postcard::to_stdvec(&header).map_err(|e| e.to_string())?;
    postcard::to_extend(cpu, state).map_err(|e| e.to_string())
}

/// The machine put as it was in `state`, if it's a state, of this version,
/// of this machine; what a state leaves out its own.
pub fn restore<M: Machine>(cpu: &mut CPU<M>, state: &[u8]) -> Result<(), String> {
    let not_a_state = || "not a save state".to_string();
    let (header, rest) = postcard::take_from_bytes::<Header>(state).map_err(|_| not_a_state())?;
    if header.magic != MAGIC {
        return Err(not_a_state());
    }
    if header.version != VERSION {
        return Err(format!(
            "a state of version {}, not {VERSION}",
            header.version
        ));
    }
    if header.machine != cpu.memory.identity() {
        return Err(format!("a state of another machine: {}", header.machine));
    }
    let mut loaded: CPU<M> = postcard::from_bytes(rest).map_err(|e| e.to_string())?;
    loaded.memory.keep(&mut cpu.memory);
    *cpu = loaded;
    Ok(())
}

/// An array of bytes, in a state: serde has none over 32 of its own.
pub mod bytes {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer, const N: usize>(bytes: &[u8; N], serializer: S) -> Result<S::Ok, S::Error> {
        bytes[..].serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error> {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let len = bytes.len();
        bytes.try_into().map_err(|_| D::Error::custom(format!("{len} bytes, not {N}")))
    }
}
