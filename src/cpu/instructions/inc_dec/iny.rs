use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub fn iny(&mut self) {
        self.y = self.y.wrapping_add(1);
        self.status.zero = self.y == 0;
        self.status.negative = (self.y & 0x80) != 0;

        self.read_byte(self.pc); // the next byte, read and thrown away
    }
}
