use crate::cpu::CPU;

impl CPU {
    pub fn dey(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        self.y = self.y.wrapping_sub(1);
        self.status.zero = self.y == 0;
        self.status.negative = (self.y & 0x80) != 0;

        self.read_next_ignored(cycles);

        true
    }
}
