use crate::cpu::CPU;

impl CPU {
    pub fn tsx(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        self.x = self.sp;
        self.status.zero = self.x == 0;
        self.status.negative = (self.x & 0x80) != 0;

        self.read_next_ignored(cycles);
        true
    }
}
