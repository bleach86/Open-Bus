use crate::cpu::{CPU, StatusFlag};

impl CPU {
    pub fn mod_flag(&mut self, flag: StatusFlag, value: bool, cycles: &mut u32) {
        *cycles = cycles.wrapping_add(1);

        match flag {
            StatusFlag::Carry => self.status.carry = value,
            StatusFlag::Zero => self.status.zero = value,
            StatusFlag::InterruptDisable => self.status.interrupt_disable = value,
            StatusFlag::DecimalMode => self.status.decimal_mode = value,
            StatusFlag::BreakCommand => self.status.break_command = value,
            StatusFlag::Overflow => self.status.overflow = value,
            StatusFlag::Negative => self.status.negative = value,
        }
    }
}
