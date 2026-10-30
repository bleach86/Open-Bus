use crate::cpu::CPU;

impl CPU {
    fn and_set_flags(&mut self) {
        self.status.zero = self.a == 0;
        self.status.negative = (self.a & 0x80) != 0;
    }

    pub fn and_immediate(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let value = self.fetch_byte(cycles);
        self.a &= value;
        self.and_set_flags();
        true
    }

    pub fn and_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 3;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        let value = self.read_byte(addr, cycles);
        self.a &= value;
        self.and_set_flags();
        true
    }

    pub fn and_zero_page_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        self.read_byte(base_addr as u16, cycles); // read while X is added
        let addr = base_addr.wrapping_add(self.x) as u16;
        let value = self.read_byte(addr, cycles);
        self.a &= value;
        self.and_set_flags();
        true
    }

    pub fn and_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        let value = self.read_byte(addr, cycles);
        self.a &= value;
        self.and_set_flags();
        true
    }

    pub fn and_absolute_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.x as u16);
        self.index_cycle(base_addr, addr, false, cycles);
        let value = self.read_byte(addr, cycles);
        self.a &= value;
        self.and_set_flags();
        true
    }

    pub fn and_absolute_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.y as u16);
        self.index_cycle(base_addr, addr, false, cycles);
        let value = self.read_byte(addr, cycles);
        self.a &= value;
        self.and_set_flags();
        true
    }

    pub fn and_indirect_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_addr_indirect_x(cycles);
        let value = self.read_byte(addr, cycles);
        self.a &= value;
        self.and_set_flags();
        true
    }

    pub fn and_indirect_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 5;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        let ptr = self.read_zero_page_word(base_addr, cycles);
        let addr = ptr.wrapping_add(self.y as u16);
        self.index_cycle(ptr, addr, false, cycles);
        let value = self.read_byte(addr, cycles);
        self.a &= value;
        self.and_set_flags();
        true
    }
}
