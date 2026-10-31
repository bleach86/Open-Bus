use crate::cpu::CPU;

impl CPU {
    pub fn pha(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
        self.push_byte(self.a);
    }
}
