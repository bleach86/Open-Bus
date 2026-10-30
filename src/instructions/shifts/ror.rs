use crate::cpu::CPU;

impl CPU {
    fn ror_set_flags(&mut self, original: u8, result: u8) {
        self.status.carry = (original & 0x01) != 0; // the bit rotated out
        self.status.zero = result == 0;
        self.status.negative = (result & 0x80) != 0;
    }

    fn ror_memory(&mut self, addr: u16, cycles: &mut u32) {
        let original = self.read_byte(addr, cycles);
        let carry_in = if self.status.carry { 1 } else { 0 };
        let result = (original >> 1) | (carry_in << 7);
        self.write_byte(addr, result, cycles);
        self.ror_set_flags(original, result);
        *cycles = cycles.wrapping_add(1);
    }

    pub fn ror_accumulator(&mut self, cycles: &mut u32) {
        let original = self.a;
        let carry_in = if self.status.carry { 1 } else { 0 };
        self.a = (original >> 1) | (carry_in << 7);
        self.ror_set_flags(original, self.a);

        *cycles = cycles.wrapping_add(1);
    }

    pub fn ror_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        self.ror_memory(addr, cycles);
    }

    pub fn ror_zero_page_x(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles).wrapping_add(self.x) as u16;
        self.ror_memory(addr, cycles);

        *cycles = cycles.wrapping_add(1);
    }

    pub fn ror_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        self.ror_memory(addr, cycles);
    }

    pub fn ror_absolute_x(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles).wrapping_add(self.x as u16);
        self.ror_memory(addr, cycles);

        *cycles = cycles.wrapping_add(1);
    }
}
