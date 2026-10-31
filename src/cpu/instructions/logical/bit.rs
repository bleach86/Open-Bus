use crate::cpu::bus::Bus;
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

    /// The 65C02's BIT immediate: Z alone; N and V as they were.
    pub fn bit_immediate(&mut self) {
        let value = self.fetch_byte();
        self.status.zero = (self.a & value) == 0;
    }

    pub fn bit_zero_page_x(&mut self) {
        let addr = self.fetch_addr_zero_page_indexed(self.x);
        let value = self.read_byte(addr);

        self.status.zero = (self.a & value) == 0;
        self.status.overflow = (value & 0x40) != 0;
        self.status.negative = (value & 0x80) != 0;
    }

    pub fn bit_absolute_x(&mut self) {
        let addr = self.fetch_addr_absolute_indexed(self.x);
        let value = self.read_byte(addr);

        self.status.zero = (self.a & value) == 0;
        self.status.overflow = (value & 0x40) != 0;
        self.status.negative = (value & 0x80) != 0;
    }
}
