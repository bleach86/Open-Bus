use crate::cpu::CPU;

impl CPU {
    fn adc_set_flags(&mut self, result: u16, operand: u8) {
        self.status.carry = result > 0xFF;
        self.status.zero = (result as u8) == 0;

        let result = result as u8;

        self.status.negative = (result & 0x80) != 0;
        self.status.overflow = (!(self.a ^ operand) & (self.a ^ result)) & 0x80 != 0;
    }

    fn adc_binary(&mut self, operand: u8) {
        let carry_in: u16 = if self.status.carry { 1 } else { 0 } as u16;

        let sum = (self.a as u16) + (operand as u16) + (carry_in);
        self.adc_set_flags(sum, operand);
        self.a = sum as u8;
    }

    /// Decimal mode, as the NMOS 6502 does it: each digit adjusted by 6
    /// when it goes past 9, the carry out of the high digit. Z comes from
    /// the binary sum, N and V from the sum before the high digit's
    /// adjustment.
    pub(crate) fn adc_decimal(&mut self, operand: u8) {
        let (a, b) = (self.a as u16, operand as u16);
        let carry_in = u16::from(self.status.carry);

        let mut low = (a & 0x0F) + (b & 0x0F) + carry_in;
        if low > 9 {
            low += 6;
        }
        let mut high = (a >> 4) + (b >> 4) + u16::from(low > 0x0F);
        let unadjusted = ((high << 4) | (low & 0x0F)) as u8;

        self.status.zero = (a + b + carry_in) & 0xFF == 0;
        self.status.negative = unadjusted & 0x80 != 0;
        self.status.overflow = (!(self.a ^ operand) & (self.a ^ unadjusted)) & 0x80 != 0;

        if high > 9 {
            high += 6;
        }
        self.status.carry = high > 0x0F;
        self.a = ((high << 4) | (low & 0x0F)) as u8;
    }

    pub fn adc_immediate(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let operand = self.fetch_byte(cycles);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }

        true
    }

    pub fn adc_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 3;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }

        true
    }

    pub fn adc_zero_page_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        self.read_byte(base_addr as u16, cycles); // read while X is added
        let addr = base_addr.wrapping_add(self.x) as u16;
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }

        true
    }

    pub fn adc_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }

        true
    }

    pub fn adc_absolute_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.x as u16);
        self.index_cycle(base_addr, addr, false, cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }

        true
    }

    pub fn adc_absolute_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.y as u16);
        self.index_cycle(base_addr, addr, false, cycles);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }

        true
    }

    pub fn adc_indirect_x(&mut self, cycles: &mut u32) -> bool {
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
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }

        true
    }

    pub fn adc_indirect_y(&mut self, cycles: &mut u32) -> bool {
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

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;

    /// Decimal adds carry out of the high digit (Dormann's test caught
    /// them never carrying).
    #[test]
    fn decimal_adds_carry() {
        let mut cpu = CPU::new();
        cpu.status.decimal_mode = true;
        for (a, b, carry_in, sum, carry) in [(0x50, 0x50, false, 0x00, true), (0x99, 0x01, false, 0x00, true), (0x45, 0x27, true, 0x73, false), (0x58, 0x46, true, 0x05, true)] {
            cpu.a = a;
            cpu.status.carry = carry_in;
            cpu.adc_decimal(b);
            assert_eq!((cpu.a, cpu.status.carry), (sum, carry), "${a:02X} + ${b:02X} + {}", u8::from(carry_in));
        }
    }
}
