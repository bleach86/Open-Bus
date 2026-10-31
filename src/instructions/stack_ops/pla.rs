use crate::cpu::CPU;

impl CPU {
    pub fn pla(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
        self.read_byte(self.sp_to_addr()); // the stack, before S moves on
        self.a = self.pop_byte();


        self.status.zero = self.a == 0;
        self.status.negative = (self.a & 0x80) != 0;
    }
}
