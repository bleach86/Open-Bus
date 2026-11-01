//! Save states: a machine as it is, between instructions, its CPU and all
//! it's wired to, kept to be loaded back and gone on with as it was. A
//! state is a header, then the CPU, the machine within it, in postcard's
//! encoding of their serde. The header says what it is: a state, the
//! version of what's saved, and the machine it's of, so it's loaded only
//! into the same machine, whose own (its ROMs, the host's side) the state
//! can leave out.

use crate::cpu::bus::Bus;
use crate::cpu::CPU;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const MAGIC: &str = "cpu_emu save state";
/// What's saved: one more each time it changes, so an older state isn't
/// loaded as this one.
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Header {
    magic: String,
    version: u32,
    machine: String,
}

/// The state of `cpu` and its machine, `machine` (its make, its ROMs and
/// what's in it, as it describes itself).
pub fn save<B: Bus + Serialize>(cpu: &CPU<B>, machine: &str) -> Result<Vec<u8>, String> {
    let header = Header { magic: MAGIC.to_string(), version: VERSION, machine: machine.to_string() };
    let state = postcard::to_stdvec(&header).map_err(|e| e.to_string())?;
    postcard::to_extend(cpu, state).map_err(|e| e.to_string())
}

/// The CPU and its machine of `state`, if it's a state, of this version,
/// of `machine`.
pub fn load<B: Bus + DeserializeOwned>(state: &[u8], machine: &str) -> Result<CPU<B>, String> {
    let not_a_state = || "not a save state".to_string();
    let (header, rest) = postcard::take_from_bytes::<Header>(state).map_err(|_| not_a_state())?;
    if header.magic != MAGIC {
        return Err(not_a_state());
    }
    if header.version != VERSION {
        return Err(format!("a state of version {}, not {VERSION}", header.version));
    }
    if header.machine != machine {
        return Err(format!("a state of another machine: {}", header.machine));
    }
    postcard::from_bytes(rest).map_err(|e| e.to_string())
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
