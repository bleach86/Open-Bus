use crate::cpu::CPU;

impl CPU {
    pub fn nop(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
    }
}
