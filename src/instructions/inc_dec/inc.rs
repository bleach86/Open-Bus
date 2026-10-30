use crate::cpu::CPU;

impl CPU {
    fn inc_set_flags(&mut self, value: u8) {
        self.status.zero = value == 0;
        self.status.negative = (value & 0x80) != 0;
    }

    pub fn inc_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        let mut value = self.read_byte(addr, cycles);
        value = value.wrapping_add(1);
        self.write_byte(addr, value, cycles);

        *cycles = cycles.wrapping_add(1);

        self.inc_set_flags(value);
    }

    pub fn inc_zero_page_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_byte(cycles);
        let addr = base_addr.wrapping_add(self.x) as u16;
        let mut value = self.read_byte(addr, cycles);
        value = value.wrapping_add(1);
        self.write_byte(addr, value, cycles);

        *cycles = cycles.wrapping_add(2);

        self.inc_set_flags(value);
    }

    pub fn inc_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        let mut value = self.read_byte(addr, cycles);
        value = value.wrapping_add(1);
        self.write_byte(addr, value, cycles);

        *cycles = cycles.wrapping_add(1);

        self.inc_set_flags(value);
    }

    pub fn inc_absolute_x(&mut self, cycles: &mut u32) {
        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.x as u16);
        let mut value = self.read_byte(addr, cycles);
        value = value.wrapping_add(1);
        self.write_byte(addr, value, cycles);

        *cycles = cycles.wrapping_add(2);

        self.inc_set_flags(value);
    }
}
