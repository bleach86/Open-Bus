use crate::cpu::CPU;

impl CPU {
    fn dec_set_flags(&mut self, value: u8) {
        self.status.zero = value == 0;
        self.status.negative = (value & 0x80) != 0;
    }

    pub fn dec_zero_page(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 5;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_byte(cycles) as u16;
        let mut value = self.read_byte(addr, cycles);
        self.write_byte(addr, value, cycles); // written back while it changes
        value = value.wrapping_sub(1);
        self.write_byte(addr, value, cycles);

        self.dec_set_flags(value);
        true
    }

    pub fn dec_zero_page_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_byte(cycles);
        self.read_byte(base_addr as u16, cycles); // read while X is added
        let addr = base_addr.wrapping_add(self.x) as u16;
        let mut value = self.read_byte(addr, cycles);
        self.write_byte(addr, value, cycles); // written back while it changes
        value = value.wrapping_sub(1);
        self.write_byte(addr, value, cycles);

        self.dec_set_flags(value);
        true
    }

    pub fn dec_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);
        let mut value = self.read_byte(addr, cycles);
        self.write_byte(addr, value, cycles); // written back while it changes
        value = value.wrapping_sub(1);
        self.write_byte(addr, value, cycles);

        self.dec_set_flags(value);
        true
    }

    pub fn dec_absolute_x(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 7;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let base_addr = self.fetch_word(cycles);
        let addr = base_addr.wrapping_add(self.x as u16);
        self.index_cycle(base_addr, addr, true, cycles);
        let mut value = self.read_byte(addr, cycles);
        self.write_byte(addr, value, cycles); // written back while it changes
        value = value.wrapping_sub(1);
        self.write_byte(addr, value, cycles);

        self.dec_set_flags(value);
        true
    }
}
