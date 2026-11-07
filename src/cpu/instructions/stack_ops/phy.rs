use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// The 65C02's PHY.
    pub fn phy(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
        self.push_byte(self.y);
    }
}
