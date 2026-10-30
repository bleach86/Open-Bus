use crate::cpu::CPU;

impl CPU {
    /// CMP, CPX, CPY: `register` - value, setting N, Z and C only.
    pub fn compare(&mut self, register: u8, value: u8) {
        self.status.carry = register >= value;
        self.set_nz(register.wrapping_sub(value));
    }
}
