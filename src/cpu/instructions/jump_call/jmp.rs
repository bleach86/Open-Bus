use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub fn jmp_absolute(&mut self) {
        let addr = self.fetch_word();

        self.pc = addr;
    }

    pub fn jmp_indirect(&mut self) {
        let ptr_addr = self.fetch_word();
        // The NMOS 6502 doesn't carry into the pointer's high byte: a
        // pointer at $xxFF has its high byte at $xx00. A 65C02 reads there
        // too, then takes a cycle more to read it where it is.
        let low = self.read_byte(ptr_addr) as u16;
        let high_addr = (ptr_addr & 0xFF00) | (ptr_addr.wrapping_add(1) & 0x00FF);
        let mut high = self.read_byte(high_addr) as u16;
        if self.cmos() {
            high = self.read_byte(ptr_addr.wrapping_add(1)) as u16;
        }
        let addr = (high << 8) | low;

        self.pc = addr;
    }

    /// The 65C02's JMP (abs,X): the pointer at the address plus X, the
    /// operand's first byte read again while X is added.
    pub fn jmp_absolute_indexed_indirect(&mut self) {
        let base_addr = self.fetch_word();
        self.read_byte(self.pc.wrapping_sub(2));
        let ptr_addr = base_addr.wrapping_add(self.x as u16);
        self.pc = self.read_word(ptr_addr);
    }
}
