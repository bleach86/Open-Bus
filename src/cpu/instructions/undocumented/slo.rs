use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// SLO: ASL the byte, then ORA it into A; C from the shift, N and Z
    /// from A.
    fn slo(&mut self, addr: u16) {
        let value = self.asl_memory(addr);
        self.a |= value;
        self.ora_set_flags();
    }

    pub fn slo_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.slo(addr);
    }

    pub fn slo_zero_page_x(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.x);
        self.slo(addr);
    }

    pub fn slo_absolute(&mut self) {
        let addr = self.fetch_word();
        self.slo(addr);
    }

    pub fn slo_absolute_x(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.x);
        self.slo(addr);
    }

    pub fn slo_absolute_y(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.y);
        self.slo(addr);
    }

    pub fn slo_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        self.slo(addr);
    }

    pub fn slo_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y_for_write();
        self.slo(addr);
    }
}
