use crate::cpu::CPU;

impl CPU {
    fn lsr_set_flags(&mut self, original: u8, result: u8) {
        self.status.carry = (original & 0x01) != 0; // the bit shifted out
        self.status.zero = result == 0;
        self.status.negative = (result & 0x80) != 0;
    }

    fn lsr_memory(&mut self, addr: u16, cycles: &mut u32) {
        let original = self.read_byte(addr, cycles);
        let result = original.wrapping_shr(1);
        self.write_byte(addr, result, cycles);
        self.lsr_set_flags(original, result);
        *cycles = cycles.wrapping_add(1);
    }

    pub fn lsr_accumulator(&mut self, cycles: &mut u32) {
        let original = self.a;
        self.a = original.wrapping_shr(1);
        self.lsr_set_flags(original, self.a);

        *cycles = cycles.wrapping_add(1);
    }

    pub fn lsr_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        self.lsr_memory(addr, cycles);
    }

    pub fn lsr_zero_page_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.lsr_memory(addr, cycles);

        *cycles = cycles.wrapping_add(1);
    }

    pub fn lsr_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        self.lsr_memory(addr, cycles);
    }

    pub fn lsr_absolute_x(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles).wrapping_add(self.x as u16);
        self.lsr_memory(addr, cycles);

        *cycles = cycles.wrapping_add(1);
    }
}
