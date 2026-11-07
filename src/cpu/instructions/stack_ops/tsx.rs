use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    pub fn tsx(&mut self) {
        self.x = self.sp;
        self.status.zero = self.x == 0;
        self.status.negative = (self.x & 0x80) != 0;

        self.read_byte(self.pc); // the next byte, read and thrown away
    }
}
