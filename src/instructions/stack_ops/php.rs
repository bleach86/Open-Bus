use crate::cpu::CPU;

impl CPU {
    pub fn php(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 3;
        if *cycles < cycles_required - 1 {
            return false;
        }

        // PHP always pushes with the B flag (bit 4) set.
        let status = self.status.to_byte() | 0x10;

        self.read_next_ignored(cycles);
        self.push_byte(status, cycles);
        true
    }
}
