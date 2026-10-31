use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// RLA: ROL the byte, then AND it into A; C from the rotate, N and Z
    /// from A.
    fn rla(&mut self, addr: u16) {
        let value = self.rol_memory(addr);
        self.a &= value;
        self.and_set_flags();
    }

    pub fn rla_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.rla(addr);
    }

    pub fn rla_zero_page_x(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.x);
        self.rla(addr);
    }

    pub fn rla_absolute(&mut self) {
        let addr = self.fetch_word();
        self.rla(addr);
    }

    pub fn rla_absolute_x(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.x);
        self.rla(addr);
    }

    pub fn rla_absolute_y(&mut self) {
        let addr = self.fetch_addr_absolute_indexed_for_write(self.y);
        self.rla(addr);
    }

    pub fn rla_indirect_x(&mut self) {
        let addr = self.fetch_addr_indirect_x();
        self.rla(addr);
    }

    pub fn rla_indirect_y(&mut self) {
        let addr = self.fetch_addr_indirect_y_for_write();
        self.rla(addr);
    }
}
