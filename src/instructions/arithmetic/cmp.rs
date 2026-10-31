use crate::cpu::CPU;

impl CPU {
    fn cmp_set_flags(&mut self, result: u8, operand: u8) {
        self.status.carry = self.a >= operand;
        self.status.zero = self.a == operand;
        self.status.negative = (result & 0x80) != 0;
    }

    pub fn cmp_immediate(&mut self) {
        let operand = self.fetch_byte();
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        let operand = self.read_byte(addr);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_zero_page_x(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        let operand = self.read_byte(addr);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_absolute(&mut self) {
        let addr = self.fetch_word();
        let operand = self.read_byte(addr);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_absolute_x(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.x as u16);
        // Additional cycle if page crossed: the address is read first with
        // the high byte not yet carried into
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF));
        }
        let operand = self.read_byte(addr);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_absolute_y(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.y as u16);
        // Additional cycle if page crossed: the address is read first with
        // the high byte not yet carried into
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF));
        }
        let operand = self.read_byte(addr);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        let operand = self.read_byte(addr);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }

    pub fn cmp_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y();
        let operand = self.read_byte(addr);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
    }
}
