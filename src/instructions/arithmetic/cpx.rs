use crate::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    fn cpx_set_flags(&mut self, result: u8, operand: u8) {
        self.status.carry = self.x >= operand;
        self.status.zero = self.x == operand;
        self.status.negative = (result & 0x80) != 0;
    }

    pub fn cpx_immediate(&mut self) {
        let operand = self.fetch_byte();
        let result = self.x.wrapping_sub(operand);
        self.cpx_set_flags(result, operand);
    }

    pub fn cpx_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        let operand = self.read_byte(addr);
        let result = self.x.wrapping_sub(operand);
        self.cpx_set_flags(result, operand);
    }

    pub fn cpx_absolute(&mut self) {
        let addr = self.fetch_word();
        let operand = self.read_byte(addr);
        let result = self.x.wrapping_sub(operand);
        self.cpx_set_flags(result, operand);
    }
}
