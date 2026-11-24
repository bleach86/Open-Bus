//! The X16's I2C bus, bit-banged by the CPU on VIA 1's PA0 (SDA) and PA1
//! (SCL), and what's on it: the SMC, its system management controller
//! ($42), with the keyboard; and the RTC, an MCP7940N ($6F), a clock and 64
//! bytes of battery-backed RAM.
//!
//! The bus as I2C has it: a START, SDA falling while SCL's high; a STOP,
//! rising; a byte's 8 bits each read as SCL rises, then a 9th clock for
//! its ACK, SDA held low by whoever's receiving it. A transaction's first
//! byte is a device's address and a read or write; a device there ACKs
//! it. One being written takes a register, or command, then data; one
//! being read gives bytes for as long as it's ACKed, each bit on SDA as
//! SCL falls.
//!
//! The SMC as x16emu has it (its firmware's, 47.0.0): a command written
//! and a byte, done (power off, a reset, an NMI, the activity light, the
//! default read); a command written, then read, what it says, and after it
//! the default, the next key and the mouse's packet. Its keyboard's codes
//! are the IBM key numbers, a key down, or let go with bit 7 set; it holds
//! 16. Its mouse's are PS/2 packets, its buttons and how far it's moved
//! (3 bytes, or 4 with a wheel's, as the mouse's ID is 3); it holds 15
//! bytes, and gives a packet whole, or a 0 if there isn't one.
//!
//! The RTC as its data sheet has it, but its alarms, its square wave and
//! its trimming: its time, BCD, from its seconds' register at 0; 0x07 to
//! 0x1F, its control registers, holding what's written; 0x20 to 0x5F, its
//! RAM. Its address steps on with each byte, round within the clock's
//! registers or its RAM. It starts as a new one does, stopped: its
//! oscillator on once ST is set, the time counted each second from then,
//! by the host's clock, not the CPU's; or set, as an X16's is, its battery
//! keeping it (`set`).

use crate::host::clock::{self, DateTime};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const SMC: u8 = 0x42;
pub const RTC: u8 = 0x6F;

/// The keyboard's codes the SMC holds, at most.
pub const KEYS_HELD: usize = 16;
/// The mouse's bytes it holds, and more: a packet's let in only if there's
/// room for it and one more byte, as x16emu has it.
const MOUSE_HELD: usize = 16;
/// Its version, as it says.
const SMC_VERSION: [u8; 3] = [47, 0, 0];
/// Its read as it's set at reset: the next key, then the mouse's.
const KEY_AND_MOUSE: u8 = 0x43;

/// Where a transaction is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Phase {
    /// Waiting for a START; or not addressed, for a STOP.
    #[default]
    Idle,
    /// A byte coming from the CPU: the address, if `first`.
    Taking { first: bool },
    /// A byte going to it.
    Giving,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct I2c {
    /// SCL and SDA as the CPU's side has them: low, or let go.
    scl: bool,
    sda: bool,
    phase: Phase,
    /// The device addressed, and whether it's being read.
    device: u8,
    reading: bool,
    /// The byte coming or going, its clocks so far: 8 bits, then the ACK.
    byte: u8,
    clocks: u8,
    /// Whether the device addressed is holding SDA low.
    holding: bool,
    /// Whether a byte written is its first since the address.
    register_next: bool,
    pub smc: Smc,
    pub rtc: Rtc,
}

impl I2c {
    pub fn new() -> I2c {
        I2c {
            scl: true,
            sda: true,
            phase: Phase::Idle,
            device: 0,
            reading: false,
            byte: 0,
            clocks: 0,
            holding: false,
            register_next: false,
            smc: Smc::new(),
            rtc: Rtc::new(),
        }
    }

    /// SDA as it is: low if the CPU or a device holds it.
    pub fn sda(&self) -> bool {
        self.sda && !self.holding
    }

    /// The CPU's side of SCL and SDA as they are now, `scl` and `sda`: high
    /// if let go.
    pub fn lines(&mut self, scl: bool, sda: bool) {
        let was_sda = self.sda();
        let was_scl = self.scl;
        (self.scl, self.sda) = (scl, sda);
        if was_scl && scl {
            match (was_sda, self.sda()) {
                (true, false) => self.start(),
                (false, true) => self.stop(),
                _ => {}
            }
        } else if !was_scl && scl {
            self.rise();
        } else if was_scl && !scl {
            self.fall();
        }
    }

    fn start(&mut self) {
        self.phase = Phase::Taking { first: true };
        (self.byte, self.clocks, self.holding) = (0, 0, false);
    }

    fn stop(&mut self) {
        self.phase = Phase::Idle;
        self.holding = false;
    }

    /// SCL rising: a bit read, the CPU's, or its ACK of a byte given.
    fn rise(&mut self) {
        match self.phase {
            Phase::Idle => {}
            Phase::Taking { .. } => {
                if self.clocks < 8 {
                    self.byte = self.byte << 1 | self.sda() as u8;
                }
                self.clocks += 1;
            }
            Phase::Giving => {
                if self.clocks == 8 && self.sda() {
                    // NACKed: the last it wanted.
                    self.phase = Phase::Idle;
                }
                self.clocks += 1;
            }
        }
    }

    /// SCL falling: SDA as the device addressed has it next.
    fn fall(&mut self) {
        match self.phase {
            Phase::Idle => self.holding = false,
            Phase::Taking { first } => match self.clocks {
                // The byte's in: its ACK, if it's taken.
                8 => self.holding = self.took(first),
                9 => {
                    self.holding = false;
                    (self.byte, self.clocks) = (0, 0);
                    if first && self.reading {
                        self.give();
                    } else {
                        self.phase = Phase::Taking { first: false };
                    }
                }
                _ => {}
            },
            Phase::Giving => match self.clocks {
                1..=7 => self.holding = self.byte >> (7 - self.clocks) & 1 == 0,
                8 => self.holding = false,
                _ => self.give(),
            },
        }
    }

    /// A byte taken: the address, and whether a device is at it; or what
    /// the device addressed is written.
    fn took(&mut self, first: bool) -> bool {
        let byte = self.byte;
        if first {
            (self.device, self.reading) = (byte >> 1, byte & 1 != 0);
            self.register_next = true;
            let there = matches!(self.device, SMC | RTC);
            if !there {
                self.phase = Phase::Idle;
            }
            return there;
        }
        let register = std::mem::take(&mut self.register_next);
        match self.device {
            SMC => self.smc.written(byte, register),
            _ => self.rtc.written(byte, register),
        }
        true
    }

    /// The next byte given, its first bit on SDA.
    fn give(&mut self) {
        self.byte = match self.device {
            SMC => self.smc.read(),
            _ => self.rtc.read(),
        };
        self.phase = Phase::Giving;
        self.clocks = 0;
        self.holding = self.byte & 0x80 == 0;
    }
}

impl Default for I2c {
    fn default() -> I2c {
        I2c::new()
    }
}

/// What the SMC's asked to do, by a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Asked {
    PowerOff,
    Reset,
    Nmi,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Smc {
    /// The keyboard's codes waiting.
    pub keys: VecDeque<u8>,
    /// The command written, and its read; the read it goes back to.
    command: u8,
    default_read: u8,
    /// Whether the key's been given, of the default read's key and mouse.
    key_given: bool,
    /// The mouse's packets' bytes waiting; how many of the one being read
    /// have been given; its ID, 3 with a wheel, or 0.
    pub mouse: VecDeque<u8>,
    mouse_given: usize,
    mouse_id: u8,
    pub activity_light: bool,
    /// What it's been asked, not yet done.
    pub asked: Option<Asked>,
}

impl Smc {
    /// Whether it's to give the mouse's packets: its default read the
    /// mouse's, or the key's and the mouse's, as the KERNAL sets it when a
    /// program turns the mouse on (MOUSE 1), and back to the key's alone
    /// when it's turned off.
    pub fn mouse_on(&self) -> bool {
        matches!(self.default_read, 0x42 | KEY_AND_MOUSE)
    }

    /// Its default read set, as the KERNAL sets it: for the host's tests.
    #[cfg(test)]
    pub fn set_default_read(&mut self, read: u8) {
        self.default_read = read;
    }

    fn new() -> Smc {
        Smc {
            keys: VecDeque::new(),
            command: KEY_AND_MOUSE,
            default_read: KEY_AND_MOUSE,
            key_given: false,
            mouse: VecDeque::new(),
            mouse_given: 0,
            mouse_id: 3,
            activity_light: false,
            asked: None,
        }
    }

    /// A key's code from the keyboard, unless it's full.
    pub fn key(&mut self, code: u8) -> bool {
        let room = self.keys.len() < KEYS_HELD;
        if room {
            self.keys.push_back(code);
        }
        room
    }

    /// A packet's bytes, as the mouse's ID has them.
    fn packet_size(&self) -> usize {
        if self.mouse_id == 3 { 4 } else { 3 }
    }

    /// A packet from the mouse, unless it's full: its buttons (left,
    /// right, middle, bits 0 to 2) and how far it's moved, -256 to 255 each
    /// way, up for y.
    pub fn mouse(&mut self, buttons: u8, x: i16, y: i16) -> bool {
        let size = self.packet_size();
        let room = self.mouse.len() + size < MOUSE_HELD;
        if room {
            let signs = ((y >> 8 & 1) as u8) << 5 | ((x >> 8 & 1) as u8) << 4;
            self.mouse
                .extend([signs | 0x08 | buttons & 7, x as u8, y as u8, 0]);
            self.mouse.truncate(self.mouse.len() - (4 - size));
        }
        room
    }

    /// `byte` written to it: its command, or the command's byte.
    fn written(&mut self, byte: u8, command: bool) {
        if command {
            self.command = byte;
            return;
        }
        match (self.command, byte) {
            (0x01, 0x00) => self.asked = Some(Asked::PowerOff),
            (0x01, 0x01) | (0x02, 0x00) => self.asked = Some(Asked::Reset),
            (0x03, 0x00) => self.asked = Some(Asked::Nmi),
            (0x05, _) => self.activity_light = byte >= 0x80,
            (0x20, _) => {
                self.mouse_id = if matches!(byte, 3 | 4) { 3 } else { 0 };
                self.mouse.clear();
            }
            (0x40, _) => {
                self.default_read = byte;
                self.key_given = false;
            }
            _ => {}
        }
        self.command = self.default_read;
    }

    /// A byte read, as its command says; the default read's next.
    fn read(&mut self) -> u8 {
        let byte = match self.command {
            KEY_AND_MOUSE if !self.key_given => {
                self.key_given = true;
                self.keys.pop_front().unwrap_or(0)
            }
            // The mouse's packet, a byte at a time, once it's all there;
            // or a 0.
            0x21 | 0x42 | KEY_AND_MOUSE => {
                if self.mouse_given == 0 && self.mouse.len() < self.packet_size() {
                    self.key_given = false;
                    0
                } else {
                    self.mouse_given += 1;
                    if self.mouse_given == self.packet_size() {
                        (self.mouse_given, self.key_given) = (0, false);
                    }
                    self.mouse.pop_front().unwrap_or(0)
                }
            }
            0x41 => {
                self.key_given = false;
                self.keys.pop_front().unwrap_or(0)
            }
            0x07 => self.keys.pop_front().unwrap_or(0),
            // The power button wasn't held long to turn it on.
            0x09 => 0,
            0x22 => self.mouse_id,
            command @ 0x30..=0x32 => SMC_VERSION[(command - 0x30) as usize],
            _ => 0xFF,
        };
        self.command = self.default_read;
        byte
    }
}

/// Its time's registers, in its order: seconds, minutes, hours, the day of
/// the week (1 to 7), the day, the month, the year (of 2000 to 2099).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rtc {
    seconds: u8,
    minutes: u8,
    hours: u8,
    weekday: u8,
    day: u8,
    month: u8,
    year: u8,
    /// Its oscillator on (ST); its hours 12 to a half day, not 24; its
    /// battery on (VBATEN).
    running: bool,
    twelve: bool,
    battery: bool,
    /// 0x07 to 0x1F, as written.
    control: Vec<u8>,
    pub ram: Vec<u8>,
    /// Its RAM written, since it was last asked.
    #[serde(skip)]
    pub ram_written: bool,
    /// The register at its address.
    address: u8,
    /// When its second began, as `clock::seconds` has it: none till it's
    /// counted from (as a state's loaded, it's counted from then).
    #[serde(skip)]
    counted: Option<f64>,
}

fn bcd(n: u8) -> u8 {
    ((n / 10) << 4) | (n % 10)
}

fn unbcd(n: u8) -> u8 {
    (n >> 4) * 10 + (n & 0x0F)
}

impl Rtc {
    fn new() -> Rtc {
        let mut control = vec![0; 0x19];
        control[0] = 0x80; // CONTROL: its MFP pin high
        Rtc {
            seconds: 0,
            minutes: 0,
            hours: 0,
            weekday: 1,
            day: 1,
            month: 1,
            year: 0,
            running: false,
            twelve: false,
            battery: false,
            control,
            ram: vec![0; 0x40],
            ram_written: false,
            address: 0,
            counted: None,
        }
    }

    /// Set to `now`, running, 24 hours to a day, its battery on: as one
    /// in an X16 is, set once and kept by its battery since. Sunday's day 7
    /// of the week, as x16emu has it; the year's of 2000 to 2099.
    pub fn set(&mut self, now: DateTime) {
        self.seconds = now.second as u8;
        self.minutes = now.minute as u8;
        self.hours = now.hour as u8;
        self.weekday = if now.weekday == 0 {
            7
        } else {
            now.weekday as u8
        };
        self.day = now.day as u8;
        self.month = now.month as u8;
        self.year = (now.year - 2000).clamp(0, 99) as u8;
        (self.running, self.twelve, self.battery) = (true, false, true);
        // Its second begun as the host's did, to turn over as it does.
        self.counted = Some(clock::seconds() - clock::into_second());
    }

    /// The seconds gone by since it last counted, by the host's clock,
    /// counted, if it's running: as a real one's crystal keeps time,
    /// whatever the machine's speed. (Counted by the CPU's cycles, it gained
    /// 5 seconds in 10 minutes as its frames were kept in step with a 60 Hz
    /// display, VERA's being 59.52 a second; and it ran on in fast-forward,
    /// and stood still as the machine was paused.) Stopped, its time stands
    /// still, and its second begins again as it's started.
    fn catch_up(&mut self) {
        let now = clock::seconds();
        match self.counted {
            Some(mut from) if self.running => {
                while now - from >= 1.0 {
                    self.second();
                    from += 1.0;
                }
                self.counted = Some(from);
            }
            _ => self.counted = Some(now),
        }
    }

    /// `seconds` gone by, as if they had, to be counted as it's next read.
    #[cfg(test)]
    pub fn pass(&mut self, seconds: f64) {
        self.catch_up();
        self.counted = self.counted.map(|from| from - seconds);
    }

    /// Its address stepped on: round within its clock's registers, or its
    /// RAM.
    fn step(&mut self) {
        self.address = match self.address {
            0x1F => 0x00,
            0x5F => 0x20,
            0xFF => 0x60,
            n => n + 1,
        };
    }

    fn leap(&self) -> bool {
        self.year.is_multiple_of(4)
    }

    fn read(&mut self) -> u8 {
        if self.address <= 0x06 {
            self.catch_up();
        }
        let byte = match self.address {
            0x00 => bcd(self.seconds) | (self.running as u8) << 7,
            0x01 => bcd(self.minutes),
            0x02 if self.twelve => {
                let pm = self.hours >= 12;
                let hour = match self.hours % 12 {
                    0 => 12,
                    hour => hour,
                };
                0x40 | (pm as u8) << 5 | bcd(hour)
            }
            0x02 => bcd(self.hours),
            0x03 => self.weekday | (self.battery as u8) << 3 | (self.running as u8) << 5,
            0x04 => bcd(self.day),
            0x05 => bcd(self.month) | (self.leap() as u8) << 5,
            0x06 => bcd(self.year),
            n @ 0x07..=0x1F => self.control[(n - 7) as usize],
            n @ 0x20..=0x5F => self.ram[(n - 0x20) as usize],
            _ => 0xFF,
        };
        self.step();
        byte
    }

    fn written(&mut self, byte: u8, address: bool) {
        if address {
            self.address = byte;
            return;
        }
        if self.address <= 0x06 {
            self.catch_up();
        }
        match self.address {
            0x00 => {
                self.running = byte & 0x80 != 0;
                self.seconds = unbcd(byte & 0x7F);
                // Its seconds set: the second they're of begins now.
                self.counted = Some(clock::seconds());
            }
            0x01 => self.minutes = unbcd(byte & 0x7F),
            0x02 => {
                self.twelve = byte & 0x40 != 0;
                self.hours = if self.twelve {
                    unbcd(byte & 0x1F) % 12 + if byte & 0x20 != 0 { 12 } else { 0 }
                } else {
                    unbcd(byte & 0x3F)
                };
            }
            0x03 => {
                self.weekday = byte & 7;
                self.battery = byte & 0x08 != 0;
            }
            0x04 => self.day = unbcd(byte & 0x3F),
            0x05 => self.month = unbcd(byte & 0x1F),
            0x06 => self.year = unbcd(byte),
            n @ 0x07..=0x1F => self.control[(n - 7) as usize] = byte,
            n @ 0x20..=0x5F => {
                self.ram[(n - 0x20) as usize] = byte;
                self.ram_written = true;
            }
            _ => {}
        }
        self.step();
    }

    fn second(&mut self) {
        const DAYS: [u8; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        self.seconds += 1;
        if self.seconds < 60 {
            return;
        }
        self.seconds = 0;
        self.minutes += 1;
        if self.minutes < 60 {
            return;
        }
        self.minutes = 0;
        self.hours += 1;
        if self.hours < 24 {
            return;
        }
        self.hours = 0;
        self.weekday = self.weekday % 7 + 1;
        self.day += 1;
        let month = self.month.clamp(1, 12) as usize - 1;
        let days = DAYS[month] + (month == 1 && self.leap()) as u8;
        if self.day <= days {
            return;
        }
        self.day = 1;
        self.month += 1;
        if self.month <= 12 {
            return;
        }
        self.month = 1;
        self.year = (self.year + 1) % 100;
    }
}
