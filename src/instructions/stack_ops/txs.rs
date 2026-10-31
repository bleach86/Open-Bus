use crate::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub fn txs(&mut self) {
        self.sp = self.x;

        self.read_byte(self.pc); // the next byte, read and thrown away
    }
}
