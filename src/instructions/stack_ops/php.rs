use crate::cpu::CPU;

impl CPU {
    pub fn php(&mut self, cycles: &mut u32) {
        // PHP always pushes with the B flag (bit 4) set.
        let status = self.status.to_byte() | 0x10;

        self.push_byte(status, cycles);
        *cycles = cycles.wrapping_add(1);
    }
}
