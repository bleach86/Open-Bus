use crate::cpu::{CPU, Registers};

impl CPU {
    pub fn transfer_register(
        &mut self,
        source: Registers,
        destination: Registers,
        cycles: &mut u32,
    ) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let value = match source {
            Registers::A => self.a,
            Registers::X => self.x,
            Registers::Y => self.y,
        };

        match destination {
            Registers::A => self.a = value,
            Registers::X => self.x = value,
            Registers::Y => self.y = value,
        }

        let dest_value = match destination {
            Registers::A => self.a,
            Registers::X => self.x,
            Registers::Y => self.y,
        };

        self.status.zero = dest_value == 0;
        self.status.negative = (dest_value & 0x80) != 0;

        *cycles = cycles.wrapping_sub(1);
        true
    }
}
