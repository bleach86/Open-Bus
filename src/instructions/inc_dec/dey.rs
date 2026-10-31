use crate::cpu::CPU;

impl CPU {
    pub fn dey(&mut self) {
        self.y = self.y.wrapping_sub(1);
        self.status.zero = self.y == 0;
        self.status.negative = (self.y & 0x80) != 0;

        self.read_byte(self.pc); // the next byte, read and thrown away
    }
}
