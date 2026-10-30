use crate::cpu::CPU;

impl CPU {
    /// DEC, DEX, DEY: one less.
    pub fn dec(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        self.set_nz(result);
        result
    }
}
