use crate::bus::Bus;
use crate::cpu::{CPU, StatusFlag};
use crate::instructions::op_codes::Opcode;

const CHECK_SET: [Opcode; 4] = [Opcode::BCS, Opcode::BEQ, Opcode::BMI, Opcode::BVS];

impl<B: Bus> CPU<B> {
    pub fn branch(&mut self, status_flag: StatusFlag, op_code: Opcode) {
        let offset: i8 = self.fetch_byte() as i8;
        let original_pc: u16 = self.pc;

        let flag_set = match status_flag {
            StatusFlag::Carry => self.status.carry,
            StatusFlag::Zero => self.status.zero,
            StatusFlag::Negative => self.status.negative,
            StatusFlag::Overflow => self.status.overflow,
            _ => false,
        };

        let should_branch = if CHECK_SET.contains(&op_code) {
            flag_set
        } else {
            !flag_set
        };

        if should_branch {
            self.read_byte(original_pc); // the next opcode, read and thrown away
            self.pc = (self.pc as i32 + offset as i32) as u16;

            // Another page: the target is read first with the old high byte
            if (self.pc & 0xFF00) != (original_pc & 0xFF00) {
                self.read_byte((original_pc & 0xFF00) | (self.pc & 0x00FF));
            }
        }
    }
}
