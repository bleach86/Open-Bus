use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    fn rol_set_flags(&mut self, original: u8, result: u8) {
        self.status.carry = (original & 0x80) != 0; // the bit rotated out
        self.status.zero = result == 0;
        self.status.negative = (result & 0x80) != 0;
    }

    fn rol_memory(&mut self, addr: u16) {
        let original = self.read_byte(addr);
        self.write_byte(addr, original); // written back first, as it was
        let carry_in = if self.status.carry { 1 } else { 0 };
        let result = (original << 1) | carry_in;
        self.write_byte(addr, result);
        self.rol_set_flags(original, result);
    }

    pub fn rol_accumulator(&mut self) {
        let original = self.a;
        let carry_in = if self.status.carry { 1 } else { 0 };
        self.a = (original << 1) | carry_in;
        self.rol_set_flags(original, self.a);

        self.read_byte(self.pc); // the next byte, read and thrown away
    }

    pub fn rol_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.rol_memory(addr);
    }

    pub fn rol_zero_page_x(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        self.rol_memory(addr);
    }

    pub fn rol_absolute(&mut self) {
        let addr = self.fetch_word();
        self.rol_memory(addr);
    }

    pub fn rol_absolute_x(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.x as u16);
        self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF)); // before the carry
        self.rol_memory(addr);
    }
}
