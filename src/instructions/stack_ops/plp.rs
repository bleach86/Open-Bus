use crate::cpu::CPU;

impl CPU {
    pub fn plp(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 4;
        if *cycles < cycles_required - 1 {
            return false;
        }

        self.read_next_ignored(cycles);
        self.read_byte(self.sp_to_addr(), cycles); // the stack as it stands, read while S moves
        let status = self.pop_byte(cycles);
        self.status.from_byte(status);
        true
    }
}
