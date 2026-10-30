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

    fn adc_decimal(&mut self, operand: u8) {
        let carry_in = if self.status.carry { 1 } else { 0 };

        let mut low_nibble = (self.a & 0x0F) + (operand & 0x0F) + carry_in;

        let mut high_nibble = (self.a >> 4).wrapping_add(operand >> 4);
        let mut carry_to_high = false;

        // If low nibble is > 9, adjust it and carry to high nibble
        if low_nibble > 9 {
            low_nibble = low_nibble.wrapping_sub(10);
            carry_to_high = true;
        }

        high_nibble = high_nibble.wrapping_add(u8::from(carry_to_high));

        // Adjust high nibble if it's > 9 and set final carry flag
        let adjusted_high = if high_nibble > 9 {
            high_nibble.wrapping_sub(10)
        } else {
            high_nibble
        };

        let result = (adjusted_high << 4) | (low_nibble & 0x0f);

        self.adc_set_flags(result as u16, operand);

        self.a = result;
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
        *cycles = cycles.wrapping_sub(1);

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

        let addr = self.fetch_word(cycles).wrapping_add(self.x as u16);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }
        *cycles = cycles.wrapping_sub(1);

        true
    }

    pub fn adc_absolute_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles).wrapping_add(self.y as u16);
        let operand = self.read_byte(addr, cycles);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }
        *cycles = cycles.wrapping_sub(1);

        true
    }

    pub fn adc_indirect_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
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
        *cycles = cycles.wrapping_sub(1);

        true
    }

    pub fn adc_indirect_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 5;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        let ptr = base_addr;
        let addr = self
            .read_zero_page_word(ptr, cycles)
            .wrapping_add(self.y as u16);
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
