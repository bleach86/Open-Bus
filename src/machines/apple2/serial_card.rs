//! A Super Serial Card, in slot 2: its 6551 ACIA, and on its port a modem
//! (`host::modem`), Hayes's, that dials places on the internet, as BBSes
//! are reached now; for terminal programs (ProTERM, say) to go online
//! with. Its firmware is our own (roms/serial_card.s, built in here), with
//! the SSC's signature and its entries for BASIC and Pascal.
//!
//! Its registers, $C0n0 to $C0nF: $C0n8 to $C0nB, the 6551's; $C0n1 and
//! $C0n2, its switches, as MAME's SSC (BSD-3) has them read: SW1, its baud
//! rate in bits 7 to 4 and its mode in 1 and 0; SW2, its stop bits (7),
//! data bits (5), parity (3, 2), line feeds after returns (1) and
//! interrupts (2, 0 on). Here set for 9600 baud, communications, 8 bits,
//! no parity, interrupts on: its IRQ the 6551's.

use crate::chips::acia6551::Acia6551;
use crate::host::modem::Modem;
use serde::{Deserialize, Serialize};

/// The firmware, assembled (`make -C roms`) and committed.
pub const FIRMWARE: &[u8; 256] = include_bytes!("../../../roms/serial_card.bin");

/// Its switches, as they read: SW1, 9600 baud, communications mode; SW2,
/// a stop bit, 8 data bits, no parity, no line feeds added, interrupts on.
const SWITCHES_1: u8 = 0xE0;
const SWITCHES_2: u8 = 0x02;

#[derive(Serialize, Deserialize)]
pub struct SerialCard {
    pub acia: Acia6551,
    /// The modem on its port: the host's, not in a save state.
    #[serde(skip)]
    pub modem: Modem,
}

impl SerialCard {
    pub fn new(clock_hz: u32) -> Self {
        SerialCard {
            acia: Acia6551::new(clock_hz),
            modem: Modem::new(),
        }
    }

    /// The firmware, at $Cn00 to $CnFF.
    pub fn rom(&self, addr: u16) -> u8 {
        FIRMWARE[(addr & 0xFF) as usize]
    }

    /// A register, $C0n0 to $C0nF, read or written: the 6551's at 8 to
    /// $B; the switches, read, below them.
    pub fn io(&mut self, addr: u16, written: Option<u8>) -> Option<u8> {
        let reg = (addr & 0x0F) as u8;
        if reg & 8 != 0 {
            return match written {
                Some(value) => {
                    self.acia.write(reg & 3, value);
                    None
                }
                None => Some(self.acia.read(reg & 3)),
            };
        }
        if written.is_some() {
            return None;
        }
        let mut switches = 0xFF;
        if reg & 2 == 0 {
            switches &= SWITCHES_1;
        }
        if reg & 1 == 0 {
            switches &= SWITCHES_2;
        }
        Some(switches)
    }

    /// A cycle, the 6551's.
    pub fn tick(&mut self) {
        self.acia.tick();
    }

    /// Whether it's holding the IRQ line, its interrupts switched on.
    pub fn irq(&self) -> bool {
        self.acia.irq()
    }

    /// A frame gone by: what the port's sent, to the modem; what the modem
    /// has, to the port; and whether it's ready and has a carrier.
    pub fn frame(&mut self) {
        let sent = std::mem::take(&mut self.acia.outgoing);
        self.modem.rate = self.acia.rate();
        self.modem.send(&sent);
        self.modem.frame();
        // What's come, as the port has room for it: two frames' worth at
        // its rate, the rest left with the modem, for the machine's XOFF
        // to hold there, as a modem's buffer does.
        let room =
            (self.acia.rate() as usize / 10 / 60 * 2 + 4).saturating_sub(self.acia.incoming.len());
        self.acia.incoming.extend(self.modem.for_machine(room));
        self.acia.ready = true;
        self.acia.connected = self.modem.carrier();
    }
}
