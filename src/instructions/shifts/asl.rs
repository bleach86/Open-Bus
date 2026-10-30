use crate::cpu::CPU;

impl CPU {
    /// ASL: left a bit, bit 7 to carry.
    pub fn asl(&mut self, value: u8) -> u8 {
        self.status.carry = value & 0x80 != 0;
        let result = value << 1;
        self.set_nz(result);
        result
    }
}
