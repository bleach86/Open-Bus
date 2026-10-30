use crate::cpu::CPU;

impl CPU {
    fn ror_set_flags(&mut self, original: u8, result: u8) {
        self.status.carry = (original & 1) != 0;
        self.status.zero = result == 0;
        self.status.negative = (result & 0x80) != 0;
    }

    fn ror_memory(&mut self, addr: u16, cycles: &mut u32) {
        let original = self.read_byte(addr, cycles);
        self.write_byte(addr, original, cycles); // written back while it shifts
        let carry_in = if self.status.carry { 1 } else { 0 };
        let result = (original >> 1) | (carry_in << 7);
        self.write_byte(addr, result, cycles);
        self.ror_set_flags(original, result);
    }

    pub fn ror_accumulator(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 2;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let original = self.a;
        let carry_in = if self.status.carry { 1 } else { 0 };
        self.a = (original >> 1) | (carry_in << 7);
        self.ror_set_flags(original, self.a);

        self.read_next_ignored(cycles);
        true
    }

    pub fn ror_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 5;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        self.ror_memory(addr, cycles);
        true
    }

    pub fn ror_zero_page_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 6;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        self.read_byte(base_addr as u16, cycles); // read while X is added
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.ror_memory(addr, cycles);


        true
    }

    pub fn ror_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 6;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        self.ror_memory(addr, cycles);
        true
    }

    pub fn ror_absolute_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 7;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.x as u16);
        self.index_cycle(base_addr, addr, true, cycles);
        self.ror_memory(addr, cycles);

        true
    }
}
