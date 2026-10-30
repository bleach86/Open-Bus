use crate::cpu::CPU;

impl CPU {
    /// An IRQ or NMI, taken between instructions: as BRK, but the PC pushed
    /// is the next instruction's and the status is pushed with B clear.
    pub fn interrupt(&mut self, vector: u16, cycles: &mut u32) {
        self.push_word(self.pc, cycles);

        let status_byte = self.status.to_byte() & !0x10;
        self.push_byte(status_byte, cycles);
        self.status.interrupt_disable = true;

        self.pc = self.read_word(vector, cycles);

        *cycles = cycles.wrapping_add(2);
    }
}
