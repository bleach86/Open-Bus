use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub fn sta_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.write_byte(addr, self.a);
    }

    pub fn sta_zero_page_x(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        self.write_byte(addr, self.a);
    }

    pub fn sta_absolute(&mut self) {
        let addr = self.fetch_word();
        self.write_byte(addr, self.a);
    }

    pub fn sta_absolute_x(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.x as u16);

        self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF)); // before the carry

        self.write_byte(addr, self.a);
    }

    pub fn sta_absolute_y(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.y as u16);

        self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF)); // before the carry

        self.write_byte(addr, self.a);
    }

    pub fn sta_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();


        self.write_byte(addr, self.a);
    }

    pub fn sta_indirect_y(&mut self) {
        let base_addr = self.fetch_byte();
        let ptr = base_addr;
        // The pointer's high byte wraps around in the zero page.
        let lo = self.read_byte(ptr as u16) as u16;
        let hi = self.read_byte(ptr.wrapping_add(1) as u16) as u16;
        let base_addr = (hi << 8) | lo;
        let addr = base_addr.wrapping_add(self.y as u16);

        self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF)); // before the carry

        self.write_byte(addr, self.a);
    }
}
