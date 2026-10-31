use crate::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub fn bit_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        let value = self.read_byte(addr);

        self.status.zero = (self.a & value) == 0;
        self.status.overflow = (value & 0x40) != 0;
        self.status.negative = (value & 0x80) != 0;
    }

    pub fn bit_absolute(&mut self) {
        let addr = self.fetch_word();
        let value = self.read_byte(addr);

        self.status.zero = (self.a & value) == 0;
        self.status.overflow = (value & 0x40) != 0;
        self.status.negative = (value & 0x80) != 0;
    }
}
