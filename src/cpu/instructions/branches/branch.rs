use crate::cpu::bus::Bus;
use crate::cpu::instructions::op_codes::Opcode;
use crate::cpu::{CPU, StatusFlag};

const CHECK_SET: [Opcode; 4] = [Opcode::BCS, Opcode::BEQ, Opcode::BMI, Opcode::BVS];

impl<B: Bus> CPU<B> {
    pub fn branch(&mut self, status_flag: StatusFlag, op_code: Opcode) {
        let offset: i8 = self.fetch_byte() as i8;

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
            self.take_branch(offset);
        }
    }

    /// A branch taken, by `offset` from the PC, past its operand.
    pub(crate) fn take_branch(&mut self, offset: i8) {
        let original_pc: u16 = self.pc;
        self.read_byte(original_pc); // the next opcode, read and thrown away
        self.pc = (self.pc as i32 + offset as i32) as u16;

        // Another page: the target is read first with the old high byte
        if (self.pc & 0xFF00) != (original_pc & 0xFF00) {
            self.read_byte((original_pc & 0xFF00) | (self.pc & 0x00FF));
        } else if !self.cmos() {
            // On the 6502, not: an IRQ come in its last cycle isn't seen
            // till the next instruction's end.
            self.irq_seen = self.irq_before;
        }
    }

    /// The 65C02's BRA: a branch always taken.
    pub fn bra(&mut self) {
        let offset = self.fetch_byte() as i8;
        self.take_branch(offset);
    }
}
