use crate::cpu::CPU;

impl CPU {
    pub fn ldx_set_status(&mut self) {
        self.status.zero = self.x == 0;
        self.status.negative = (self.x & 0x80) != 0;
    }

    pub fn ldx_immediate(&mut self, cycles: &mut u32) {
        let value = self.fetch_byte(cycles);
        self.x = value;
        self.ldx_set_status();
    }

    pub fn ldx_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        let value = self.read_byte(addr, cycles);
        self.x = value;

        self.ldx_set_status();
    }

    pub fn ldx_zero_page_y(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.y) as u16;
        let value = self.read_byte(addr, cycles);

        *cycles = cycles.wrapping_add(1);
        self.x = value;

        self.ldx_set_status();
    }

    pub fn ldx_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        let value = self.read_byte(addr, cycles);
        self.x = value;

        self.ldx_set_status();
    }

    pub fn ldx_absolute_y(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.y as u16);
        let value = self.read_byte(addr, cycles);

        // Additional cycle if page crossed
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            *cycles = cycles.wrapping_add(1);
        }
        self.x = value;

        self.ldx_set_status();
    }
}
