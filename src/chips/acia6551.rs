//! The MOS 6551 ACIA: a serial port, at 4 registers, clocked with the CPU.
//! Its other end is the host's: what's to come in, `incoming`, and what's
//! gone out, `outgoing`, a byte at a time as fast as its baud rate lets
//! them, as it's set; `connected`, the carrier and the other end ready.
//!
//! 0, its data: a read takes the byte received (receive data register
//! full no longer, and its errors cleared); a write's to be sent, held
//! until the transmitter's free. 1, its status: bit 7 an interrupt, which
//! reading it clears; 6 DSR and 5 DCD, low while the other end's there; 4
//! the transmit data register empty; 3 the receive data register full; 2
//! an overrun, a byte come in before the last was read, and lost. Writing
//! it is a programmed reset: the command's low 5 bits and the overrun
//! cleared. 2, the command: bit 0 DTR, the receiver and the interrupts on;
//! 1, the receiver's interrupt off; 3 and 2, %01 the transmitter's
//! interrupt on. 3, the control: bits 3 to 0 the baud rate, 6 and 5 the
//! word's length, 7 the stop bits.
//!
//! An interrupt is each byte received, and each time the transmit data
//! register empties, as they're on. Parity's sent and checked as set but
//! the other end never errs; the echo mode, and a break, aren't here.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const IRQ: u8 = 0x80;
const NO_DSR: u8 = 0x40;
const NO_DCD: u8 = 0x20;
const TDRE: u8 = 0x10;
const RDRF: u8 = 0x08;
const OVERRUN: u8 = 0x04;

/// The baud rates of the control register's low 4 bits, from its 1.8432
/// MHz crystal: 0, the external clock, none here, as 115,200.
const BAUDS: [u32; 16] = [
    115_200, 50, 75, 110, 135, 150, 300, 600, 1_200, 1_800, 2_400, 3_600, 4_800, 7_200, 9_600,
    19_200,
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acia6551 {
    command: u8,
    control: u8,
    status: u8,
    received: u8,
    /// The byte held to send, and the one being sent, with its cycles
    /// still to go.
    holding: Option<u8>,
    sending: Option<(u8, u32)>,
    /// Cycles since the last byte came in.
    since_received: u32,
    /// The CPU's clock, for the time a byte takes.
    clock_hz: u32,
    #[serde(skip)]
    pub incoming: VecDeque<u8>,
    #[serde(skip)]
    pub outgoing: Vec<u8>,
    #[serde(skip)]
    pub connected: bool,
}

impl Acia6551 {
    /// One as at power-on, on a CPU's clock of `clock_hz`.
    pub fn new(clock_hz: u32) -> Acia6551 {
        Acia6551 {
            status: TDRE,
            clock_hz,
            ..Acia6551::default()
        }
    }

    /// The cycles a byte takes at the rate set: its start bit, its word, its
    /// parity bit if it has one, its stop bits.
    fn byte_cycles(&self) -> u32 {
        let word = 8 - (self.control >> 5 & 3) as u32;
        let parity = (self.command & 0x20 != 0) as u32;
        let stop = 1 + (self.control >> 7) as u32;
        let bits = 1 + word + parity + stop;
        (self.clock_hz as u64 * bits as u64 / BAUDS[(self.control & 0x0F) as usize] as u64) as u32
    }

    fn on(&self) -> bool {
        self.command & 0x01 != 0
    }

    fn receive_interrupts(&self) -> bool {
        self.on() && self.command & 0x02 == 0
    }

    fn transmit_interrupts(&self) -> bool {
        self.on() && self.command & 0x0C == 0x04
    }

    /// Whether its IRQ line's held.
    pub fn irq(&self) -> bool {
        self.status & IRQ != 0
    }

    /// A read of register `reg`, 0 to 3.
    pub fn read(&mut self, reg: u8) -> u8 {
        match reg & 3 {
            0 => {
                self.status &= !(RDRF | OVERRUN);
                self.received
            }
            1 => {
                let away = if self.connected { 0 } else { NO_DSR | NO_DCD };
                let status = self.status | away;
                self.status &= !IRQ;
                status
            }
            2 => self.command,
            _ => self.control,
        }
    }

    /// A write of register `reg`, 0 to 3.
    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 3 {
            0 => {
                self.holding = Some(value);
                self.status &= !TDRE;
                self.next_to_send();
            }
            1 => {
                self.command &= 0xE0;
                self.status &= !OVERRUN;
            }
            2 => self.command = value,
            _ => self.control = value,
        }
    }

    /// The byte held, to the transmitter, if it's free: the transmit data
    /// register empty, an interrupt.
    fn next_to_send(&mut self) {
        if self.sending.is_some() {
            return;
        }
        if let Some(byte) = self.holding.take() {
            self.sending = Some((byte, self.byte_cycles()));
            self.status |= TDRE;
            if self.transmit_interrupts() {
                self.status |= IRQ;
            }
        }
    }

    /// A cycle: the byte being sent, a cycle nearer gone; the next to come
    /// in, if one's due.
    pub fn tick(&mut self) {
        if let Some((byte, left)) = &mut self.sending {
            *left = left.saturating_sub(1);
            if *left == 0 {
                self.outgoing.push(*byte);
                self.sending = None;
                self.next_to_send();
            }
        }
        self.since_received = self.since_received.saturating_add(1);
        if !self.on() || self.since_received < self.byte_cycles() {
            return;
        }
        let Some(byte) = self.incoming.pop_front() else {
            return;
        };
        self.since_received = 0;
        if self.status & RDRF != 0 {
            self.status |= OVERRUN;
            return;
        }
        self.received = byte;
        self.status |= RDRF;
        if self.receive_interrupts() {
            self.status |= IRQ;
        }
    }
}
