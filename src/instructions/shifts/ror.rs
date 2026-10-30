use crate::cpu::CPU;

impl CPU {
    /// ROR: right a bit through carry.
    pub fn ror(&mut self, value: u8) -> u8 {
        let result = value >> 1 | u8::from(self.status.carry) << 7;
        self.status.carry = value & 0x01 != 0;
        self.set_nz(result);
        result
    }
}
