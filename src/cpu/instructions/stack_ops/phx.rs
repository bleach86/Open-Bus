use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    /// The 65C02's PHX.
    pub fn phx(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
        self.push_byte(self.x);
    }
}
