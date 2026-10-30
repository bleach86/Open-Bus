use crate::cpu::CPU;

impl CPU {
    pub fn jsr_absolute(&mut self, cycles: &mut u32) {
        // The 6502 pushes the return address (its last byte's) before it
        // reads the target's high byte.
        let low = self.fetch_byte(cycles) as u16;
        let return_addr = self.pc;
        self.push_word(return_addr, cycles);
        let high = self.fetch_byte(cycles) as u16;
        let sub_addr = (high << 8) | low;

        self.pc = sub_addr;
        *cycles = cycles.wrapping_add(1);
    }
}
