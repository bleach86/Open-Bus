use crate::cpu::CPU;

impl CPU {
    /// LSR: right a bit, bit 0 to carry.
    pub fn lsr(&mut self, value: u8) -> u8 {
        self.status.carry = value & 0x01 != 0;
        let result = value >> 1;
        self.set_nz(result);
        result
    }
}
