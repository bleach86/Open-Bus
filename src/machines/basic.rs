//! This machine: the flat 64K, a ROM at its top if it has one, and the
//! terminal at 0xFFF8 and 0xFFF9; 0xFFF7, written, turns it off; it runs
//! Microsoft BASIC.

use crate::cpu::CPU;
use crate::cpu::bus::Bus;
use crate::host::terminal::Terminal;
use std::fmt;
use std::io::Write;
use std::time::Instant;

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SystemRegister {
    PowerOnMs = 0xFFF0,
    PowerOnMsHigh = 0xFFF1,
    PowerOnMsHigher = 0xFFF2,
    PowerOnMsHighest = 0xFFF3,
    UptimeLatch = 0xFFF4,
    PowerOff = 0xFFF7,
    TerminalOutput = 0xFFF8,
    TerminalInput = 0xFFF9,
}

impl TryFrom<u16> for SystemRegister {
    type Error = ();

    fn try_from(addr: u16) -> Result<Self, Self::Error> {
        match addr {
            0xFFF0 => Ok(Self::PowerOnMs),
            0xFFF1 => Ok(Self::PowerOnMsHigh),
            0xFFF2 => Ok(Self::PowerOnMsHigher),
            0xFFF3 => Ok(Self::PowerOnMsHighest),
            0xFFF4 => Ok(Self::UptimeLatch),
            0xFFF7 => Ok(Self::PowerOff),
            0xFFF8 => Ok(Self::TerminalOutput),
            0xFFF9 => Ok(Self::TerminalInput),
            _ => Err(()),
        }
    }
}

impl SystemRegister {
    fn timer_shift(self) -> Option<u32> {
        match self {
            Self::PowerOnMs => Some(0),
            Self::PowerOnMsHigh => Some(8),
            Self::PowerOnMsHigher => Some(16),
            Self::PowerOnMsHighest => Some(24),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Memory {
    // 64KB of memory
    pub data: [u8; 0x10000],
    // Where ROM starts, if there is any: from there up, writes don't take,
    // but for the terminal at 0xFFF8 and 0xFFF9.
    pub rom_start: Option<u16>,
    // Every read and write, in order, while there is a log: the address, the
    // value and whether it was a write. A logic analyzer on the bus.
    pub bus_log: Option<Vec<(u16, u8, bool)>>,
    // The terminal on 0xFFF8 and 0xFFF9, if one is attached: without, 0xFFF9
    // is RAM like the rest.
    pub terminal: Option<Terminal>,
    // When the machine was powered on, to compute uptime, if the uptime
    // registers are attached: without, 0xFFF0 to 0xFFF4 are RAM like the
    // rest, as the CPU's tests need them.
    pub power_on: Option<Instant>,
    // The uptime snapshot, in milliseconds, when the latch was last set.
    pub uptime_snapshot: u32,
    // Whether the uptime snapshot is latched: if so, reads of the uptime
    // are from the snapshot, not the current uptime.
    pub uptime_latch: bool,
}

impl Default for Memory {
    fn default() -> Self {
        Memory {
            data: [0; 0x10000],
            rom_start: None,
            bus_log: None,
            terminal: None,
            power_on: None,
            uptime_snapshot: 0,
            uptime_latch: false,
        }
    }
}

impl Memory {
    fn get_power_on_ms(&self) -> u32 {
        // If the uptime snapshot is latched, return it instead of the current uptime.
        if self.uptime_latch {
            return self.uptime_snapshot;
        }

        let Some(power_on) = self.power_on else {
            return 0;
        };
        let duration = power_on.elapsed();
        let ms = duration.as_millis();
        // wrap around to 32 bits, as the 6502 would.
        (ms % (u32::MAX as u128 + 1)) as u32
    }
}

impl Bus for Memory {
    fn read(&mut self, addr: u16) -> u8 {
        // Ctrl-C typed: the key waiting, whatever else is, as BASIC looks
        // for it there alone, between statements.
        if addr == 0xFFF9
            && let Some(terminal) = self.terminal.as_ref()
            && terminal.take_break()
        {
            self.data[0xFFF9] = 0x03;
        }

        // The terminal's key: the next typed, once the last has been taken
        // (written back 0).
        if addr == 0xFFF9
            && self.data[0xFFF9] == 0
            && let Some(key) = self.terminal.as_ref().and_then(|terminal| terminal.key())
        {
            self.data[0xFFF9] = key;
        }

        // The uptime registers: the uptime in milliseconds, low to high, at
        // 0xFFF0 to 0xFFF3, the uptime snapshot if the
        // latch is set. The uptime snapshot is latched by writing a non-zero
        // value to 0xFFF4, and unlatched by writing a zero to 0xFFF4.
        let shift = self
            .power_on
            .and(SystemRegister::try_from(addr).ok())
            .and_then(SystemRegister::timer_shift);
        if let Some(shift) = shift {
            let ms = self.get_power_on_ms();
            self.data[addr as usize] = (ms >> shift) as u8;
        }

        let value = self.data[addr as usize];
        if let Some(log) = &mut self.bus_log {
            log.push((addr, value, false));
        }
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        if let Some(log) = &mut self.bus_log {
            log.push((addr, value, true));
        }
        // Written, with a terminal attached: the machine turned off, as
        // BASIC's line editor asks, Q or QUIT typed at its prompt.
        if addr == SystemRegister::PowerOff as u16 && self.terminal.is_some() {
            crate::host::terminal::power_off();
        }
        if self.rom_start.is_some_and(|start| addr >= start)
            && addr != 0xFFF8
            && addr != 0xFFF9
            && !(addr == 0xFFF4 && self.power_on.is_some())
        {
            return; // ROM
        }
        self.data[addr as usize] = value;

        match SystemRegister::try_from(addr) {
            Ok(SystemRegister::TerminalOutput) => {
                // Special behavior: if writing to address 0xFFF8, print the character
                print!("{}", value as char);
                std::io::stdout().flush().unwrap();
            }
            Ok(SystemRegister::UptimeLatch) if self.power_on.is_some() => {
                // Special behavior: if writing to address 0xFFF4, latch or unlatch the uptime snapshot.
                // If the value is 0, unlatch; otherwise, latch and take a snapshot.
                if value == 0 {
                    self.uptime_latch = false;
                } else {
                    self.uptime_snapshot = self.get_power_on_ms();
                    self.uptime_latch = true;
                }
            }
            _ => {}
        }
    }
}

impl fmt::UpperHex for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.data.iter().enumerate() {
            if i % 16 == 0 {
                write!(f, "\n{:04X}: ", i)?;
            }
            write!(f, "{:02X} ", byte)?;
        }
        Ok(())
    }
}

/// Its speed: a megahertz, as a 6502 computer of BASIC's day ran at. Its
/// output, a program printing in a loop, comes as fast as such a machine's
/// did, which a terminal on a 9600 baud line can follow.
pub const CLOCK_HZ: u32 = 1_000_000;

/// This machine run, with BASIC: its ROM the top 16K, vectors and all; RAM
/// below it; the terminal attached; at a megahertz.
pub fn run(rom_opt: Option<&str>, with_terminal: bool) {
    let mut cpu = CPU::new();
    cpu.clock_hz = CLOCK_HZ;

    let rom: Vec<u8> = if let Some(rom_path) = rom_opt {
        std::fs::read(rom_path).expect("rom file not found")
    } else {
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/roms/basic.bin"))
            .expect("roms/basic.bin: run tools/fetch-msbasic")
    };

    let rom_start = cpu
        .memory
        .data
        .len()
        .checked_sub(rom.len())
        .expect("ROM is larger than available memory");
    cpu.memory.data[rom_start..].copy_from_slice(&rom);
    cpu.memory.rom_start = Some(rom_start as u16);

    if with_terminal {
        cpu.memory.terminal = Some(Terminal::new());
        // BASIC ends its lines with CR LF itself.
        crate::host::terminal::raw_output();
    }
    cpu.memory.power_on = Some(Instant::now());

    cpu.reset();
    cpu.execute_forever();
}
