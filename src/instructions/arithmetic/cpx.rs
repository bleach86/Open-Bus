use crate::cpu::CPU;

impl CPU {
    fn cpx_set_flags(&mut self, result: u8, operand: u8) {
        self.status.carry = self.x >= operand;
        self.status.zero = self.x == operand;
        self.status.negative = (result & 0x80) != 0;
    }

    pub fn cpx_immediate(&mut self, cycles: &mut u32) {
        let operand = self.fetch_byte(cycles);
        let result = self.x.wrapping_sub(operand);
        self.cpx_set_flags(result, operand);
    }

    pub fn cpx_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        let operand = self.read_byte(addr, cycles);
        let result = self.x.wrapping_sub(operand);
        self.cpx_set_flags(result, operand);
    }

    pub fn cpx_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        let operand = self.read_byte(addr, cycles);
        let result = self.x.wrapping_sub(operand);
        self.cpx_set_flags(result, operand);
    }
}
