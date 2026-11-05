pub mod bus;
pub mod instructions;

#[cfg(test)]
mod tests {
    mod bus;
    mod clock;
    mod dormann;
    mod harte;
    mod interrupts;
    mod wai_stp;
}

use crate::cpu::bus::Bus;
use crate::cpu::instructions::op_codes::Opcode;
use crate::machines::basic::Memory;
use serde::{Deserialize, Serialize};
use std::{
    thread::sleep,
    time::{Duration, Instant},
};

pub enum StatusFlag {
    Carry = 0,
    Zero = 1,
    InterruptDisable = 2,
    DecimalMode = 3,

    #[allow(dead_code)]
    BreakCommand = 4,
    Overflow = 6,
    Negative = 7,
}

pub enum Registers {
    A,
    X,
    Y,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct StatusRegister {
    pub carry: bool,
    pub zero: bool,
    pub interrupt_disable: bool,
    pub decimal_mode: bool,
    pub break_command: bool,
    pub overflow: bool,
    pub negative: bool,
}

impl StatusRegister {
    pub fn to_byte(&self) -> u8 {
        (self.carry as u8)
            | ((self.zero as u8) << 1)
            | ((self.interrupt_disable as u8) << 2)
            | ((self.decimal_mode as u8) << 3)
            | ((self.break_command as u8) << 4)
            | (1 << 5) // Unused bit, always set to 1
            | ((self.overflow as u8) << 6)
            | ((self.negative as u8) << 7)
    }

    pub fn from_byte(&mut self, value: u8) {
        self.carry = (value & 0x01) != 0;
        self.zero = (value & 0x02) != 0;
        self.interrupt_disable = (value & 0x04) != 0;
        self.decimal_mode = (value & 0x08) != 0;
        self.break_command = (value & 0x10) != 0;
        self.overflow = (value & 0x40) != 0;
        self.negative = (value & 0x80) != 0;
    }
}

const DEFAULT_CLOCK_HZ: u32 = 8_000_000; // 8 MHz
const PACES_PER_SECOND: u64 = 2_000; // check every 0.5 ms of emulated time

/// The 6502, wired to its bus: the machine around it. Plain CPU is this
/// machine's, its Memory.
/// Which of the 6502 family the CPU is: the NMOS 6502, its undocumented
/// opcodes and all; or a CMOS 65C02, the base one (Synertek's, as in the
/// enhanced Apple IIe), Rockwell's with its bit instructions, or WDC's with
/// those and WAI and STP. The 65C02s fix the 6502's bugs, make its unused
/// opcodes NOPs, and take their dummy cycles differently. Or the NES's
/// Ricoh 2A03: the NMOS 6502 with its decimal mode cut out, D set and
/// cleared, pushed and pulled, but its adds and subtracts binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Variant {
    #[default]
    Nmos6502,
    Synertek65C02,
    Rockwell65C02,
    Wdc65C02,
    Ricoh2A03,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CPU<B: Bus = Memory> {
    pub pc: u16,                // Program Counter
    pub sp: u8,                 // Stack Pointer (8-bit, page $0100)
    pub a: u8,                  // Accumulator
    pub x: u8,                  // X Register
    pub y: u8,                  // Y Register
    pub status: StatusRegister, // Status Register
    pub memory: B,              // Memory: what it's wired to
    pub irq: bool,              // IRQ line: held by a device until it's served
    pub nmi: bool,              // NMI: an edge, taken once
    nmi_line: bool,             // the machine's NMI line, as last sampled
    irq_seen: bool,             // its IRQ line, as last sampled: what's polled
    irq_before: bool,           // and as sampled the cycle before that
    in_reset: bool,             // held in reset: resets when RESET is let go
    jammed: Option<u8>,         // stopped by a JAM: its cycles since, up to 3
    waiting: bool,              // a WDC 65C02 at WAI, for an interrupt
    stopped: bool,              // a WDC 65C02 at STP, until reset
    halted: bool,               // the last read halted, for a DMA of the machine's
    pub clock: u32,             // CPU clock counter
    pub clock_hz: u32,          // its speed: cycles a second
    pub variant: Variant,       // which of the family it is
}

impl CPU {
    pub fn new() -> Self {
        CPU::with_bus(Memory::default())
    }
}

impl<B: Bus> CPU<B> {
    /// The CPU wired to `memory`, any machine's bus; at power-on.
    pub fn with_bus(memory: B) -> Self {
        CPU {
            pc: 0,
            sp: 0,
            a: 0,
            x: 0,
            y: 0,
            status: StatusRegister::default(),
            memory,
            irq: false,
            nmi: false,
            nmi_line: false,
            irq_seen: false,
            irq_before: false,
            in_reset: false,
            jammed: None,
            waiting: false,
            stopped: false,
            halted: false,
            clock: 0,
            clock_hz: DEFAULT_CLOCK_HZ,
            variant: Variant::Nmos6502,
        }
    }

    /// Whether it's a 65C02, of any make.
    pub fn cmos(&self) -> bool {
        matches!(
            self.variant,
            Variant::Synertek65C02 | Variant::Rockwell65C02 | Variant::Wdc65C02
        )
    }

    /// Whether its adds and subtracts are decimal: with D set, but not on
    /// a 2A03, which hasn't the circuit.
    pub fn decimal(&self) -> bool {
        self.status.decimal_mode && self.variant != Variant::Ricoh2A03
    }

    /// Reset, in the 6502's 7 cycles: as an interrupt, but with its three
    /// pushes made reads (S goes down 3, from 0 at power-on to $FD), then the
    /// reset vector. It sets I, and leaves A, X, Y and the other flags as
    /// they were: at power-on, 0.
    pub fn reset(&mut self) {
        self.read_byte(self.pc);
        self.read_byte(self.pc);
        for _ in 0..3 {
            self.read_byte(self.sp_to_addr());
            self.sp = self.sp.wrapping_sub(1);
        }
        self.pc = self.read_word(0xFFFC); // the reset vector

        self.status.interrupt_disable = true; // reset sets I, as on the 6502
        if self.cmos() {
            self.status.decimal_mode = false; // and on a 65C02 clears D
        }
        self.jammed = None; // the one way out of a JAM, and of STP
        self.waiting = false;
        self.stopped = false;
    }

    /// Run, until the machine wants the run loop handed back.
    pub fn execute_forever(&mut self) {
        let cycles = u32::MAX;

        self.execute_inner(cycles, true);
    }

    #[allow(dead_code)]
    pub fn execute(&mut self, cycles: u32) {
        self.execute_inner(cycles, false);
    }

    /// The run loop: an instruction at a time, for `cycles` cycles. As on
    /// the 6502, an instruction once fetched runs all its cycles: the last
    /// one can end past `cycles`.
    fn execute_inner(&mut self, cycles: u32, run_forever: bool) {
        let mut total_cycles: u64 = 0;
        let pace_interval = (self.clock_hz as u64 / PACES_PER_SECOND).max(1);
        let mut next_pace: u64 = pace_interval;
        let start = Instant::now();

        while run_forever || total_cycles < cycles as u64 {
            let cycle_start = self.clock;

            // RESET held: the 6502 only reads, a cycle at a time, until it's
            // let go; then it resets, and runs on from the reset vector.
            if self.memory.reset_held() {
                self.read_byte(self.pc);
                self.in_reset = true;
            } else if self.in_reset {
                self.in_reset = false;
                self.reset();
            } else if self.jammed.is_some() {
                self.jammed_cycle();
            } else if self.stopped {
                self.advance_clock(); // its clock going, nothing on its bus
            } else if self.waiting {
                self.wait_cycle();
            } else {
                self.step();
            }

            // The machine wants it back, between instructions.
            if self.memory.hand_back() {
                break;
            }

            let used = self.clock.wrapping_sub(cycle_start) as u64;
            total_cycles += used;

            if total_cycles >= next_pace {
                next_pace = total_cycles + pace_interval;
                // When these cycles are due at clock_hz; exact, at any speed.
                let target_ns = total_cycles as u128 * 1_000_000_000 / self.clock_hz as u128;
                let target = Duration::from_nanos(target_ns as u64);
                let elapsed = start.elapsed();
                // Ahead: sleep it off. The target is from the start, so a late
                // wake is made up by the next chunk.
                if target > elapsed {
                    sleep(target - elapsed);
                }
            }
        }
    }

    /// A cycle at WAI: still waiting, with nothing on the bus; or, an
    /// interrupt come, on: an NMI taken; an IRQ taken unless I is set, and
    /// then the next instruction.
    fn wait_cycle(&mut self) {
        let irq = self.irq || self.memory.irq();
        if self.nmi {
            self.waiting = false;
            self.nmi = false;
            self.interrupt(0xFFFA);
        } else if irq {
            self.waiting = false;
            if !self.status.interrupt_disable {
                self.interrupt(0xFFFE);
            }
        } else {
            self.advance_clock();
        }
    }

    /// An instruction, and the interrupt after it, if one is due.
    fn step(&mut self) {
        let i_before = self.status.interrupt_disable;

        let opcode = self.fetch_byte();

        self.exec_opcode(opcode);

        // A JAM stopped it dead: it doesn't get as far as an interrupt.
        if self.jammed.is_some() {
            return;
        }

        // The 6502 looks at IRQ before an instruction's last cycle: the I
        // that CLI, SEI and PLP set on that cycle counts from the next. BRK,
        // an interrupt's sequence, looks at none: its handler's first
        // instruction runs before another is taken.
        let masked = match Opcode::decode(opcode, self.variant) {
            Opcode::BRK => return,
            Opcode::CLI | Opcode::SEI | Opcode::PLP => i_before,
            _ => self.status.interrupt_disable,
        };

        // Interrupts are taken between instructions, as the lines were
        // the cycle before the last; NMI first.
        if self.nmi {
            self.nmi = false;
            self.interrupt(0xFFFA);
        } else if self.irq_seen && !masked {
            self.interrupt(0xFFFE);
        }
    }

    /// One instruction, already fetched: what execute_inner's loop does next.
    pub(crate) fn exec_opcode(&mut self, opcode: u8) {
        match Opcode::decode(opcode, self.variant) {
            Opcode::ADCImmediate => {
                self.adc_immediate();
            }
            Opcode::ADCZeroPage => {
                self.adc_zero_page();
            }
            Opcode::ADCZeroPageX => {
                self.adc_zero_page_x();
            }
            Opcode::ADCAbsolute => {
                self.adc_absolute();
            }
            Opcode::ADCAbsoluteX => {
                self.adc_absolute_x();
            }
            Opcode::ADCAbsoluteY => {
                self.adc_absolute_y();
            }
            Opcode::ADCIndirectX => {
                self.adc_indirect_x();
            }
            Opcode::ADCIndirectY => {
                self.adc_indirect_y();
            }
            Opcode::ANDImmediate => {
                self.and_immediate();
            }
            Opcode::ANDZeroPage => {
                self.and_zero_page();
            }
            Opcode::ANDZeroPageX => {
                self.and_zero_page_x();
            }
            Opcode::ANDAbsolute => {
                self.and_absolute();
            }
            Opcode::ANDAbsoluteX => {
                self.and_absolute_x();
            }
            Opcode::ANDAbsoluteY => {
                self.and_absolute_y();
            }
            Opcode::ANDIndirectX => {
                self.and_indirect_x();
            }
            Opcode::ANDIndirectY => {
                self.and_indirect_y();
            }
            Opcode::ASLAccumulator => {
                self.asl_accumulator();
            }
            Opcode::ASLZeroPage => {
                self.asl_zero_page();
            }
            Opcode::ASLZeroPageX => {
                self.asl_zero_page_x();
            }
            Opcode::ASLAbsolute => {
                self.asl_absolute();
            }
            Opcode::ASLAbsoluteX => {
                self.asl_absolute_x();
            }
            Opcode::BCC => {
                self.branch(StatusFlag::Carry, Opcode::BCC);
            }
            Opcode::BCS => {
                self.branch(StatusFlag::Carry, Opcode::BCS);
            }
            Opcode::BEQ => {
                self.branch(StatusFlag::Zero, Opcode::BEQ);
            }
            Opcode::BMI => {
                self.branch(StatusFlag::Negative, Opcode::BMI);
            }
            Opcode::BNE => {
                self.branch(StatusFlag::Zero, Opcode::BNE);
            }
            Opcode::BPL => {
                self.branch(StatusFlag::Negative, Opcode::BPL);
            }
            Opcode::BVC => {
                self.branch(StatusFlag::Overflow, Opcode::BVC);
            }
            Opcode::BVS => {
                self.branch(StatusFlag::Overflow, Opcode::BVS);
            }
            Opcode::BITZeroPage => {
                self.bit_zero_page();
            }
            Opcode::BITAbsolute => {
                self.bit_absolute();
            }
            Opcode::CLC => {
                self.mod_flag(StatusFlag::Carry, false);
            }
            Opcode::CLD => {
                self.mod_flag(StatusFlag::DecimalMode, false);
            }
            Opcode::CLI => {
                self.mod_flag(StatusFlag::InterruptDisable, false);
            }
            Opcode::CLV => {
                self.mod_flag(StatusFlag::Overflow, false);
            }
            Opcode::CMPImmediate => {
                self.cmp_immediate();
            }
            Opcode::CMPZeroPage => {
                self.cmp_zero_page();
            }
            Opcode::CMPZeroPageX => {
                self.cmp_zero_page_x();
            }
            Opcode::CMPAbsolute => {
                self.cmp_absolute();
            }
            Opcode::CMPAbsoluteX => {
                self.cmp_absolute_x();
            }
            Opcode::CMPAbsoluteY => {
                self.cmp_absolute_y();
            }
            Opcode::CMPIndirectX => {
                self.cmp_indirect_x();
            }
            Opcode::CMPIndirectY => {
                self.cmp_indirect_y();
            }
            Opcode::CPXImmediate => {
                self.cpx_immediate();
            }
            Opcode::CPXZeroPage => {
                self.cpx_zero_page();
            }
            Opcode::CPXAbsolute => {
                self.cpx_absolute();
            }
            Opcode::CPYImmediate => {
                self.cpy_immediate();
            }
            Opcode::CPYZeroPage => {
                self.cpy_zero_page();
            }
            Opcode::CPYAbsolute => {
                self.cpy_absolute();
            }
            Opcode::DEX => {
                self.dex();
            }
            Opcode::DEY => {
                self.dey();
            }
            Opcode::DECZeroPage => {
                self.dec_zero_page();
            }
            Opcode::DECZeroPageX => {
                self.dec_zero_page_x();
            }
            Opcode::DECAbsolute => {
                self.dec_absolute();
            }
            Opcode::DECAbsoluteX => {
                self.dec_absolute_x();
            }
            Opcode::EORImmediate => {
                self.eor_immediate();
            }
            Opcode::EORZeroPage => {
                self.eor_zero_page();
            }
            Opcode::EORZeroPageX => {
                self.eor_zero_page_x();
            }
            Opcode::EORAbsolute => {
                self.eor_absolute();
            }
            Opcode::EORAbsoluteX => {
                self.eor_absolute_x();
            }
            Opcode::EORAbsoluteY => {
                self.eor_absolute_y();
            }
            Opcode::EORIndirectX => {
                self.eor_indirect_x();
            }
            Opcode::EORIndirectY => {
                self.eor_indirect_y();
            }
            Opcode::JMPAbsolute => {
                self.jmp_absolute();
            }
            Opcode::JMPIndirect => {
                self.jmp_indirect();
            }
            Opcode::INX => {
                self.inx();
            }
            Opcode::INY => {
                self.iny();
            }
            Opcode::INCZeroPage => {
                self.inc_zero_page();
            }
            Opcode::INCZeroPageX => {
                self.inc_zero_page_x();
            }
            Opcode::INCAbsolute => {
                self.inc_absolute();
            }
            Opcode::INCAbsoluteX => {
                self.inc_absolute_x();
            }
            Opcode::LDAImmediate => {
                self.lda_immediate();
            }
            Opcode::LDAZeroPage => {
                self.lda_zero_page();
            }
            Opcode::LDAZeroPageX => {
                self.lda_zero_page_x();
            }
            Opcode::LDAAbsolute => {
                self.lda_absolute();
            }
            Opcode::LDAAbsoluteX => {
                self.lda_absolute_x();
            }
            Opcode::LDAAbsoluteY => {
                self.lda_absolute_y();
            }
            Opcode::LDAIndirectX => {
                self.lda_indirect_x();
            }
            Opcode::LDAIndirectY => {
                self.lda_indirect_y();
            }
            Opcode::LDXImmediate => {
                self.ldx_immediate();
            }
            Opcode::LDXZeroPage => {
                self.ldx_zero_page();
            }
            Opcode::LDXZeroPageY => {
                self.ldx_zero_page_y();
            }
            Opcode::LDXAbsolute => {
                self.ldx_absolute();
            }
            Opcode::LDXAbsoluteY => {
                self.ldx_absolute_y();
            }
            Opcode::LDYImmediate => {
                self.ldy_immediate();
            }
            Opcode::LDYZeroPage => {
                self.ldy_zero_page();
            }
            Opcode::LDYZeroPageX => {
                self.ldy_zero_page_x();
            }
            Opcode::LDYAbsolute => {
                self.ldy_absolute();
            }
            Opcode::LDYAbsoluteX => {
                self.ldy_absolute_x();
            }
            Opcode::LSRAccumulator => {
                self.lsr_accumulator();
            }
            Opcode::LSRZeroPage => {
                self.lsr_zero_page();
            }
            Opcode::LSRZeroPageX => {
                self.lsr_zero_page_x();
            }
            Opcode::LSRAbsolute => {
                self.lsr_absolute();
            }
            Opcode::LSRAbsoluteX => {
                self.lsr_absolute_x();
            }
            Opcode::PHA => {
                self.pha();
            }
            Opcode::PHP => {
                self.php();
            }
            Opcode::PLA => {
                self.pla();
            }
            Opcode::PLP => {
                self.plp();
            }
            Opcode::ORAbsoluteY => {
                self.or_absolute_y();
            }
            Opcode::ORIndirectX => {
                self.or_indirect_x();
            }
            Opcode::ORIndirectY => {
                self.or_indirect_y();
            }
            Opcode::ORImmediate => {
                self.or_immediate();
            }
            Opcode::ORZeroPage => {
                self.or_zero_page();
            }
            Opcode::ORZeroPageX => {
                self.or_zero_page_x();
            }
            Opcode::ORAbsolute => {
                self.or_absolute();
            }
            Opcode::ORAbsoluteX => {
                self.or_absolute_x();
            }
            Opcode::SBCImmediate => {
                self.sbc_immediate();
            }
            Opcode::SBCZeroPage => {
                self.sbc_zero_page();
            }
            Opcode::SBCZeroPageX => {
                self.sbc_zero_page_x();
            }
            Opcode::SBCAbsolute => {
                self.sbc_absolute();
            }
            Opcode::SBCAbsoluteX => {
                self.sbc_absolute_x();
            }
            Opcode::SBCAbsoluteY => {
                self.sbc_absolute_y();
            }
            Opcode::SBCIndirectX => {
                self.sbc_indirect_x();
            }
            Opcode::SBCIndirectY => {
                self.sbc_indirect_y();
            }
            Opcode::SEC => {
                self.mod_flag(StatusFlag::Carry, true);
            }
            Opcode::SED => {
                self.mod_flag(StatusFlag::DecimalMode, true);
            }
            Opcode::SEI => {
                self.mod_flag(StatusFlag::InterruptDisable, true);
            }
            Opcode::STAZeroPage => {
                self.sta_zero_page();
            }
            Opcode::STAZeroPageX => {
                self.sta_zero_page_x();
            }
            Opcode::STAAbsolute => {
                self.sta_absolute();
            }
            Opcode::STAAbsoluteX => {
                self.sta_absolute_x();
            }
            Opcode::STAAbsoluteY => {
                self.sta_absolute_y();
            }
            Opcode::STAIndirectX => {
                self.sta_indirect_x();
            }
            Opcode::STAIndirectY => {
                self.sta_indirect_y();
            }
            Opcode::STXZeroPage => {
                self.stx_zero_page();
            }
            Opcode::STXZeroPageY => {
                self.stx_zero_page_y();
            }
            Opcode::STXAbsolute => {
                self.stx_absolute();
            }
            Opcode::STYZeroPage => {
                self.sty_zero_page();
            }
            Opcode::STYZeroPageX => {
                self.sty_zero_page_x();
            }
            Opcode::STYAbsolute => {
                self.sty_absolute();
            }
            Opcode::TAX => {
                self.transfer_register(Registers::A, Registers::X);
            }
            Opcode::TAY => {
                self.transfer_register(Registers::A, Registers::Y);
            }
            Opcode::TXA => {
                self.transfer_register(Registers::X, Registers::A);
            }
            Opcode::TYA => {
                self.transfer_register(Registers::Y, Registers::A);
            }
            Opcode::TSX => {
                self.tsx();
            }
            Opcode::TXS => {
                self.txs();
            }
            Opcode::JSRAbsolute => {
                self.jsr_absolute();
            }
            Opcode::ROLAccumulator => {
                self.rol_accumulator();
            }
            Opcode::ROLZeroPage => {
                self.rol_zero_page();
            }
            Opcode::ROLZeroPageX => {
                self.rol_zero_page_x();
            }
            Opcode::ROLAbsolute => {
                self.rol_absolute();
            }
            Opcode::ROLAbsoluteX => {
                self.rol_absolute_x();
            }
            Opcode::RORAccumulator => {
                self.ror_accumulator();
            }
            Opcode::RORZeroPage => {
                self.ror_zero_page();
            }
            Opcode::RORZeroPageX => {
                self.ror_zero_page_x();
            }
            Opcode::RORAbsolute => {
                self.ror_absolute();
            }
            Opcode::RORAbsoluteX => {
                self.ror_absolute_x();
            }
            Opcode::RTS => {
                self.rts();
            }
            Opcode::RTI => {
                self.rti();
            }
            Opcode::NOP => {
                self.nop();
            }
            Opcode::BRK => {
                self.brk();
            }
            Opcode::TSBZeroPage => {
                self.tsb_zero_page();
            }
            Opcode::TSBAbsolute => {
                self.tsb_absolute();
            }
            Opcode::TRBZeroPage => {
                self.trb_zero_page();
            }
            Opcode::TRBAbsolute => {
                self.trb_absolute();
            }
            Opcode::ORZeroPageIndirect => {
                self.or_zero_page_indirect();
            }
            Opcode::ANDZeroPageIndirect => {
                self.and_zero_page_indirect();
            }
            Opcode::EORZeroPageIndirect => {
                self.eor_zero_page_indirect();
            }
            Opcode::ADCZeroPageIndirect => {
                self.adc_zero_page_indirect();
            }
            Opcode::STAZeroPageIndirect => {
                self.sta_zero_page_indirect();
            }
            Opcode::LDAZeroPageIndirect => {
                self.lda_zero_page_indirect();
            }
            Opcode::CMPZeroPageIndirect => {
                self.cmp_zero_page_indirect();
            }
            Opcode::SBCZeroPageIndirect => {
                self.sbc_zero_page_indirect();
            }
            Opcode::INCAccumulator => {
                self.inc_accumulator();
            }
            Opcode::DECAccumulator => {
                self.dec_accumulator();
            }
            Opcode::BITZeroPageX => {
                self.bit_zero_page_x();
            }
            Opcode::BITAbsoluteX => {
                self.bit_absolute_x();
            }
            Opcode::BITImmediate => {
                self.bit_immediate();
            }
            Opcode::PHY => {
                self.phy();
            }
            Opcode::PLY => {
                self.ply();
            }
            Opcode::PHX => {
                self.phx();
            }
            Opcode::PLX => {
                self.plx();
            }
            Opcode::STZZeroPage => {
                self.stz_zero_page();
            }
            Opcode::STZZeroPageX => {
                self.stz_zero_page_x();
            }
            Opcode::STZAbsolute => {
                self.stz_absolute();
            }
            Opcode::STZAbsoluteX => {
                self.stz_absolute_x();
            }
            Opcode::JMPAbsoluteIndexedIndirect => {
                self.jmp_absolute_indexed_indirect();
            }
            Opcode::BRA => {
                self.bra();
            }
            Opcode::RMB(bit) => {
                self.rmb(bit);
            }
            Opcode::SMB(bit) => {
                self.smb(bit);
            }
            Opcode::BBR(bit) => {
                self.bbr(bit);
            }
            Opcode::BBS(bit) => {
                self.bbs(bit);
            }
            Opcode::WAI => {
                self.wai();
            }
            Opcode::STP => {
                self.stp();
            }
            Opcode::NOPOneByte => {} // its fetch, its one cycle
            Opcode::NOPThreeBytes => {
                self.nop_three_bytes();
            }
            Opcode::NOPThreeBytesShort => {
                self.fetch_word(); // the operand, read and thrown away
            }
            Opcode::SLOZeroPage => {
                self.slo_zero_page();
            }
            Opcode::SLOZeroPageX => {
                self.slo_zero_page_x();
            }
            Opcode::SLOAbsolute => {
                self.slo_absolute();
            }
            Opcode::SLOAbsoluteX => {
                self.slo_absolute_x();
            }
            Opcode::SLOAbsoluteY => {
                self.slo_absolute_y();
            }
            Opcode::SLOIndirectX => {
                self.slo_indirect_x();
            }
            Opcode::SLOIndirectY => {
                self.slo_indirect_y();
            }
            Opcode::RLAZeroPage => {
                self.rla_zero_page();
            }
            Opcode::RLAZeroPageX => {
                self.rla_zero_page_x();
            }
            Opcode::RLAAbsolute => {
                self.rla_absolute();
            }
            Opcode::RLAAbsoluteX => {
                self.rla_absolute_x();
            }
            Opcode::RLAAbsoluteY => {
                self.rla_absolute_y();
            }
            Opcode::RLAIndirectX => {
                self.rla_indirect_x();
            }
            Opcode::RLAIndirectY => {
                self.rla_indirect_y();
            }
            Opcode::SREZeroPage => {
                self.sre_zero_page();
            }
            Opcode::SREZeroPageX => {
                self.sre_zero_page_x();
            }
            Opcode::SREAbsolute => {
                self.sre_absolute();
            }
            Opcode::SREAbsoluteX => {
                self.sre_absolute_x();
            }
            Opcode::SREAbsoluteY => {
                self.sre_absolute_y();
            }
            Opcode::SREIndirectX => {
                self.sre_indirect_x();
            }
            Opcode::SREIndirectY => {
                self.sre_indirect_y();
            }
            Opcode::RRAZeroPage => {
                self.rra_zero_page();
            }
            Opcode::RRAZeroPageX => {
                self.rra_zero_page_x();
            }
            Opcode::RRAAbsolute => {
                self.rra_absolute();
            }
            Opcode::RRAAbsoluteX => {
                self.rra_absolute_x();
            }
            Opcode::RRAAbsoluteY => {
                self.rra_absolute_y();
            }
            Opcode::RRAIndirectX => {
                self.rra_indirect_x();
            }
            Opcode::RRAIndirectY => {
                self.rra_indirect_y();
            }
            Opcode::DCPZeroPage => {
                self.dcp_zero_page();
            }
            Opcode::DCPZeroPageX => {
                self.dcp_zero_page_x();
            }
            Opcode::DCPAbsolute => {
                self.dcp_absolute();
            }
            Opcode::DCPAbsoluteX => {
                self.dcp_absolute_x();
            }
            Opcode::DCPAbsoluteY => {
                self.dcp_absolute_y();
            }
            Opcode::DCPIndirectX => {
                self.dcp_indirect_x();
            }
            Opcode::DCPIndirectY => {
                self.dcp_indirect_y();
            }
            Opcode::ISCZeroPage => {
                self.isc_zero_page();
            }
            Opcode::ISCZeroPageX => {
                self.isc_zero_page_x();
            }
            Opcode::ISCAbsolute => {
                self.isc_absolute();
            }
            Opcode::ISCAbsoluteX => {
                self.isc_absolute_x();
            }
            Opcode::ISCAbsoluteY => {
                self.isc_absolute_y();
            }
            Opcode::ISCIndirectX => {
                self.isc_indirect_x();
            }
            Opcode::ISCIndirectY => {
                self.isc_indirect_y();
            }
            Opcode::LAXZeroPage => {
                self.lax_zero_page();
            }
            Opcode::LAXZeroPageY => {
                self.lax_zero_page_y();
            }
            Opcode::LAXAbsolute => {
                self.lax_absolute();
            }
            Opcode::LAXAbsoluteY => {
                self.lax_absolute_y();
            }
            Opcode::LAXIndirectX => {
                self.lax_indirect_x();
            }
            Opcode::LAXIndirectY => {
                self.lax_indirect_y();
            }
            Opcode::SAXZeroPage => {
                self.sax_zero_page();
            }
            Opcode::SAXZeroPageY => {
                self.sax_zero_page_y();
            }
            Opcode::SAXAbsolute => {
                self.sax_absolute();
            }
            Opcode::SAXIndirectX => {
                self.sax_indirect_x();
            }
            Opcode::NOPImmediate => {
                self.nop_immediate();
            }
            Opcode::NOPZeroPage => {
                self.nop_zero_page();
            }
            Opcode::NOPZeroPageX => {
                self.nop_zero_page_x();
            }
            Opcode::NOPAbsolute => {
                self.nop_absolute();
            }
            Opcode::NOPAbsoluteX => {
                self.nop_absolute_x();
            }
            Opcode::ANCImmediate => {
                self.anc_immediate();
            }
            Opcode::ALRImmediate => {
                self.alr_immediate();
            }
            Opcode::ARRImmediate => {
                self.arr_immediate();
            }
            Opcode::SBXImmediate => {
                self.sbx_immediate();
            }
            Opcode::ANEImmediate => {
                self.ane_immediate();
            }
            Opcode::LXAImmediate => {
                self.lxa_immediate();
            }
            Opcode::SHAAbsoluteY => {
                self.sha_absolute_y();
            }
            Opcode::SHAIndirectY => {
                self.sha_indirect_y();
            }
            Opcode::SHXAbsoluteY => {
                self.shx_absolute_y();
            }
            Opcode::SHYAbsoluteX => {
                self.shy_absolute_x();
            }
            Opcode::TASAbsoluteY => {
                self.tas_absolute_y();
            }
            Opcode::LASAbsoluteY => {
                self.las_absolute_y();
            }
            Opcode::JAM => {
                self.jam();
            }
        }
    }

    pub fn sp_to_addr(&self) -> u16 {
        0x0100 | self.sp as u16
    }

    /// A cycle: the clock, the machine's chips; and the interrupt lines
    /// sampled, NMI for its edge and IRQ for its level, as the machine has
    /// them at the end of the cycle before: the 6502 samples them at each
    /// cycle's end, and what an instruction's end polls is what they were
    /// the cycle before its last.
    pub fn advance_clock(&mut self) {
        self.clock = self.clock.wrapping_add(1);
        self.memory.tick();
        let nmi_line = self.memory.nmi();
        self.nmi |= nmi_line && !self.nmi_line;
        self.nmi_line = nmi_line;
        self.irq_before = self.irq_seen;
        self.irq_seen = self.irq || self.memory.irq();
    }

    /// A read, a cycle. Unless the machine holds RDY, for a DMA of its own:
    /// then this cycle's read is thrown away, the DMA has its cycles, and
    /// the read is made again on one of its own.
    pub fn read_byte(&mut self, addr: u16) -> u8 {
        self.advance_clock();
        self.halted = self.memory.halt(addr);
        if self.halted {
            self.memory.read(addr);
            while self.memory.halt(addr) {
                self.advance_clock();
            }
            self.advance_clock();
        }
        self.memory.read(addr)
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        self.advance_clock();
        self.memory.write(addr, value);
    }

    pub fn read_word(&mut self, addr: u16) -> u16 {
        let low = self.read_byte(addr) as u16;
        let high = self.read_byte(addr.wrapping_add(1)) as u16;
        (high << 8) | low
    }

    pub fn fetch_addr_indirect_x(&mut self) -> u16 {
        let zp_addr = self.fetch_byte();
        self.read_byte(zp_addr as u16); // the base, read before X is added
        let effective_addr_lo = zp_addr.wrapping_add(self.x);
        let lo = self.read_byte(effective_addr_lo as u16) as u16;
        let hi = self.read_byte((effective_addr_lo.wrapping_add(1) & 0x00FF) as u16) as u16;
        (hi << 8) | lo
    }

    pub fn fetch_addr_indirect_y(&mut self) -> u16 {
        let zp_addr = self.fetch_byte();
        let lo = self.read_byte(zp_addr as u16) as u16;
        let hi = self.read_byte((zp_addr.wrapping_add(1) & 0x00FF) as u16) as u16;
        let base = (hi << 8) | lo;
        let addr = base.wrapping_add(self.y as u16);

        self.read_if_page_crossed(base, addr);
        addr
    }

    /// The cycle an indexed address takes to carry into its high byte: the
    /// 6502 reads the address with its high byte not yet carried into; a
    /// 65C02, the instruction's last byte again.
    pub fn read_before_carry(&mut self, base_addr: u16, addr: u16) {
        if self.cmos() {
            self.read_byte(self.pc.wrapping_sub(1));
        } else {
            self.read_byte((base_addr & 0xFF00) | (addr & 0x00FF));
        }
    }

    /// For a read: that cycle only if a page is crossed.
    pub fn read_if_page_crossed(&mut self, base_addr: u16, addr: u16) {
        if (base_addr & 0xFF00) != (addr & 0xFF00) {
            self.read_before_carry(base_addr, addr);
        }
    }

    /// A read-modify-write's cycle between its read and its write: the
    /// 6502 writes the byte back as it was; a 65C02 reads it again.
    pub fn modify_cycle(&mut self, addr: u16, value: u8) {
        if self.cmos() {
            self.read_byte(addr);
        } else {
            self.write_byte(addr, value);
        }
    }

    /// (zp), the 65C02's: the address at a zero page pointer, its high byte
    /// wrapping round in the zero page.
    pub fn fetch_addr_zero_page_indirect(&mut self) -> u16 {
        let zp_addr = self.fetch_byte();
        let lo = self.read_byte(zp_addr as u16) as u16;
        let hi = self.read_byte(zp_addr.wrapping_add(1) as u16) as u16;
        (hi << 8) | lo
    }

    /// zp,X or zp,Y: the base read before the index is added, the sum
    /// wrapping in the zero page.
    pub fn fetch_addr_zero_page_indexed(&mut self, index: u8) -> u16 {
        let base_addr = self.fetch_byte();
        self.read_byte(base_addr as u16); // the base, read before the index is added
        base_addr.wrapping_add(index) as u16
    }

    /// abs,X or abs,Y, for a read: an extra cycle if a page is crossed, the
    /// address read first with the high byte not yet carried into.
    pub fn fetch_addr_absolute_indexed(&mut self, index: u8) -> u16 {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(index as u16);
        self.read_if_page_crossed(base_addr, addr);
        addr
    }

    /// abs,X or abs,Y, for a write or a read-modify-write: the address read
    /// before the carry always, crossed or not.
    pub fn fetch_addr_absolute_indexed_for_write(&mut self, index: u8) -> u16 {
        let base_addr = self.fetch_word();
        let addr = base_addr.wrapping_add(index as u16);
        self.read_before_carry(base_addr, addr);
        addr
    }

    /// (zp),Y, for a write or a read-modify-write: the address read before
    /// the carry always, crossed or not.
    pub fn fetch_addr_indirect_y_for_write(&mut self) -> u16 {
        let zp_addr = self.fetch_byte();
        let lo = self.read_byte(zp_addr as u16) as u16;
        let hi = self.read_byte(zp_addr.wrapping_add(1) as u16) as u16;
        let base = (hi << 8) | lo;
        let addr = base.wrapping_add(self.y as u16);
        self.read_before_carry(base, addr);
        addr
    }

    pub fn fetch_byte(&mut self) -> u8 {
        let byte = self.read_byte(self.pc);
        self.pc = self.pc.wrapping_add(1);

        byte
    }

    pub fn fetch_word(&mut self) -> u16 {
        let low = self.fetch_byte() as u16;
        let high = self.fetch_byte() as u16;
        (high << 8) | low
    }

    pub fn push_byte(&mut self, value: u8) {
        let addr = self.sp_to_addr();
        self.write_byte(addr, value);
        self.sp = self.sp.wrapping_sub(1); // decrement SP after push
    }

    pub fn pop_byte(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1); // increment SP before pop
        let addr = self.sp_to_addr();
        self.read_byte(addr)
    }

    pub fn push_word(&mut self, value: u16) {
        let low = (value & 0xFF) as u8;
        let high = (value >> 8) as u8;

        self.push_byte(high);
        self.push_byte(low);
    }
    pub fn pop_word(&mut self) -> u16 {
        let low = self.pop_byte() as u16;
        let high = self.pop_byte() as u16;
        (high << 8) | low
    }
}
