use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    // SAX: A AND X stored, the flags as they were.

    pub fn sax_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.write_byte(addr, self.a & self.x);
    }

    pub fn sax_zero_page_y(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.y);
        self.write_byte(addr, self.a & self.x);
    }

    pub fn sax_absolute(&mut self) {
        let addr = self.fetch_word();
        self.write_byte(addr, self.a & self.x);
    }

    pub fn sax_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        self.write_byte(addr, self.a & self.x);
    }
}
