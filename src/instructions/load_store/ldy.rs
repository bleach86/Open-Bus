use crate::cpu::CPU;

impl CPU {
    pub fn ldy_set_status(&mut self) {
        self.status.zero = self.y == 0;
        self.status.negative = (self.y & 0x80) != 0;
    }

    pub fn ldy_immediate(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let value = self.fetch_byte(cycles);
        self.y = value;
        self.ldy_set_status();
        true
    }

    pub fn ldy_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 3;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        let value = self.read_byte(addr, cycles);
        self.y = value;

        self.ldy_set_status();
        true
    }

    pub fn ldy_zero_page_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.x) as u16;
        let value = self.read_byte(addr, cycles);

        *cycles = cycles.wrapping_sub(1);
        self.y = value;

        self.ldy_set_status();
        true
    }

    pub fn ldy_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        let value = self.read_byte(addr, cycles);
        self.y = value;

        self.ldy_set_status();
        true
    }

    pub fn ldy_absolute_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.x as u16);
        let value = self.read_byte(addr, cycles);

        *cycles = cycles.wrapping_sub(1);
        self.y = value;

        self.ldy_set_status();
        true
    }
}
