use crate::cpu::CPU;

impl CPU {
    fn cmp_set_flags(&mut self, result: u8, operand: u8) {
        self.status.carry = self.a >= operand;
        self.status.zero = self.a == operand;
        self.status.negative = (result & 0x80) != 0;
    }

    pub fn cmp_immediate(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let operand = self.fetch_byte(cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
        true
    }

    pub fn cmp_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 3;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
        true
    }

    pub fn cmp_zero_page_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        self.read_byte(base_addr as u16, cycles); // read while X is added
        let addr = base_addr.wrapping_add(self.x) as u16;
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);

        true
    }

    pub fn cmp_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
        true
    }

    pub fn cmp_absolute_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.x as u16);
        self.index_cycle(base_addr, addr, false, cycles);
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
        true
    }

    pub fn cmp_absolute_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.y as u16);
        self.index_cycle(base_addr, addr, false, cycles);
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);
        true
    }

    pub fn cmp_indirect_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        self.read_byte(base_addr as u16, cycles); // read while X is added
        let ptr = base_addr.wrapping_add(self.x);
        let addr = self.read_zero_page_word(ptr, cycles);
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);

        true
    }

    pub fn cmp_indirect_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 5;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        let ptr = base_addr;
        let indirect = self.read_zero_page_word(ptr, cycles);
        let addr = indirect.wrapping_add(self.y as u16);
        self.index_cycle(indirect, addr, false, cycles);
        let operand = self.read_byte(addr, cycles);
        let result = self.a.wrapping_sub(operand);
        self.cmp_set_flags(result, operand);

        true
    }
}
