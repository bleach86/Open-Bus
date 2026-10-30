use crate::cpu::CPU;

impl CPU {
    pub fn jsr_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        // The 6502's order: the target's low byte, a cycle with the stack,
        // the return address pushed (the high byte's own address), and only
        // then the high byte, so code the stack runs over sees the push.
        let low = self.fetch_byte(cycles) as u16;
        self.read_byte(self.sp_to_addr(), cycles);
        let return_addr = self.pc;
        self.push_word(return_addr, cycles);
        let high = self.read_byte(self.pc, cycles) as u16;

        self.pc = (high << 8) | low;
        true
    }
}
