use crate::cpu::CPU;

impl CPU {
    pub fn jmp_absolute(&mut self, cycles: &mut u32) {
        self.pc = self.fetch_word(cycles);
    }

    /// JMP (ind). The NMOS quirk: the pointer's high byte comes from the
    /// same page, so a pointer at $xxFF reads $xx00 for it.
    pub fn jmp_indirect(&mut self, cycles: &mut u32) {
        let pointer = self.fetch_word(cycles);
        let low = self.read_byte(pointer, cycles) as u16;
        let high = self.read_byte(pointer & 0xFF00 | pointer.wrapping_add(1) & 0x00FF, cycles) as u16;
        self.pc = high << 8 | low;
    }

    /// JSR: the target's low byte; a cycle with the stack; the return
    /// address (the high byte's own, less one) pushed; then the high byte,
    /// read only now (so code under the stack sees the push first).
    pub fn jsr(&mut self, cycles: &mut u32) {
        let low = self.fetch_byte(cycles) as u16;
        self.read_byte(self.stack_address(), cycles);
        self.push_word(self.pc, cycles);
        let high = self.read_byte(self.pc, cycles) as u16;
        self.pc = high << 8 | low;
    }

    /// RTS: the return address pulled, a cycle to add one to it.
    pub fn rts(&mut self, cycles: &mut u32) {
        self.read_byte(self.pc, cycles);
        self.read_byte(self.stack_address(), cycles);
        let address = self.pop_word(cycles);
        self.read_byte(address, cycles);
        self.pc = address.wrapping_add(1);
    }

    /// RTI: the status pulled, then the return address.
    pub fn rti(&mut self, cycles: &mut u32) {
        self.read_byte(self.pc, cycles);
        self.read_byte(self.stack_address(), cycles);
        let status = self.pop_byte(cycles);
        self.status.from_byte(status);
        self.pc = self.pop_word(cycles);
    }
}
