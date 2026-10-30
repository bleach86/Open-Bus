use crate::cpu::CPU;

impl CPU {
    /// PHA, PHP: a value pushed, after the cycle every one-byte
    /// instruction spends reading the next byte.
    pub fn push(&mut self, value: u8, cycles: &mut u32) {
        self.implied(cycles);
        self.push_byte(value, cycles);
    }

    /// PLA, PLP: a value pulled, after a cycle reading the next byte and
    /// one at the stack as it stands.
    pub fn pull(&mut self, cycles: &mut u32) -> u8 {
        self.implied(cycles);
        self.read_byte(self.stack_address(), cycles);
        self.pop_byte(cycles)
    }
}
