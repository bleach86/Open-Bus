use crate::cpu::{CPU, StatusFlag};
use crate::instructions::op_codes::Opcode;

const CYCLES_REQUIRED: u32 = 4;
const CHECK_SET: [Opcode; 4] = [Opcode::BCS, Opcode::BEQ, Opcode::BMI, Opcode::BVS];

impl CPU {
    pub fn branch(&mut self, cycles: &mut u32, status_flag: StatusFlag, op_code: Opcode) -> bool {
        if *cycles < CYCLES_REQUIRED - 1 {
            return false;
        }

        let offset: i8 = self.fetch_byte(cycles) as i8;
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
            self.pc = (self.pc as i32 + offset as i32) as u16;
            *cycles = cycles.wrapping_sub(1);

            if (self.pc & 0xFF00) != (original_pc & 0xFF00) {
                *cycles = cycles.wrapping_sub(1);
            }
        }

        true
    }
}
