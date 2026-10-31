use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    fn adc_set_flags(&mut self, result: u16, operand: u8) {
        self.status.carry = result > 0xFF;

        let result = result as u8;
        self.status.zero = result == 0;

        self.status.negative = (result & 0x80) != 0;
        self.status.overflow = (!(self.a ^ operand) & (self.a ^ result)) & 0x80 != 0;
    }

    pub(crate) fn adc_binary(&mut self, operand: u8) {
        let carry_in: u16 = if self.status.carry { 1 } else { 0 } as u16;

        let sum = (self.a as u16) + (operand as u16) + (carry_in);
        self.adc_set_flags(sum, operand);
        self.a = sum as u8;
    }

    pub(crate) fn adc_decimal(&mut self, operand: u8) {
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
        // The NMOS 6502's flags in decimal mode: N and V from the sum before
        // its high nibble is adjusted, the decimal carry as bit 8; Z from
        // the binary sum.
        let unadjusted = ((high_nibble << 4) | (low_nibble & 0x0f)) as u16;
        let sum = unadjusted | (u16::from(high_nibble > 9) << 8);

        self.adc_set_flags(sum, operand);
        self.status.zero = self.a.wrapping_add(operand).wrapping_add(carry_in) == 0;

        self.a = result;
    }

    pub fn adc_immediate(&mut self) {
        let operand = self.fetch_byte();

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }
    }

    pub fn adc_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }
    }

    pub fn adc_zero_page_x(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }
    }

    pub fn adc_absolute(&mut self) {
        let addr = self.fetch_word();
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }
    }

    pub fn adc_absolute_x(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.x as u16);
        // Additional cycle if page crossed: the address is read first with
        // the high byte not yet carried into
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF));
        }
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }
    }

    pub fn adc_absolute_y(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.y as u16);
        // Additional cycle if page crossed: the address is read first with
        // the high byte not yet carried into
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF));
        }
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }
    }

    pub fn adc_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }
    }

    pub fn adc_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y();
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => {
                self.adc_binary(operand);
            }
            true => {
                self.adc_decimal(operand);
            }
        }
    }
}
