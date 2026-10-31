use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    /// The 65C02's PLY.
    pub fn ply(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
        self.read_byte(self.sp_to_addr()); // the stack, before S moves on
        self.y = self.pop_byte();

        self.status.zero = self.y == 0;
        self.status.negative = (self.y & 0x80) != 0;
    }
}
