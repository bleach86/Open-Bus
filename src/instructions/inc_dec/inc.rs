use crate::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    fn inc_set_flags(&mut self, value: u8) {
        self.status.zero = value == 0;
        self.status.negative = (value & 0x80) != 0;
    }

    pub fn inc_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        let mut value = self.read_byte(addr);
        self.write_byte(addr, value); // written back first, as it was
        value = value.wrapping_add(1);
        self.write_byte(addr, value);


        self.inc_set_flags(value);
    }

    pub fn inc_zero_page_x(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        let mut value = self.read_byte(addr);
        self.write_byte(addr, value); // written back first, as it was
        value = value.wrapping_add(1);
        self.write_byte(addr, value);

        // Takes 2 clock cycles

        self.inc_set_flags(value);
    }

    pub fn inc_absolute(&mut self) {
        let addr = self.fetch_word();
        let mut value = self.read_byte(addr);
        self.write_byte(addr, value); // written back first, as it was
        value = value.wrapping_add(1);
        self.write_byte(addr, value);


        self.inc_set_flags(value);
    }

    pub fn inc_absolute_x(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.x as u16);
        self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF)); // before the carry
        let mut value = self.read_byte(addr);
        self.write_byte(addr, value); // written back first, as it was
        value = value.wrapping_add(1);
        self.write_byte(addr, value);

        // Takes 2 clock cycles

        self.inc_set_flags(value);
    }
}
