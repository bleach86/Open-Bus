use crate::cpu::CPU;

impl CPU {
    pub fn ldx_set_status(&mut self) {
        self.status.zero = self.x == 0;
        self.status.negative = (self.x & 0x80) != 0;
    }

    pub fn ldx_immediate(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let value = self.fetch_byte(cycles);
        self.x = value;
        self.ldx_set_status();
        true
    }

    pub fn ldx_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 3;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        let value = self.read_byte(addr, cycles);
        self.x = value;

        self.ldx_set_status();
        true
    }

    pub fn ldx_zero_page_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        self.read_byte(base_addr as u16, cycles); // read while Y is added
        let addr = base_addr.wrapping_add(self.y) as u16;
        let value = self.read_byte(addr, cycles);
        self.x = value;

        self.ldx_set_status();
        true
    }

    pub fn ldx_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        let value = self.read_byte(addr, cycles);
        self.x = value;

        self.ldx_set_status();
        true
    }

    pub fn ldx_absolute_y(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 4;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.y as u16);
        self.index_cycle(base_addr, addr, false, cycles);
        let value = self.read_byte(addr, cycles);
        self.x = value;

        self.ldx_set_status();
        true
    }
}
