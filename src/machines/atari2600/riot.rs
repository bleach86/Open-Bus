//! The RIOT, a 6532: the 2600's 128 bytes of RAM ($80 to $FF, and again at
//! $180 up, the stack's), its timer, and its two ports: A the joysticks
//! (SWCHA, the left's in bits 7 to 4, right, left, down, up; the right's in
//! 3 to 0; 0 while pressed), B the console's switches (SWCHB: bit 0 Reset,
//! 1 Select, 0 while pressed; 3 colour, not black and white; 6 and 7 the
//! left and right difficulties, A, not B). A port's bits written as outputs
//! (its DDR's bits set) read as written, ANDed with what's on the pins.
//!
//! The timer: written at TIM1T, TIM8T, TIM64T or T1024T, it counts down
//! every 1, 8, 64 or 1024 cycles, the first a cycle after it's written; on
//! going past 0 it sets its flag (TIMINT's bit 7) and counts down every
//! cycle from $FF. Reading INTIM, or writing it, clears the flag (but not
//! on the very cycle it went past 0), and it counts as written again. As
//! Stella has it.

use serde::{Deserialize, Serialize};

/// TIMINT's timer flag; its PA7 flag, the edge of port A's bit 7.
const TIMER_FLAG: u8 = 0x80;
const PA7_FLAG: u8 = 0x40;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Riot {
    pub ram: Vec<u8>,
    timer: u8,
    /// The timer's divider, and its count towards it.
    divider: u16,
    count: u16,
    wrapped_this_cycle: bool,
    flags: u8,
    ddr_a: u8,
    out_a: u8,
    ddr_b: u8,
    out_b: u8,
    edge_positive: bool,
    pa7_last: bool,
    /// What's on port A's pins and port B's, as the controllers and the
    /// switches have them.
    #[serde(skip)]
    pub pins_a: u8,
    #[serde(skip)]
    pub pins_b: u8,
}

impl Riot {
    /// As at power-on: its RAM `ram`, its timer counting down every 1024
    /// cycles from `timer`, as random as a real one's.
    pub fn new(ram: Vec<u8>, timer: u8) -> Riot {
        Riot {
            ram,
            timer,
            divider: 1024,
            count: 0,
            wrapped_this_cycle: false,
            flags: 0,
            ddr_a: 0,
            out_a: 0,
            ddr_b: 0,
            out_b: 0,
            edge_positive: false,
            pa7_last: true,
            pins_a: 0xFF,
            pins_b: 0xFF,
        }
    }

    /// A CPU cycle: the timer counted, and PA7's edge looked for.
    pub fn cycle(&mut self) {
        self.wrapped_this_cycle = false;
        self.count += 1;
        let ticked = self.count >= self.divider;
        if ticked {
            self.count = 0;
        }
        if self.flags & TIMER_FLAG != 0 {
            self.timer = self.timer.wrapping_sub(1);
            self.wrapped_this_cycle = self.timer == 0xFF;
        } else if ticked {
            if self.timer == 0 {
                self.timer = 0xFF;
                self.flags |= TIMER_FLAG;
                self.wrapped_this_cycle = true;
            } else {
                self.timer -= 1;
            }
        }
        let pa7 = self.port_a() & 0x80 != 0;
        if pa7 != self.pa7_last && pa7 == self.edge_positive {
            self.flags |= PA7_FLAG;
        }
        self.pa7_last = pa7;
    }

    fn port_a(&self) -> u8 {
        self.driven_a() & self.pins_a
    }

    /// What it puts on port A's pins: its outputs as written, its inputs
    /// high.
    pub fn driven_a(&self) -> u8 {
        self.out_a | !self.ddr_a
    }

    /// A read of `addr`: RAM if A9's clear, else its registers.
    pub fn read(&mut self, addr: u16) -> u8 {
        if addr & 0x0200 == 0 {
            return self.ram[addr as usize & 0x7F];
        }
        match addr & 0x07 {
            0x00 => self.port_a(),
            0x01 => self.ddr_a,
            0x02 => (self.out_b | !self.ddr_b) & (self.pins_b | self.ddr_b),
            0x03 => self.ddr_b,
            0x04 | 0x06 => {
                if !self.wrapped_this_cycle {
                    self.flags &= !TIMER_FLAG;
                }
                self.timer
            }
            _ => {
                let flags = self.flags;
                self.flags &= !PA7_FLAG;
                flags
            }
        }
    }

    /// A write to `addr`: RAM if A9's clear; else A2 says the timer (or with
    /// A4 clear, PA7's edge), not the ports.
    pub fn write(&mut self, addr: u16, value: u8) {
        if addr & 0x0200 == 0 {
            self.ram[addr as usize & 0x7F] = value;
            return;
        }
        if addr & 0x04 != 0 {
            if addr & 0x10 != 0 {
                self.divider = [1, 8, 64, 1024][addr as usize & 3];
                self.timer = value;
                self.count = self.divider - 1;
                if !self.wrapped_this_cycle {
                    self.flags &= !TIMER_FLAG;
                }
            } else {
                self.edge_positive = addr & 0x01 != 0;
            }
            return;
        }
        match addr & 0x03 {
            0 => self.out_a = value,
            1 => self.ddr_a = value,
            2 => self.out_b = value,
            _ => self.ddr_b = value,
        }
    }
}
