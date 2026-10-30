use crate::cpu::CPU;

impl CPU {
    pub fn jmp_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);

        self.pc = addr;
    }

    pub fn jmp_indirect(&mut self, cycles: &mut u32) {
        let ptr_addr = self.fetch_word(cycles);
        // The NMOS 6502 doesn't carry into the pointer's high byte: a
        // pointer at $xxFF has its high byte at $xx00.
        let low = self.read_byte(ptr_addr, cycles) as u16;
        let high_addr = (ptr_addr & 0xFF00) | (ptr_addr.wrapping_add(1) & 0x00FF);
        let high = self.read_byte(high_addr, cycles) as u16;
        let addr = (high << 8) | low;

        self.pc = addr;
    }
}
