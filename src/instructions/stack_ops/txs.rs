use crate::cpu::CPU;

impl CPU {
    pub fn txs(&mut self) {
        self.sp = self.x;

        self.read_byte(self.pc); // the next byte, read and thrown away
    }
}
