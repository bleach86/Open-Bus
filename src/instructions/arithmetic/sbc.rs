use crate::cpu::CPU;

impl CPU {
    fn sbc_binary(&mut self, operand: u8) {
        let carry_in = u16::from(self.status.carry);
        let result = (self.a as u16)
            .wrapping_sub(operand as u16)
            .wrapping_sub(1 - carry_in);

        self.status.carry = (self.a as u16) >= operand as u16 + (1 - carry_in);
        self.status.zero = (result as u8) == 0;

        let result = result as u8;
        self.status.negative = (result & 0x80) != 0;
        self.status.overflow = (self.a ^ operand) & (self.a ^ result) & 0x80 != 0;

        self.a = result as u8;
    }

    /// Decimal mode, as the NMOS 6502 does it: the flags are the binary
    /// subtraction's, the result each digit borrowed from and adjusted by 6.
    pub(crate) fn sbc_decimal(&mut self, operand: u8) {
        let (a, b) = (self.a as i16, operand as i16);
        let carry_in = i16::from(self.status.carry);

        let mut low = (a & 0x0F) - (b & 0x0F) + carry_in - 1;
        if low < 0 {
            low = ((low - 6) & 0x0F) - 0x10;
        }
        let mut result = (a & 0xF0) - (b & 0xF0) + low;
        if result < 0 {
            result -= 0x60;
        }

        self.sbc_binary(operand); // the flags, and A, replaced below
        self.a = result as u8;
    }

    pub fn sbc_immediate(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let operand = self.fetch_byte(cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };

        true
    }

    pub fn sbc_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 3;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };

        true
    }

    pub fn sbc_zero_page_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        self.read_byte(base_addr as u16, cycles); // read while X is added
        let addr = base_addr.wrapping_add(self.x) as u16;
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };

        true
    }

    pub fn sbc_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };

        true
    }

    pub fn sbc_absolute_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(u16::from(self.x));
        self.index_cycle(base_addr, addr, false, cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };

        true
    }

    pub fn sbc_absolute_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(u16::from(self.y));
        self.index_cycle(base_addr, addr, false, cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
        true
    }

    pub fn sbc_indirect_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        self.read_byte(base_addr as u16, cycles); // read while X is added
        let ptr = base_addr.wrapping_add(self.x);
        let addr = self.read_zero_page_word(ptr, cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };

        true
    }

    pub fn sbc_indirect_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 5;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        let ptr = base_addr;
        let indirect = self.read_zero_page_word(ptr, cycles);
        let addr = indirect.wrapping_add(u16::from(self.y));
        self.index_cycle(indirect, addr, false, cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };

        true
    }
}

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;

    /// Decimal subtracts borrow; the flags are the binary subtraction's.
    #[test]
    fn decimal_subtracts_borrow() {
        let mut cpu = CPU::new();
        cpu.status.decimal_mode = true;
        for (a, b, carry_in, difference, carry) in [(0x00, 0x01, true, 0x99, false), (0x46, 0x12, true, 0x34, true), (0x40, 0x13, true, 0x27, true), (0x32, 0x02, false, 0x29, true)] {
            cpu.a = a;
            cpu.status.carry = carry_in;
            cpu.sbc_decimal(b);
            assert_eq!((cpu.a, cpu.status.carry), (difference, carry), "${a:02X} - ${b:02X}, carry {}", u8::from(carry_in));
        }
        cpu.a = 0x01;
        cpu.status.carry = true;
        cpu.sbc_decimal(0x01);
        assert!(cpu.status.zero);
    }
}
