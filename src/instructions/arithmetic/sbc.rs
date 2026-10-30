use crate::cpu::CPU;

impl CPU {
    /// SBC: A - value - borrow (carry clear), in binary or, with D set,
    /// decimal.
    pub fn sbc(&mut self, value: u8) {
        if self.status.decimal_mode {
            self.sbc_decimal(value);
        } else {
            self.sbc_binary(value);
        }
    }

    fn sbc_binary(&mut self, operand: u8) {
        let borrow = u16::from(!self.status.carry);
        let difference = (self.a as u16).wrapping_sub(operand as u16).wrapping_sub(borrow);
        let result = difference as u8;
        self.status.carry = difference < 0x100;
        self.status.overflow = (self.a ^ operand) & (self.a ^ result) & 0x80 != 0;
        self.a = result;
        self.set_nz(result);
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
