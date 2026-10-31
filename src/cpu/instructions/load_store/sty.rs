use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub fn sty_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.write_byte(addr, self.y);
    }

    pub fn sty_zero_page_x(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        self.write_byte(addr, self.y);
    }

    pub fn sty_absolute(&mut self) {
        let addr = self.fetch_word();
        self.write_byte(addr, self.y);
    }
}
