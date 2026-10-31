use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    fn cpy_set_flags(&mut self, result: u8, operand: u8) {
        self.status.carry = self.y >= operand;
        self.status.zero = self.y == operand;
        self.status.negative = (result & 0x80) != 0;
    }

    pub fn cpy_immediate(&mut self) {
        let operand = self.fetch_byte();
        let result = self.y.wrapping_sub(operand);
        self.cpy_set_flags(result, operand);
    }

    pub fn cpy_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        let operand = self.read_byte(addr);
        let result = self.y.wrapping_sub(operand);
        self.cpy_set_flags(result, operand);
    }

    pub fn cpy_absolute(&mut self) {
        let addr = self.fetch_word();
        let operand = self.read_byte(addr);
        let result = self.y.wrapping_sub(operand);
        self.cpy_set_flags(result, operand);
    }
}
