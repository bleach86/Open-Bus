//! The Disk II: its controller card, in slot 6, and a drive.
//!
//! The card's boot ROM (P5, 341-0027) is at $C600; its 16 switches, at
//! $C0E0 to $C0EF, work the stepper's four magnets, the motor, which drive,
//! and Q6 and Q7, which set the sequencer reading, sensing write protect or
//! writing. The sequencer is a state machine in a PROM (P6, 341-0028),
//! stepped twice a CPU cycle: from its state, Q7, Q6, the data register's
//! top bit and the drive's read pulse, the PROM gives its next state and
//! what to do to the data register: clear it, shift in a 0 or a 1, shift in
//! write protect, load it, or nothing. That is how the bits on the disk
//! become the bytes the CPU reads.
//!
//! The drive turns the disk a bit every 8 sequencer steps, 4 microseconds,
//! a 1 (a flux change) a read pulse in its first. Its read amplifier, an
//! MC3470, passes them on a bit late; but more than three bits without
//! one, it has turned its gain up so far that it reads noise, pulses at
//! random, 3 bits in 10: copy protection counts on it, and so does a
//! half-track with no track. The stepper moves the head a half-track
//! at a time toward a magnet next to it. Switched off, the motor runs on
//! for a second, held by a timer on the card: DOS counts on it, to find
//! the disk still turning when it next reads. RESET clears the switches'
//! latch: the magnets off, the motor switched off (it runs on), drive 1,
//! Q6 and Q7 off. Writing isn't yet: the disk is write protected.

use super::CLOCK_HZ;
use super::disk_image::{Disk, TRACKS};

pub struct Disk2 {
    boot_rom: Vec<u8>,
    /// P6, rewired into its logical order.
    sequencer: [u8; 256],
    // The switches.
    phases: u8,
    pub motor: bool,
    /// Cycles the motor runs on for, switched off.
    motor_off_in: u32,
    drive2: bool,
    q6: bool,
    q7: bool,
    /// The last byte the CPU wrote to the card, for the sequencer to load.
    bus: u8,
    // The sequencer.
    state: u8,
    pub data: u8,
    // The drive: the disk in it, the head over half-track `half_track`, at
    // bit `bit`, `step` sequencer steps into it.
    pub disk: Option<Disk>,
    pub half_track: usize,
    bit: usize,
    step: u8,
    /// The amplifier's last 4 bits, the latest lowest; and its noise.
    window: u8,
    noise: u32,
}

/// How long the motor runs on, switched off: a second.
const MOTOR_RUNS_ON: u32 = CLOCK_HZ;

/// The bits in a turn of the disk, where there is no track: at 300 rpm,
/// a bit every 4 microseconds.
const REVOLUTION_BITS: usize = 50_000;

/// The farthest the head goes: track 34's half-track, and one more.
const LAST_HALF_TRACK: usize = 2 * (TRACKS - 1) + 1;

impl Disk2 {
    /// The card with its two PROMs, as dumped: P5 and P6, 256 bytes each.
    pub fn new(p5: Vec<u8>, p6: &[u8]) -> Result<Disk2, String> {
        if p5.len() != 256 || p6.len() != 256 {
            return Err(format!("P5 and P6 are 256 bytes, not {} and {}", p5.len(), p6.len()));
        }
        Ok(Disk2 {
            boot_rom: p5,
            sequencer: logical_order(p6),
            phases: 0,
            motor: false,
            motor_off_in: 0,
            drive2: false,
            q6: false,
            q7: false,
            bus: 0,
            state: 0,
            data: 0,
            disk: None,
            half_track: 0,
            bit: 0,
            step: 0,
            window: 0,
            noise: 0x2545_F491,
        })
    }

    /// The boot ROM, at $C600 to $C6FF.
    pub fn rom(&self, addr: u16) -> u8 {
        self.boot_rom[(addr & 0xFF) as usize]
    }

    /// A switch, $C0E0 to $C0EF, read or written (`written`, the byte):
    /// what a read gives, the data register at an even address.
    pub fn io(&mut self, addr: u16, written: Option<u8>) -> u8 {
        let switch = (addr & 0x0F) as u8;
        match switch {
            0x0..=0x7 => {
                let magnet = 1 << (switch >> 1);
                if switch & 1 != 0 {
                    self.phases |= magnet;
                } else {
                    self.phases &= !magnet;
                }
                self.move_head();
            }
            0x8 => {
                if self.motor {
                    self.motor_off_in = MOTOR_RUNS_ON;
                }
                self.motor = false;
            }
            0x9 => {
                self.motor = true;
                self.motor_off_in = 0;
            }
            0xA => self.drive2 = false,
            0xB => self.drive2 = true,
            0xC => self.q6 = false,
            0xD => self.q6 = true,
            0xE => self.q7 = false,
            _ => self.q7 = true,
        }
        if let Some(value) = written {
            self.bus = value;
        }
        if switch & 1 == 0 { self.data } else { 0 }
    }

    /// RESET: every switch off, as the latch holding them is cleared.
    pub fn reset(&mut self) {
        for switch in (0..16).step_by(2) {
            self.io(0xC0E0 + switch, None);
        }
    }

    /// The head toward a magnet on next to it, a half-track: the magnet
    /// under it is its half-track's, of the four in turn.
    fn move_head(&mut self) {
        let under = self.half_track % 4;
        let on = |magnet: usize| self.phases & (1 << magnet) != 0;
        let (inward, outward) = (on((under + 1) % 4), on((under + 3) % 4));
        let to = if inward && !outward && self.half_track < LAST_HALF_TRACK {
            self.half_track + 1
        } else if outward && !inward && self.half_track > 0 {
            self.half_track - 1
        } else {
            return;
        };
        // The same place round the disk, on the next track.
        let (from_len, to_len) = (self.track_len(self.half_track), self.track_len(to));
        self.bit = self.bit * to_len / from_len;
        self.half_track = to;
    }

    /// The disk in the drive switched to: drive 1's; drive 2 has none.
    fn disk(&self) -> Option<&Disk> {
        self.disk.as_ref().filter(|_| !self.drive2)
    }

    /// The bits under the head: the track's at its half-track, if there is
    /// one there.
    fn track(&self) -> Option<&Vec<u8>> {
        self.disk()?.track(self.half_track)
    }

    /// The bits in a turn at `half_track`: its track's, or a turn's of none.
    fn track_len(&self, half_track: usize) -> usize {
        self.disk.as_ref().and_then(|disk| disk.track(half_track)).map_or(REVOLUTION_BITS, Vec::len)
    }

    /// Whether the disk is turning: the motor on, or running on.
    pub fn spinning(&self) -> bool {
        self.motor || self.motor_off_in > 0
    }

    /// A CPU cycle: two sequencer steps.
    pub fn tick(&mut self) {
        self.motor_off_in = self.motor_off_in.saturating_sub(1);
        self.sequencer_step();
        self.sequencer_step();
    }

    fn sequencer_step(&mut self) {
        // The disk turning under the head, while the motor is on: the
        // amplifier's read pulse in each bit's first step; no track, no
        // flux changes.
        let mut pulse = false;
        if self.spinning() && self.disk().is_some() {
            if self.step == 0 {
                let bit = self.track().map_or(0, |track| track[self.bit % track.len()]);
                pulse = self.amplify(bit);
            }
            self.step += 1;
            if self.step == 8 {
                self.step = 0;
                self.bit = (self.bit + 1) % self.track_len(self.half_track);
            }
        }

        // The pulse is active low.
        let address = self.state
            | (self.q7 as u8) << 3
            | (self.q6 as u8) << 2
            | (self.data >> 7) << 1
            | (!pulse) as u8;
        let next = self.sequencer[address as usize];
        self.state = next & 0xF0;
        match next & 0x0F {
            0x0..=0x7 => self.data = 0,                                 // clear
            0x9 => self.data <<= 1,                                     // shift in a 0
            0xA | 0xE => self.data = self.data >> 1 | 0x80,             // shift in write protect, on
            0xB | 0xF => self.data = self.bus,                          // load
            0xD => self.data = self.data << 1 | 1,                      // shift in a 1
            _ => {}                                                     // nothing
        }
    }

    /// The read amplifier given a bit from the head: the bit before it,
    /// while there has been a 1 in the last 4; then noise.
    fn amplify(&mut self, bit: u8) -> bool {
        self.window = (self.window << 1 | bit) & 0x0F;
        if self.window != 0 { self.window & 0x02 != 0 } else { self.noise() }
    }

    /// The amplifier's noise: a 1, 3 times in 10; by xorshift.
    fn noise(&mut self) -> bool {
        let mut x = self.noise;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.noise = x;
        x % 10 < 3
    }
}

/// P6 as dumped, in its logical order: the board wires the PROM's address
/// lines to the sequencer's inputs out of order. Its A7, A6, A5 and A0 are
/// the state's bits 3, 2, 0 and 1; A4 is the read pulse; A3, A2 and A1 are
/// Q7, Q6 and the data register's top bit. Its output is as it is: the next
/// state, then the command.
fn logical_order(p6: &[u8]) -> [u8; 256] {
    let mut table = [0; 256];
    for (physical, &entry) in p6.iter().enumerate() {
        let bit = |n: usize| (physical >> n) & 1;
        let logical = bit(7) << 7 | bit(6) << 6 | bit(0) << 5 | bit(5) << 4 | bit(4) | (physical & 0x0E);
        table[logical] = entry;
    }
    table
}
