use crate::cpu::CPU;

impl CPU {
    pub fn bit_zero_page(&mut self, cycles: &mut u32) {
        let addr = self.fetch_byte(cycles) as u16;
        let value = self.read_byte(addr, cycles);

        self.status.zero = (self.a & value) == 0;
        self.status.overflow = (value & 0x40) != 0;
        self.status.negative = (value & 0x80) != 0;
    }

    pub fn bit_absolute(&mut self, cycles: &mut u32) {
        let addr = self.fetch_word(cycles);
        let value = self.read_byte(addr, cycles);

        self.status.zero = (self.a & value) == 0;
        self.status.overflow = (value & 0x40) != 0;
        self.status.negative = (value & 0x80) != 0;
    }
}
