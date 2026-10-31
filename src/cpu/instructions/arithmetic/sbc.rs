use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub(crate) fn sbc_binary(&mut self, operand: u8) {
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

    pub(crate) fn sbc_decimal(&mut self, operand: u8) {
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

    /// SBC of `operand`, read from `addr` (none, immediate): binary, or
    /// decimal; a 65C02's decimal its own, and a cycle more.
    fn sbc(&mut self, operand: u8, addr: Option<u16>) {
        match (self.status.decimal_mode, self.cmos()) {
            (false, _) => self.sbc_binary(operand),
            (true, false) => self.sbc_decimal(operand),
            (true, true) => {
                self.sbc_decimal_65c02(operand);
                self.decimal_cycle(addr.unwrap_or(0x0000));
            }
        }
    }

    /// The 65C02's decimal SBC (Bruce Clark's sequence 3): the difference,
    /// $60 off if it borrowed, and $06 if its low nibble did; N and Z from
    /// the result; C and V the binary subtraction's.
    fn sbc_decimal_65c02(&mut self, operand: u8) {
        let borrow = 1 - i16::from(self.status.carry);
        let low = (self.a & 0x0F) as i16 - (operand & 0x0F) as i16 - borrow;
        let mut difference = self.a as i16 - operand as i16 - borrow;
        if difference < 0 {
            difference -= 0x60;
        }
        if low < 0 {
            difference -= 0x06;
        }
        self.sbc_binary(operand);
        self.a = difference as u8;
        self.status.zero = self.a == 0;
        self.status.negative = self.a & 0x80 != 0;
    }

    pub fn sbc_immediate(&mut self) {
        let operand = self.fetch_byte();

        self.sbc(operand, None);
    }

    pub fn sbc_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        let operand = self.read_byte(addr);

        self.sbc(operand, Some(addr));
    }

    pub fn sbc_zero_page_x(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        let operand = self.read_byte(addr);

        self.sbc(operand, Some(addr));
    }

    pub fn sbc_absolute(&mut self) {
        let addr = self.fetch_word();
        let operand = self.read_byte(addr);

        self.sbc(operand, Some(addr));
    }

    pub fn sbc_absolute_x(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(u16::from(self.x));
        self.read_if_page_crossed(base_addr, addr);
        let operand = self.read_byte(addr);

        self.sbc(operand, Some(addr));
    }

    pub fn sbc_absolute_y(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(u16::from(self.y));
        self.read_if_page_crossed(base_addr, addr);
        let operand = self.read_byte(addr);

        self.sbc(operand, Some(addr));
    }

    pub fn sbc_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        let operand = self.read_byte(addr);

        self.sbc(operand, Some(addr));
    }

    pub fn sbc_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y();
        let operand = self.read_byte(addr);

        self.sbc(operand, Some(addr));
    }

    pub fn sbc_zero_page_indirect(&mut self) {
        let addr = self.fetch_addr_zero_page_indirect();
        let operand = self.read_byte(addr);

        self.sbc(operand, Some(addr));
    }
}
