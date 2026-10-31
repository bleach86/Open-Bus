use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    pub fn nop(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
    }

    // The undocumented NOPs with an operand: each reads it, and what it
    // addresses, as an instruction of its mode would, and does nothing with
    // them.

    pub fn nop_immediate(&mut self) {
        self.fetch_byte(); // the operand, read and thrown away
    }

    pub fn nop_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.read_byte(addr);
    }

    pub fn nop_zero_page_x(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.x);
        self.read_byte(addr);
    }

    pub fn nop_absolute(&mut self) {
        let addr = self.fetch_word();
        self.read_byte(addr);
    }

    pub fn nop_absolute_x(&mut self) {
        let addr = self.fetch_addr_absolute_indexed(self.x);
        self.read_byte(addr);
    }

    /// The 65C02's three-byte NOPs: their operand, then its last byte again.
    pub fn nop_three_bytes(&mut self) {
        self.fetch_word();
        self.read_byte(self.pc.wrapping_sub(1));
    }
}
