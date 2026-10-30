use crate::cpu::CPU;

impl CPU {
    fn ora_set_flags(&mut self) {
        self.status.zero = self.a == 0;
        self.status.negative = (self.a & 0x80) != 0;
    }

    pub fn or_immediate(&mut self, cycles: &mut u32) {
        let value = self.fetch_byte(cycles);
        self.a |= value;
        self.ora_set_flags();
    }

    pub fn or_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        let value = self.read_byte(addr, cycles);
        self.a |= value;
        self.ora_set_flags();
    }

    pub fn or_zero_page_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.x) as u16;
        let value = self.read_byte(addr, cycles);

        *cycles = cycles.wrapping_add(1);
        self.a |= value;
        self.ora_set_flags();
    }

    pub fn or_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        let value = self.read_byte(addr, cycles);
        self.a |= value;
        self.ora_set_flags();
    }

    pub fn or_absolute_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.x as u16);
        let value = self.read_byte(addr, cycles);

        // Additional cycle if page crossed
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            *cycles = cycles.wrapping_add(1);
        }
        self.a |= value;
        self.ora_set_flags();
    }

    pub fn or_absolute_y(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.y as u16);
        let value = self.read_byte(addr, cycles);

        // Additional cycle if page crossed
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            *cycles = cycles.wrapping_add(1);
        }
        self.a |= value;
        self.ora_set_flags();
    }

    pub fn or_indirect_x(&mut self, cycles: &mut u32) {
        let addr = self.fetch_addr_indirect_x(cycles);
        let value = self.read_byte(addr, cycles);

        *cycles = cycles.wrapping_add(1);
        self.a |= value;
        self.ora_set_flags();
    }

    pub fn or_indirect_y(&mut self, cycles: &mut u32) {
        let addr = self.fetch_addr_indirect_y(cycles);
        let value = self.read_byte(addr, cycles);

        self.a |= value;
        self.ora_set_flags();
    }
}
