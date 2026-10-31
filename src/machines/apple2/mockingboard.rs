//! The Mockingboard (its A and C), in slot 4 (or 5, a second): two 6522s,
//! at $Cn00 and $Cn80, each driving an AY-3-8910, six voices, in stereo:
//! the first AY's three on the left, the second's on the right. A 6522's
//! port A is its AY's data bus; port B's bits 0, 1 and 2 its BC1, BDIR and
//! /RESET pins, so the AY latches a register's number, writes it or reads
//! it as port B says, with port A's byte. Both 6522s hold the Apple's IRQ line: a music
//! player's timer interrupts, 60 a second or so. The AYs are clocked with
//! the CPU. It has no ROM; its 6522s answer all of its slot's page, each
//! its 16 registers over again.

use crate::chips::ay38910::Ay38910;
use crate::chips::via6522::Via6522;

pub struct Mockingboard {
    pub vias: [Via6522; 2],
    pub ays: [Ay38910; 2],
}

impl Mockingboard {
    pub fn new() -> Self {
        Mockingboard { vias: [Via6522::new(), Via6522::new()], ays: [Ay38910::new(), Ay38910::new()] }
    }

    /// The 6522 at `addr`: the first below $C480, the second from it.
    fn chip(addr: u16) -> usize {
        (addr as usize >> 7) & 1
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        let chip = Mockingboard::chip(addr);
        self.vias[chip].read(addr as u8 & 0x0F)
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        let chip = Mockingboard::chip(addr);
        self.vias[chip].write(addr as u8 & 0x0F, value);
        self.ay_bus(chip);
    }

    /// The AY behind 6522 `chip`, given its pins as the 6522's ports have
    /// them now: reset while /RESET is low; else its bus, the byte it
    /// reads on port A's pins, or nothing (pulled up).
    fn ay_bus(&mut self, chip: usize) {
        let (via, ay) = (&mut self.vias[chip], &mut self.ays[chip]);
        let pins = via.port_b();
        if pins & 0x04 == 0 {
            ay.reset();
            via.pa_in = 0xFF;
            return;
        }
        let (bc1, bdir) = (pins & 0x01 != 0, pins & 0x02 != 0);
        via.pa_in = ay.bus(bdir, bc1, via.port_a()).unwrap_or(0xFF);
    }

    /// RESET: the 6522s reset, their ports inputs; the AYs as they were.
    pub fn reset(&mut self) {
        for via in &mut self.vias {
            via.reset();
        }
    }

    /// Whether either 6522 holds the IRQ line.
    pub fn irq(&self) -> bool {
        self.vias.iter().any(Via6522::irq)
    }

    /// A cycle: the 6522s' timers, the AYs' clocks.
    pub fn tick(&mut self) {
        for via in &mut self.vias {
            via.tick();
        }
        for ay in &mut self.ays {
            ay.tick();
        }
    }

    /// Its left and right: each AY's three voices together, each 0 to 1.
    pub fn output(&self) -> (f32, f32) {
        let voices = |ay: &Ay38910| (0..3).map(|channel| ay.output(channel)).sum::<f32>();
        (voices(&self.ays[0]), voices(&self.ays[1]))
    }
}
