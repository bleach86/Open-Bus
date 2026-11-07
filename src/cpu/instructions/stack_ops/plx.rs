use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// The 65C02's PLX.
    pub fn plx(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
        self.read_byte(self.sp_to_addr()); // the stack, before S moves on
        self.x = self.pop_byte();

        self.status.zero = self.x == 0;
        self.status.negative = (self.x & 0x80) != 0;
    }
}
