use crate::cpu::CPU;

impl CPU {
    pub fn sta_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 3;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        self.write_byte(addr, self.a, cycles);
        true
    }

    pub fn sta_zero_page_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.write_byte(addr, self.a, cycles);

        *cycles = cycles.wrapping_sub(1);
        true
    }

    pub fn sta_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        self.write_byte(addr, self.a, cycles);
        true
    }

    pub fn sta_absolute_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles).wrapping_add(self.x as u16);
        self.write_byte(addr, self.a, cycles);
        true
    }

    pub fn sta_absolute_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles).wrapping_add(self.y as u16);
        self.write_byte(addr, self.a, cycles);
        true
    }

    pub fn sta_indirect_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        let ptr = base_addr.wrapping_add(self.x);
        let addr = self.read_word(ptr as u16, cycles);

        *cycles = cycles.wrapping_sub(1);

        self.write_byte(addr, self.a, cycles);
        true
    }

    pub fn sta_indirect_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        let ptr = base_addr;
        let base_addr = self.read_word(ptr as u16, cycles);
        let addr = base_addr.wrapping_add(self.y as u16);

        *cycles = cycles.wrapping_sub(1);

        self.write_byte(addr, self.a, cycles);
        true
    }
}
