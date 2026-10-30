use crate::cpu::CPU;

impl CPU {
    /// ROL: left a bit through carry.
    pub fn rol(&mut self, value: u8) -> u8 {
        let result = value << 1 | u8::from(self.status.carry);
        self.status.carry = value & 0x80 != 0;
        self.set_nz(result);
        result
    }
}
