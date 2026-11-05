//! The MOS 6522 VIA: two 8-bit I/O ports, two 16-bit timers and its
//! interrupts, at 16 registers, clocked with the CPU.
//!
//! A port's pins are its output register's bits where its data direction
//! is a 1, and what's driving them from outside where it's a 0 (pulled
//! up, with nothing: 1s). Port A reads its pins; port B, its output
//! register where it's an output.
//!
//! Timer 1 is loaded by writing its high byte: the cycle after, its
//! counter holds the latch, then it counts down a cycle at a time; past 0,
//! to $FFFF, it sets its interrupt flag (N + 1.5 cycles after the write,
//! visible the cycle after that) and, the cycle after, takes the latch
//! again: so free running, a period of N + 2. One-shot, it reloads as
//! well, but interrupts the once, until it's written again. Timer 2 is the
//! same, its latch only its low byte, but after its one interrupt counts
//! on down from $FFFF. Each sets its flag in the IFR; the IRQ line is low
//! while any flag set is enabled in the IER. Reading a timer's low counter
//! byte clears its flag, as does loading it (and writing T1's high latch).
//!
//! The handshake lines (CA1, CA2, CB1, CB2), the shift register's
//! shifting and PB7 from timer 1 are not here: the registers hold what's
//! written to them, and that's all. Timer 2 set to count PB6's pulses
//! holds, none coming.

use serde::{Deserialize, Serialize};

/// The IFR's and IER's bits for the timers, and the IFR's for any.
const T1: u8 = 0x40;
const T2: u8 = 0x20;
const ANY: u8 = 0x80;

/// Its registers, by number.
const ORB: u8 = 0x0;
const ORA: u8 = 0x1;
const DDRB: u8 = 0x2;
const DDRA: u8 = 0x3;
const T1C_L: u8 = 0x4;
const T1C_H: u8 = 0x5;
const T1L_L: u8 = 0x6;
const T1L_H: u8 = 0x7;
const T2C_L: u8 = 0x8;
const T2C_H: u8 = 0x9;
const SR: u8 = 0xA;
const ACR: u8 = 0xB;
const PCR: u8 = 0xC;
const IFR: u8 = 0xD;
// $E, the IER: the rest.
const ORA_NO_HANDSHAKE: u8 = 0xF;

#[derive(Default, Serialize, Deserialize)]
pub struct Via6522 {
    // The ports: their output registers, data directions (1, an output),
    // and what drives their pins from outside.
    pub ora: u8,
    pub orb: u8,
    pub ddra: u8,
    pub ddrb: u8,
    pub pa_in: u8,
    pub pb_in: u8,
    // Timer 1: its counter and latch; whether it takes the latch next
    // cycle; whether it's to interrupt at its next time out.
    t1_counter: u16,
    t1_latch: u16,
    t1_load: bool,
    t1_armed: bool,
    // Timer 2: the same, its latch a byte.
    t2_counter: u16,
    t2_latch: u8,
    t2_load: bool,
    t2_armed: bool,
    sr: u8,
    acr: u8,
    pcr: u8,
    ifr: u8,
    ier: u8,
}

impl Via6522 {
    /// At power-on: its ports inputs, nothing pulled from outside; its
    /// timers' counters and latches whatever they came up as on the chip,
    /// not 0, so running: $FFFF here, the same each time.
    pub fn new() -> Self {
        Via6522 {
            pa_in: 0xFF,
            pb_in: 0xFF,
            t1_counter: 0xFFFF,
            t1_latch: 0xFFFF,
            t2_counter: 0xFFFF,
            t2_latch: 0xFF,
            ..Default::default()
        }
    }

    /// RESET: every register cleared but the timers and the shift register.
    pub fn reset(&mut self) {
        (self.ora, self.orb, self.ddra, self.ddrb) = (0, 0, 0, 0);
        (self.acr, self.pcr, self.ifr, self.ier) = (0, 0, 0, 0);
    }

    /// Port A's pins: outputs as written, inputs as driven.
    pub fn port_a(&self) -> u8 {
        (self.ora & self.ddra) | (self.pa_in & !self.ddra)
    }

    /// Port B's pins.
    pub fn port_b(&self) -> u8 {
        (self.orb & self.ddrb) | (self.pb_in & !self.ddrb)
    }

    /// Whether it's holding its IRQ line: a flag set that's enabled.
    pub fn irq(&self) -> bool {
        self.ifr & self.ier & 0x7F != 0
    }

    /// Register `reg` (0 to 15) read.
    pub fn read(&mut self, reg: u8) -> u8 {
        match reg & 0x0F {
            ORB => self.port_b(),
            ORA | ORA_NO_HANDSHAKE => self.port_a(),
            DDRB => self.ddrb,
            DDRA => self.ddra,
            T1C_L => {
                let value = self.t1_counter as u8;

                self.ifr &= !T1;
                value
            }
            T1C_H => (self.t1_counter >> 8) as u8,
            T1L_L => self.t1_latch as u8,
            T1L_H => (self.t1_latch >> 8) as u8,
            T2C_L => {
                self.ifr &= !T2;
                self.t2_counter as u8
            }
            T2C_H => (self.t2_counter >> 8) as u8,
            SR => self.sr,
            ACR => self.acr,
            PCR => self.pcr,
            IFR => self.ifr | if self.irq() { ANY } else { 0 },
            _ => self.ier | 0x80, // the IER
        }
    }

    /// Register `reg` (0 to 15) written.
    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            ORB => self.orb = value,
            ORA | ORA_NO_HANDSHAKE => self.ora = value,
            DDRB => self.ddrb = value,
            DDRA => self.ddra = value,
            T1C_L | T1L_L => self.t1_latch = (self.t1_latch & 0xFF00) | value as u16,
            T1C_H => {
                self.t1_latch = (self.t1_latch & 0x00FF) | (value as u16) << 8;
                self.t1_load = true;
                self.t1_armed = true;
                self.ifr &= !T1;
            }
            T1L_H => {
                self.t1_latch = (self.t1_latch & 0x00FF) | (value as u16) << 8;
                self.ifr &= !T1;
            }
            T2C_L => self.t2_latch = value,
            T2C_H => {
                self.t2_counter = (value as u16) << 8 | self.t2_latch as u16;
                self.t2_load = true;
                self.t2_armed = true;
                self.ifr &= !T2;
            }
            SR => self.sr = value,
            ACR => self.acr = value,
            PCR => self.pcr = value,
            IFR => self.ifr &= !(value & 0x7F),
            _ => {
                // The IER: bit 7 set, the bits given enabled; clear, disabled.
                if value & 0x80 != 0 {
                    self.ier |= value & 0x7F;
                } else {
                    self.ier &= !(value & 0x7F);
                }
            }
        }
    }

    /// A cycle: the timers counted.
    pub fn tick(&mut self) {
        // Timer 1: loaded, or down a count; past 0, its time out.
        if self.t1_load {
            self.t1_counter = self.t1_latch;
            self.t1_load = false;
        } else {
            self.t1_counter = self.t1_counter.wrapping_sub(1);

            if self.t1_counter == 0xFFFF {
                if self.t1_armed {
                    self.ifr |= T1;
                    let free_running = self.acr & 0x40 != 0;
                    self.t1_armed = free_running;
                }
                self.t1_load = true;
            }
        }

        // Timer 2: loaded, or down a count, its one time out at $FFFF;
        // counting PB6's pulses, still.
        if self.t2_load {
            self.t2_load = false;
        } else if self.acr & 0x20 == 0 {
            self.t2_counter = self.t2_counter.wrapping_sub(1);
            if self.t2_counter == 0xFFFF && self.t2_armed {
                self.ifr |= T2;
                self.t2_armed = false;
            }
        }
    }
}
