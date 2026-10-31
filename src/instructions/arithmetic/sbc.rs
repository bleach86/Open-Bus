use crate::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
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

    pub fn sbc_immediate(&mut self) {
        let operand = self.fetch_byte();

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_zero_page_x(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_absolute(&mut self) {
        let addr = self.fetch_word();
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_absolute_x(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(u16::from(self.x));
        // Additional cycle if page crossed: the address is read first with
        // the high byte not yet carried into
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF));
        }
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_absolute_y(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(u16::from(self.y));
        // Additional cycle if page crossed: the address is read first with
        // the high byte not yet carried into
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF));
        }
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }

    pub fn sbc_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y();
        let operand = self.read_byte(addr);

        match self.status.decimal_mode {
            false => self.sbc_binary(operand),
            true => self.sbc_decimal(operand),
        };
    }
}
