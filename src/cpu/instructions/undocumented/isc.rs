use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// ISC: INC the byte, then SBC it from A; in decimal mode, as SBC is.
    fn isc(&mut self, addr: u16) {
        let mut value = self.read_byte(addr);
        self.write_byte(addr, value); // written back first, as it was
        value = value.wrapping_add(1);
        self.write_byte(addr, value);
        match self.status.decimal_mode {
            false => self.sbc_binary(value),
            true => self.sbc_decimal(value),
        }
    }

    pub fn isc_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.isc(addr);
    }

    pub fn isc_zero_page_x(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.x);
        self.isc(addr);
    }

    pub fn isc_absolute(&mut self) {
        let addr = self.fetch_word();
        self.isc(addr);
    }

    pub fn isc_absolute_x(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.x);
        self.isc(addr);
    }

    pub fn isc_absolute_y(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.y);
        self.isc(addr);
    }

    pub fn isc_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        self.isc(addr);
    }

    pub fn isc_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y_for_write();
        self.isc(addr);
    }
}
