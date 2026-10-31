use crate::cpu::CPU;

impl CPU {
    fn asl_set_flags(&mut self, original: u8, result: u8) {
        self.status.carry = (original & 0x80) != 0; // the bit shifted out
        self.status.zero = result == 0;
        self.status.negative = (result & 0x80) != 0;
    }

    fn asl_memory(&mut self, addr: u16) {
        let original = self.read_byte(addr);
        self.write_byte(addr, original); // written back first, as it was
        let result = original.wrapping_shl(1);
        self.write_byte(addr, result);
        self.asl_set_flags(original, result);
    }

    pub fn asl_accumulator(&mut self) {
        let original = self.a;
        self.a = original.wrapping_shl(1);
        self.asl_set_flags(original, self.a);

        self.read_byte(self.pc); // the next byte, read and thrown away
    }

    pub fn asl_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.asl_memory(addr);
    }

    pub fn asl_zero_page_x(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        self.asl_memory(addr);
    }

    pub fn asl_absolute(&mut self) {
        let addr = self.fetch_word();
        self.asl_memory(addr);
    }

    pub fn asl_absolute_x(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.x as u16);
        self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF)); // before the carry
        self.asl_memory(addr);
    }
}
