use crate::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub fn rti(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
        self.read_byte(self.sp_to_addr()); // the stack, before S moves on
        let status_byte = self.pop_byte();
        self.status.from_byte(status_byte);

        let ret_pc = self.pop_word();
        self.pc = ret_pc;
    }
}
