use crate::cpu::{CPU, StatusFlag};

impl CPU {
    pub fn mod_flag(&mut self, flag: StatusFlag, value: bool, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        *cycles = cycles.wrapping_sub(1);

        match flag {
            StatusFlag::Carry => self.status.carry = value,
            StatusFlag::Zero => self.status.zero = value,
            StatusFlag::InterruptDisable => self.status.interrupt_disable = value,
            StatusFlag::DecimalMode => self.status.decimal_mode = value,
            StatusFlag::BreakCommand => self.status.break_command = value,
            StatusFlag::Overflow => self.status.overflow = value,
            StatusFlag::Negative => self.status.negative = value,
        }

        true
    }
}
