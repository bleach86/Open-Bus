use crate::cpu::CPU;

impl CPU {
    pub fn stx_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 3;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        self.write_byte(addr, self.x, cycles);
        true
    }

    pub fn stx_zero_page_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.y) as u16;
        self.write_byte(addr, self.x, cycles);

        *cycles = cycles.wrapping_sub(1);
        true
    }

    pub fn stx_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        self.write_byte(addr, self.x, cycles);
        true
    }
}
