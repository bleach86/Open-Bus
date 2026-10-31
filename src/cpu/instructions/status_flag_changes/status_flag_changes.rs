use crate::cpu::bus::Bus;
use crate::cpu::{CPU, StatusFlag};

impl<B: Bus> CPU<B> {
    pub fn mod_flag(&mut self, flag: StatusFlag, value: bool) {
        self.read_byte(self.pc); // the next byte, read and thrown away

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
