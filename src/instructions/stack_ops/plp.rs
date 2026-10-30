use crate::cpu::CPU;

impl CPU {
    pub fn plp(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 4;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let status = self.pop_byte(cycles);
        self.status.from_byte(status);
        true
    }
}
