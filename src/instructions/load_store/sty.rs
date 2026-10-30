use crate::cpu::CPU;

impl CPU {
    pub fn sty_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        self.write_byte(addr, self.y, cycles);
    }

    pub fn sty_zero_page_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.write_byte(addr, self.y, cycles);

        *cycles = cycles.wrapping_add(1);
    }

    pub fn sty_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        self.write_byte(addr, self.y, cycles);
    }
}
