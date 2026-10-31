use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    /// The 65C02's TSB and TRB: Z from A AND the byte, as BIT's; then A's
    /// bits set in it (TSB), or reset (TRB); read, read again, written.
    fn test_bits(&mut self, addr: u16, set: bool) {
        let value = self.read_byte(addr);
        self.modify_cycle(addr, value);
        self.status.zero = (self.a & value) == 0;
        let result = if set { value | self.a } else { value & !self.a };
        self.write_byte(addr, result);
    }

    pub fn tsb_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.test_bits(addr, true);
    }

    pub fn tsb_absolute(&mut self) {
        let addr = self.fetch_word();
        self.test_bits(addr, true);
    }

    pub fn trb_zero_page(&mut self) {
        let addr = self.fetch_byte() as u16;
        self.test_bits(addr, false);
    }

    pub fn trb_absolute(&mut self) {
        let addr = self.fetch_word();
        self.test_bits(addr, false);
    }
}
