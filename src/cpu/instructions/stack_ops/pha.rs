use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub fn pha(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
        self.push_byte(self.a);
    }
}
