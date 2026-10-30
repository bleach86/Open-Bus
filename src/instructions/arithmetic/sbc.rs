use std::result;

use crate::cpu::CPU;

impl CPU {
    fn sbc_binary(&mut self, operand: u8) {
        let carry_in = u16::from(self.status.carry);
        let result = (self.a as u16)
            .wrapping_sub(operand as u16)
            .wrapping_sub(1 - carry_in);

        self.status.carry = result < 0x100; // set when nothing was borrowed

        let result = result as u8;
        self.status.zero = result == 0;
        self.status.negative = (result & 0x80) != 0;
        self.status.overflow = (self.a ^ operand) & (self.a ^ result) & 0x80 != 0;

        self.a = result as u8;
    }

    fn sbc_decimal(&mut self, operand: u8) {
        let carry_in = u8::from(self.status.carry);

        let mut low_nibble = (self.a & 0x0F)
            .wrapping_sub(operand & 0x0F)
            .wrapping_sub(1 - carry_in);
        let mut high_nibble = (self.a >> 4).wrapping_sub(operand >> 4);
        let mut borrow_from_high = false;

        if (low_nibble & 0x10) != 0 {
            low_nibble = low_nibble.wrapping_sub(6);
            borrow_from_high = true;
        }

        high_nibble = high_nibble.wrapping_sub(u8::from(borrow_from_high));

        if (high_nibble & 0x10) != 0 {
            high_nibble = (high_nibble.wrapping_add(10)) & 0x0f;
        }

        let result = (high_nibble << 4) | (low_nibble & 0x0F);

        // The NMOS 6502's flags in decimal mode are the binary subtraction's.
        self.sbc_binary(operand);
        self.a = result;
    }

    pub fn sbc_immediate(&mut self, cycles: &mut u32) {
        let operand = self.fetch_byte(cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_zero_page_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.x) as u16;
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };

        *cycles = cycles.wrapping_add(1);
    }

    pub fn sbc_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_absolute_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(u16::from(self.x));
        let operand = self.read_byte(addr, cycles);

        // Additional cycle if page crossed
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            *cycles = cycles.wrapping_add(1);
        }

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_absolute_y(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(u16::from(self.y));
        let operand = self.read_byte(addr, cycles);

        // Additional cycle if page crossed
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            *cycles = cycles.wrapping_add(1);
        }

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_indirect_x(&mut self, cycles: &mut u32) {
        let addr = self.fetch_addr_indirect_x(cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
        *cycles = cycles.wrapping_add(1);
    }

    pub fn sbc_indirect_y(&mut self, cycles: &mut u32) {
        let addr = self.fetch_addr_indirect_y(cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }
}
