use crate::cpu::CPU;

impl CPU {
    /// AND, ORA, EOR: A with a value, bit by bit.
    pub fn and(&mut self, value: u8) {
        self.a &= value;
        self.set_nz(self.a);
    }

    pub fn ora(&mut self, value: u8) {
        self.a |= value;
        self.set_nz(self.a);
    }

    pub fn eor(&mut self, value: u8) {
        self.a ^= value;
        self.set_nz(self.a);
    }

    /// BIT: Z from A & value; N and V from the value's bits 7 and 6.
    pub fn bit(&mut self, value: u8) {
        self.status.zero = self.a & value == 0;
        self.status.overflow = value & 0x40 != 0;
        self.status.negative = value & 0x80 != 0;
    }
}
