use crate::cpu::CPU;

impl CPU {
    /// LDA, LDX, LDY, and the transfers: a register given a value, N and Z
    /// set from it.
    pub fn load(&mut self, value: u8) -> u8 {
        self.set_nz(value);
        value
    }
}
