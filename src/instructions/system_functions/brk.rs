use crate::cpu::CPU;

impl CPU {
    pub fn brk(&mut self, cycles: &mut u32) {
        self.pc = self.pc.wrapping_add(1);
        self.push_word(self.pc, cycles);

        self.status.break_command = true;
        let status_byte = self.status.to_byte();
        self.push_byte(status_byte, cycles);
        // As for an IRQ: the handler isn't interrupted.
        self.status.interrupt_disable = true;

        self.pc = self.read_word(0xFFFE, cycles);

        *cycles = cycles.wrapping_add(1);
    }
}
