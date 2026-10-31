use crate::cpu::CPU;

impl CPU {
    pub fn ldx_set_status(&mut self) {
        self.status.zero = self.x == 0;
        self.status.negative = (self.x & 0x80) != 0;
    }

    pub fn ldx_immediate(&mut self) {
        let value = self.fetch_byte();
        self.x = value;
        self.ldx_set_status();
    }

    pub fn ldx_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        let value = self.read_byte(addr);
        self.x = value;

        self.ldx_set_status();
    }

    pub fn ldx_zero_page_y(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.y) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        let value = self.read_byte(addr);

        self.x = value;

        self.ldx_set_status();
    }

    pub fn ldx_absolute(&mut self) {
        let addr = self.fetch_word();
        let value = self.read_byte(addr);
        self.x = value;

        self.ldx_set_status();
    }

    pub fn ldx_absolute_y(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.y as u16);
        // Additional cycle if page crossed: the address is read first with
        // the high byte not yet carried into
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF));
        }
        let value = self.read_byte(addr);
        self.x = value;

        self.ldx_set_status();
    }
}
