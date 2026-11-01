use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// RRA: ROR the byte, then ADC it to A, with the carry the rotate
    /// left; in decimal mode, as ADC is.
    fn rra(&mut self, addr: u16) {
        let value = self.ror_memory(addr);
        match self.decimal() {
            false => self.adc_binary(value),
            true => self.adc_decimal(value),
        }
    }

    pub fn rra_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.rra(addr);
    }

    pub fn rra_zero_page_x(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.x);
        self.rra(addr);
    }

    pub fn rra_absolute(&mut self) {
        let addr = self.fetch_word();
        self.rra(addr);
    }

    pub fn rra_absolute_x(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.x);
        self.rra(addr);
    }

    pub fn rra_absolute_y(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.y);
        self.rra(addr);
    }

    pub fn rra_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        self.rra(addr);
    }

    pub fn rra_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y_for_write();
        self.rra(addr);
    }
}
