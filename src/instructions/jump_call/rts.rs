use crate::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub fn rts(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
        self.read_byte(self.sp_to_addr()); // the stack, before S moves on
        let ret_addr = self.pop_word();
        self.read_byte(ret_addr); // the return address, read before it is passed
        self.pc = ret_addr.wrapping_add(1);
    }
}
