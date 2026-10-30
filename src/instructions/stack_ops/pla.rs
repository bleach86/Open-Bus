use crate::cpu::CPU;

impl CPU {
    pub fn pla(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 4;
        if *cycles < cycles_required - 1 {
            return false;
        }

        self.a = self.pop_byte(cycles);
        self.status.zero = self.a == 0;
        self.status.negative = (self.a & 0x80) != 0;
        true
    }
}
