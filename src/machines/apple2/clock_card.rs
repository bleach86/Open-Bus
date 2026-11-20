//! A clock card, in slot 1: as a Thunderclock, as ProDOS knows one, so
//! ProDOS finds it and has the date and time from it, the host's. Its
//! firmware is our own (roms/clock.s, built in here): asked for the time,
//! it has the card take it, then reads its text into the input buffer a
//! character at a time: "MO,DW,DT,HR,MN,SC" and a return, in ASCII with
//! bit 7 set (the month, the day of the week from Sunday's 0, the date,
//! the hour of 24, the minute, the second). ProDOS works the year out
//! from the day of the week, as a Thunderclock hasn't one.

use crate::host::clock::{self, DateTime};
use serde::{Deserialize, Serialize};

/// The firmware, assembled (`make -C roms`) and committed.
pub const FIRMWARE: &[u8; 256] = include_bytes!("../../../roms/clock.bin");

#[derive(Serialize, Deserialize)]
pub struct ClockCard {
    /// The time it gives: the host's, now; or, for a test, this (the
    /// test's own, not in a save state).
    #[serde(skip)]
    pub fixed: Option<DateTime>,
    /// The time taken, as its text, and the place read to in it.
    text: Vec<u8>,
    at: usize,
}

impl ClockCard {
    pub fn new() -> Self {
        ClockCard {
            fixed: None,
            text: Vec::new(),
            at: 0,
        }
    }

    /// The firmware, at $Cn00 to $CnFF.
    pub fn rom(&self, addr: u16) -> u8 {
        FIRMWARE[(addr & 0xFF) as usize]
    }

    /// A register, $C0n0 to $C0nF, read or written: $C0n0 written, the
    /// time taken; $C0n1 read, its text's next character, the return its
    /// last, over again; the rest, nothing.
    pub fn io(&mut self, addr: u16, written: Option<u8>) -> Option<u8> {
        match (addr & 0x0F, written) {
            (0, Some(_)) => {
                let now = self.fixed.unwrap_or_else(clock::now);
                let fields = [
                    now.month,
                    now.weekday,
                    now.day,
                    now.hour,
                    now.minute,
                    now.second,
                ];
                let text = fields.map(|field| format!("{field:02}")).join(",") + "\r";
                self.text = text.bytes().map(|byte| byte | 0x80).collect();
                self.at = 0;
                None
            }
            (1, None) => {
                let byte = self
                    .text
                    .get(self.at)
                    .or(self.text.last())
                    .copied()
                    .unwrap_or(0x8D);
                self.at = (self.at + 1).min(self.text.len().saturating_sub(1));
                Some(byte)
            }
            _ => None,
        }
    }
}

impl Default for ClockCard {
    fn default() -> Self {
        Self::new()
    }
}
