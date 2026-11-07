use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    pub fn jsr_absolute(&mut self) {
        // The 6502 pushes the return address (its last byte's) before it
        // reads the target's high byte.
        let low = self.fetch_byte() as u16;
        let return_addr = self.pc;
        self.read_byte(self.sp_to_addr()); // the stack, before the push
        self.push_word(return_addr);
        let high = self.fetch_byte() as u16;
        let sub_addr = (high << 8) | low;

        self.pc = sub_addr;
    }
}
