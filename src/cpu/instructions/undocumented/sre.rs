use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// SRE: LSR the byte, then EOR it into A; C from the shift, N and Z
    /// from A.
    fn sre(&mut self, addr: u16) {
        let value = self.lsr_memory(addr);
        self.a ^= value;
        self.eor_set_flags();
    }

    pub fn sre_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.sre(addr);
    }

    pub fn sre_zero_page_x(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.x);
        self.sre(addr);
    }

    pub fn sre_absolute(&mut self) {
        let addr = self.fetch_word();
        self.sre(addr);
    }

    pub fn sre_absolute_x(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.x);
        self.sre(addr);
    }

    pub fn sre_absolute_y(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.y);
        self.sre(addr);
    }

    pub fn sre_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        self.sre(addr);
    }

    pub fn sre_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y_for_write();
        self.sre(addr);
    }
}
