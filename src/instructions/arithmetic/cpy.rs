use crate::cpu::CPU;

impl CPU {
    fn cpy_set_flags(&mut self, result: u8, operand: u8) {
        self.status.carry = self.y >= operand;
        self.status.zero = self.y == operand;
        self.status.negative = (result & 0x80) != 0;
    }

    pub fn cpy_immediate(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let operand = self.fetch_byte(cycles);
        let result = self.y.wrapping_sub(operand);
        self.cpy_set_flags(result, operand);
        true
    }

    pub fn cpy_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 3;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        let operand = self.read_byte(addr, cycles);
        let result = self.y.wrapping_sub(operand);
        self.cpy_set_flags(result, operand);
        true
    }

    pub fn cpy_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        let operand = self.read_byte(addr, cycles);
        let result = self.y.wrapping_sub(operand);
        self.cpy_set_flags(result, operand);
        true
    }
}
