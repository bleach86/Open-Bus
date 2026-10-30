use crate::cpu::CPU;

impl CPU {
    pub fn pha(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 3;
        if *cycles < cycles_required - 1 {
            return false;
        }

        self.read_next_ignored(cycles);
        self.push_byte(self.a, cycles);
        true
    }
}
