use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    // The 65C02's STZ: a 0 stored.

    pub fn stz_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.write_byte(addr, 0);
    }

    pub fn stz_zero_page_x(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.x);
        self.write_byte(addr, 0);
    }

    pub fn stz_absolute(&mut self) {
        let addr = self.fetch_word();
        self.write_byte(addr, 0);
    }

    pub fn stz_absolute_x(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.x);
        self.write_byte(addr, 0);
    }
}
