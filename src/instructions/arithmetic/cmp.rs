use crate::cpu::CPU;

impl CPU {
    fn cmp_set_flags(&mut self, result: u8, operand: u8) {
        self.status.carry = self.a >= operand;
        self.status.zero = self.a == operand;
        self.status.negative = (result & 0x80) != 0;
    }

    pub fn cmp_immediate(&mut self, cycles: &mut u32) {
        let operand = self.fetch_byte(cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_zero_page_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.x) as u16;
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
        *cycles = cycles.wrapping_add(1);
    }

    pub fn cmp_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_absolute_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.x as u16);
        let operand = self.read_byte(addr, cycles);

        // Additional cycle if page crossed
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            *cycles = cycles.wrapping_add(1);
        }
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_absolute_y(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.y as u16);
        let operand = self.read_byte(addr, cycles);

        // Additional cycle if page crossed
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            *cycles = cycles.wrapping_add(1);
        }
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_indirect_x(&mut self, cycles: &mut u32) {
        let addr = self.fetch_addr_indirect_x(cycles);
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
        *cycles = cycles.wrapping_add(1);
    }

    pub fn cmp_indirect_y(&mut self, cycles: &mut u32) {
        let addr = self.fetch_addr_indirect_y(cycles);
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }
}
