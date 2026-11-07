use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// The WDC 65C02's WAI: it waits, its clock still going, for an IRQ or
    /// an NMI; one comes, it goes on, taking it unless it's an IRQ with I
    /// set.
    pub fn wai(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away,
        self.read_byte(self.pc); // twice
        self.waiting = true;
    }

    /// The WDC 65C02's STP: it stops, until it's reset.
    pub fn stp(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away,
        self.read_byte(self.pc); // twice
        self.stopped = true;
    }
}
