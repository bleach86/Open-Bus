use crate::cpu::CPU;

impl CPU {
    fn rol_set_flags(&mut self, original: u8, result: u8) {
        self.status.carry = (original & 0x80) != 0;
        self.status.zero = result == 0;
        self.status.negative = (result & 0x80) != 0;
    }

    fn rol_memory(&mut self, addr: u16, cycles: &mut u32) {
        let original = self.read_byte(addr, cycles);
        let carry_in = if self.status.carry { 1 } else { 0 };
        let result = (original << 1) | carry_in;
        self.write_byte(addr, result, cycles);
        self.rol_set_flags(original, result);
        *cycles = cycles.wrapping_sub(1);
    }

    pub fn rol_accumulator(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 2;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let original = self.a;
        let carry_in = if self.status.carry { 1 } else { 0 };
        self.a = (original << 1) | carry_in;
        self.rol_set_flags(original, self.a);

        *cycles = cycles.wrapping_sub(1);
        true
    }

    pub fn rol_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 5;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        self.rol_memory(addr, cycles);
        true
    }

    pub fn rol_zero_page_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 6;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles).wrapping_add(self.x) as u16;
        self.rol_memory(addr, cycles);

        *cycles = cycles.wrapping_sub(1);
        true
    }

    pub fn rol_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 6;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        self.rol_memory(addr, cycles);
        true
    }

    pub fn rol_absolute_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 7;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles).wrapping_add(self.x as u16);
        self.rol_memory(addr, cycles);

        *cycles = cycles.wrapping_sub(1);
        true
    }
}
