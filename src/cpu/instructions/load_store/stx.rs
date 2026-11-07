use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    pub fn stx_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.write_byte(addr, self.x);
    }

    pub fn stx_zero_page_y(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.y) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        self.write_byte(addr, self.x);
    }

    pub fn stx_absolute(&mut self) {
        let addr = self.fetch_word();
        self.write_byte(addr, self.x);
    }
}
