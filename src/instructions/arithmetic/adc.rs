use crate::cpu::CPU;

impl CPU {
    /// ADC: A + value + carry, in binary or, with D set, decimal.
    pub fn adc(&mut self, value: u8) {
        if self.status.decimal_mode {
            self.adc_decimal(value);
        } else {
            self.adc_binary(value);
        }
    }

    fn adc_binary(&mut self, operand: u8) {
        let sum = self.a as u16 + operand as u16 + u16::from(self.status.carry);
        let result = sum as u8;
        self.status.carry = sum > 0xFF;
        self.status.overflow = (!(self.a ^ operand) & (self.a ^ result)) & 0x80 != 0;
        self.a = result;
        self.set_nz(result);
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
