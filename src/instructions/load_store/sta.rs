use crate::cpu::CPU;

impl CPU {
    pub fn sta_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        self.write_byte(addr, self.a, cycles);
    }

    pub fn sta_zero_page_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.write_byte(addr, self.a, cycles);

        *cycles = cycles.wrapping_add(1);
    }

    pub fn sta_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        self.write_byte(addr, self.a, cycles);
    }

    pub fn sta_absolute_x(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles).wrapping_add(self.x as u16);

        *cycles = cycles.wrapping_add(1);

        self.write_byte(addr, self.a, cycles);
    }

    pub fn sta_absolute_y(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles).wrapping_add(self.y as u16);

        *cycles = cycles.wrapping_add(1);

        self.write_byte(addr, self.a, cycles);
    }

    pub fn sta_indirect_x(&mut self, cycles: &mut u32) {
        let addr = self.fetch_addr_indirect_x(cycles);

        *cycles = cycles.wrapping_add(1);

        self.write_byte(addr, self.a, cycles);
    }

    pub fn sta_indirect_y(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_byte(cycles);
        let ptr = base_addr;
        // The pointer's high byte wraps around in the zero page.
        let lo = self.read_byte(ptr as u16, cycles) as u16;
        let hi = self.read_byte(ptr.wrapping_add(1) as u16, cycles) as u16;
        let base_addr = (hi << 8) | lo;
        let addr = base_addr.wrapping_add(self.y as u16);

        *cycles = cycles.wrapping_add(1);

        self.write_byte(addr, self.a, cycles);
    }
}
