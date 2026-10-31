use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// LAX: LDA and LDX at once, the byte into both.
    fn lax(&mut self, value: u8) {
        self.a = value;
        self.x = value;
        self.lda_set_status();
    }

    pub fn lax_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        let value = self.read_byte(addr);
        self.lax(value);
    }

    pub fn lax_zero_page_y(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.y);
        let value = self.read_byte(addr);
        self.lax(value);
    }

    pub fn lax_absolute(&mut self) {
        let addr = self.fetch_word();
        let value = self.read_byte(addr);
        self.lax(value);
    }

    pub fn lax_absolute_y(&mut self) {
        let addr = self.fetch_addr_absolute_indexed(self.y);
        let value = self.read_byte(addr);
        self.lax(value);
    }

    pub fn lax_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        let value = self.read_byte(addr);
        self.lax(value);
    }

    pub fn lax_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y();
        let value = self.read_byte(addr);
        self.lax(value);
    }
}
