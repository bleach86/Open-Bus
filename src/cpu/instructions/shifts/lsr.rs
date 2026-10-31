use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    fn lsr_set_flags(&mut self, original: u8, result: u8) {
        self.status.carry = (original & 0x01) != 0; // the bit shifted out
        self.status.zero = result == 0;
        self.status.negative = (result & 0x80) != 0;
    }

    /// The byte at `addr` shifted, as the 6502 does it: read, written back
    /// as it was, then written shifted; what it wrote.
    pub(crate) fn lsr_memory(&mut self, addr: u16) -> u8 {
        let original = self.read_byte(addr);
        self.write_byte(addr, original); // written back first, as it was
        let result = original.wrapping_shr(1);
        self.write_byte(addr, result);
        self.lsr_set_flags(original, result);
        result
    }

    pub fn lsr_accumulator(&mut self) {
        let original = self.a;
        self.a = original.wrapping_shr(1);
        self.lsr_set_flags(original, self.a);

        self.read_byte(self.pc); // the next byte, read and thrown away
    }

    pub fn lsr_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.lsr_memory(addr);
    }

    pub fn lsr_zero_page_x(&mut self) {
        let base_addr = self.fetch_byte();
        let addr = base_addr.wrapping_add(self.x) as u16;
        self.read_byte(base_addr as u16); // the base, read before the index is added
        self.lsr_memory(addr);
    }

    pub fn lsr_absolute(&mut self) {
        let addr = self.fetch_word();
        self.lsr_memory(addr);
    }

    pub fn lsr_absolute_x(&mut self) {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(self.x as u16);
        self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF)); // before the carry
        self.lsr_memory(addr);
    }
}
