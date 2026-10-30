use crate::cpu::CPU;

impl CPU {
    /// INC, INX, INY: one more.
    pub fn inc(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        self.set_nz(result);
        result
    }
}
