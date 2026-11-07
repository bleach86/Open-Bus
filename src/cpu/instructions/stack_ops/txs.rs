use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    pub fn txs(&mut self) {
        self.sp = self.x;

        self.read_byte(self.pc); // the next byte, read and thrown away
    }
}
