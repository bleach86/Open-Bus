use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    pub fn dex(&mut self) {
        self.x = self.x.wrapping_sub(1);
        self.status.zero = self.x == 0;
        self.status.negative = self.x & 0x80 != 0;

        self.read_byte(self.pc); // the next byte, read and thrown away
    }
}
