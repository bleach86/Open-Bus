use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// DCP: DEC the byte, then CMP A with it; the flags CMP's.
    fn dcp(&mut self, addr: u16) {
        let mut value = self.read_byte(addr);
        self.modify_cycle(addr, value);
        value = value.wrapping_sub(1);
        self.write_byte(addr, value);
        self.cmp_set_flags(self.a.wrapping_sub(value), value);
    }

    pub fn dcp_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.dcp(addr);
    }

    pub fn dcp_zero_page_x(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.x);
        self.dcp(addr);
    }

    pub fn dcp_absolute(&mut self) {
        let addr = self.fetch_word();
        self.dcp(addr);
    }

    pub fn dcp_absolute_x(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.x);
        self.dcp(addr);
    }

    pub fn dcp_absolute_y(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.y);
        self.dcp(addr);
    }

    pub fn dcp_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        self.dcp(addr);
    }

    pub fn dcp_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y_for_write();
        self.dcp(addr);
    }
}
