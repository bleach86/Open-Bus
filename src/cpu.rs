use crate::instructions::op_codes::Opcode;
use crate::ram::Memory;
use std::io::Write;
use std::{
    thread::sleep,
    time::{Duration, Instant},
};

use std::sync::atomic::AtomicBool;

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

/// Result of executing one instruction.
pub enum Flow {
    Continue,
    /// An instruction that could not run in the cycles left.
    Stop,
    Illegal(u8),
}

pub enum Registers {
    A,
    X,
    Y,
}

#[derive(Default, Debug, Clone)]
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

const NS_PER_CYCLE: u64 = 125; // 8 MHz
const PACE_INTERVAL_CYCLES: u64 = 16_000; // check every 2 ms of emulated time
const CYCLES_PER_FRAME: u64 = 8_000_000 / 60; // vblank at 60 Hz


#[derive(Default)]
pub struct CPU {
    pub pc: u16,                // Program Counter
    pub sp: u8,                 // Stack Pointer (8-bit, page $0100)
    pub a: u8,                  // Accumulator
    pub x: u8,                  // X Register
    pub y: u8,                  // Y Register
    pub status: StatusRegister, // Status Register
    pub memory: Memory,         // Memory
}

impl CPU {
    #[allow(dead_code)]
    pub fn new() -> Self {
        CPU::default()
    }

    /// Power on: system ROM mapped at `$C000-$FFFF`, the cart slot empty,
    /// registers cleared, PC from the reset vector at `$FFFC`.
    pub fn power_on(&mut self, rom: &[u8; crate::cart::ROM_SIZE]) {
        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.sp = 0xFD;
        self.status = StatusRegister::default();
        self.status.interrupt_disable = true;
        self.memory.power_on(rom);
        let mut cycles = u32::MAX;
        self.pc = self.read_word(0xFFFC, &mut cycles);
    }

    /// Run until the CPU stops; true if it stopped because the host asked for
    /// a reset.
    #[allow(dead_code)]
    pub fn execute(&mut self, cycles: u32) -> bool {
        self.execute_inner(cycles, true)
    }

    fn execute_inner(&mut self, mut cycles: u32, pace: bool) -> bool {
        const REPORT_CHECK_CYCLES: u64 = 1_000_000;

        let mut total_cycles: u64 = 0;
        let mut next_pace: u64 = PACE_INTERVAL_CYCLES;
        let mut next_frame: u64 = CYCLES_PER_FRAME;
        let mut executed: u64 = 0;
        let mut next_check: u64 = REPORT_CHECK_CYCLES;
        let mut start = Instant::now();
        let mut report_start = start;
        let mut report_executed: u64 = 0;

        while cycles > 0 {
            let cycle_start = cycles;

            // The interrupt lines, sampled between instructions as the 6502
            // does: a chip raising IRQ (with the I flag clear) or NMI sends
            // the CPU through $FFFE or $FFFA, the return address and the
            // status (B clear) on the stack, in 7 cycles. The bus learns from
            // the cycle within the frame where the video chip's beam is.
            let frame_cycle = total_cycles + CYCLES_PER_FRAME - next_frame;
            if let Some(vector) = self.memory.interrupt(frame_cycle, self.status.interrupt_disable) {
                // Two cycles reading at PC, as BRK's opcode and padding.
                self.read_byte(self.pc, &mut cycles);
                self.read_byte(self.pc, &mut cycles);
                self.push_word(self.pc, &mut cycles);
                self.status.break_command = false;
                let status = self.status.to_byte();
                self.push_byte(status, &mut cycles);
                self.status.interrupt_disable = true;
                self.pc = self.read_word(vector, &mut cycles);
            }

            // The monitor (monitor.rs), when one is attached: it may stop the
            // CPU here, before the instruction at PC, and have it until told
            // to go on. The pacing then starts again from where the CPU is,
            // rather than racing to make up the time stopped.
            if self.memory.monitor_wants(self.pc) {
                crate::monitor::session(self);
                start = Instant::now() - Duration::from_nanos(total_cycles * NS_PER_CYCLE);
            }

            let opcode = self.fetch_byte(&mut cycles);

            match self.exec_opcode(opcode, &mut cycles) {
                Flow::Continue => {}
                Flow::Stop => break,
                Flow::Illegal(op) => {
                    println!("Unimplemented opcode: ${:02X}", op);
                    break;
                }
            }

            let used = cycle_start.wrapping_sub(cycles) as u64;
            total_cycles += used;
            executed += used;

            if !pace && executed >= next_check {
                next_check = executed + REPORT_CHECK_CYCLES;
                let elapsed = report_start.elapsed();
                if elapsed >= Duration::from_secs(1) {
                    let mhz = (executed - report_executed) as f64 / elapsed.as_secs_f64() / 1e6;
                    eprintln!("{mhz:.1} MHz");
                    report_start = Instant::now();
                    report_executed = executed;
                }
            }

            // Frame wait halts the CPU until the next vblank, or until an
            // interrupt a chip has coming sooner (the video chip's raster
            // line), as a 65C02's WAI does.
            if self.memory.take_frame_wait() {
                let frame_start = next_frame - CYCLES_PER_FRAME;
                let wake = self
                    .memory
                    .next_interrupt(total_cycles - frame_start)
                    .map_or(next_frame, |at| (frame_start + at).min(next_frame));
                total_cycles = total_cycles.max(wake);
            }
            if total_cycles >= next_frame {
                next_frame += CYCLES_PER_FRAME;
                self.memory.end_frame();
                // A host call hands control back to whatever started the
                // emulator (on the RG351MP, the launcher that runs ES-DE).
                if let Some(status) = self.memory.take_host_call() {
                    self.memory.keep_save();
                    std::process::exit(status.into());
                }
                if self.memory.take_reset() {
                    return true;
                }
            }

            if pace && total_cycles >= next_pace {
                next_pace = total_cycles + PACE_INTERVAL_CYCLES;
                let target = Duration::from_nanos(total_cycles * NS_PER_CYCLE);
                let elapsed = start.elapsed();
                if target > elapsed {
                    // Sleeping can overshoot by tens of microseconds, but the
                    // target is absolute, so the next interval makes up for
                    // it: no busy-wait, and the average rate stays exact.
                    sleep(target - elapsed);
                }
            }

            cycles = u32::MAX;
        }
        false
    }

    /// Execute a single, already-fetched opcode.
    pub(crate) fn exec_opcode(&mut self, opcode: u8, cycles: &mut u32) -> Flow {
        match Opcode::from_byte(opcode) {
            Some(Opcode::ADCImmediate) => {
                if !self.adc_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ADCZeroPage) => {
                if !self.adc_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ADCZeroPageX) => {
                if !self.adc_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ADCAbsolute) => {
                if !self.adc_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ADCAbsoluteX) => {
                if !self.adc_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ADCAbsoluteY) => {
                if !self.adc_absolute_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ADCIndirectX) => {
                if !self.adc_indirect_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ADCIndirectY) => {
                if !self.adc_indirect_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ANDImmediate) => {
                if !self.and_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ANDZeroPage) => {
                if !self.and_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ANDZeroPageX) => {
                if !self.and_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ANDAbsolute) => {
                if !self.and_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ANDAbsoluteX) => {
                if !self.and_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ANDAbsoluteY) => {
                if !self.and_absolute_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ANDIndirectX) => {
                if !self.and_indirect_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ANDIndirectY) => {
                if !self.and_indirect_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ASLAccumulator) => {
                if !self.asl_accumulator(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ASLZeroPage) => {
                if !self.asl_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ASLZeroPageX) => {
                if !self.asl_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ASLAbsolute) => {
                if !self.asl_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ASLAbsoluteX) => {
                if !self.asl_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BCC) => {
                if !self.branch(cycles, StatusFlag::Carry, Opcode::BCC) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BCS) => {
                if !self.branch(cycles, StatusFlag::Carry, Opcode::BCS) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BEQ) => {
                if !self.branch(cycles, StatusFlag::Zero, Opcode::BEQ) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BMI) => {
                if !self.branch(cycles, StatusFlag::Negative, Opcode::BMI) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BNE) => {
                if !self.branch(cycles, StatusFlag::Zero, Opcode::BNE) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BPL) => {
                if !self.branch(cycles, StatusFlag::Negative, Opcode::BPL) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BVC) => {
                if !self.branch(cycles, StatusFlag::Overflow, Opcode::BVC) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BVS) => {
                if !self.branch(cycles, StatusFlag::Overflow, Opcode::BVS) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BITZeroPage) => {
                if !self.bit_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BITAbsolute) => {
                if !self.bit_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CLC) => {
                if !self.mod_flag(StatusFlag::Carry, false, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CLD) => {
                if !self.mod_flag(StatusFlag::DecimalMode, false, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CLI) => {
                if !self.mod_flag(StatusFlag::InterruptDisable, false, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CLV) => {
                if !self.mod_flag(StatusFlag::Overflow, false, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CMPImmediate) => {
                if !self.cmp_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CMPZeroPage) => {
                if !self.cmp_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CMPZeroPageX) => {
                if !self.cmp_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CMPAbsolute) => {
                if !self.cmp_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CMPAbsoluteX) => {
                if !self.cmp_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CMPAbsoluteY) => {
                if !self.cmp_absolute_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CMPIndirectX) => {
                if !self.cmp_indirect_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CMPIndirectY) => {
                if !self.cmp_indirect_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CPXImmediate) => {
                if !self.cpx_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CPXZeroPage) => {
                if !self.cpx_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CPXAbsolute) => {
                if !self.cpx_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CPYImmediate) => {
                if !self.cpy_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CPYZeroPage) => {
                if !self.cpy_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::CPYAbsolute) => {
                if !self.cpy_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::DEX) => {
                if !self.dex(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::DEY) => {
                if !self.dey(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::DECZeroPage) => {
                if !self.dec_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::DECZeroPageX) => {
                if !self.dec_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::DECAbsolute) => {
                if !self.dec_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::DECAbsoluteX) => {
                if !self.dec_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::EORImmediate) => {
                if !self.eor_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::EORZeroPage) => {
                if !self.eor_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::EORZeroPageX) => {
                if !self.eor_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::EORAbsolute) => {
                if !self.eor_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::EORAbsoluteX) => {
                if !self.eor_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::EORAbsoluteY) => {
                if !self.eor_absolute_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::EORIndirectX) => {
                if !self.eor_indirect_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::EORIndirectY) => {
                if !self.eor_indirect_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::JMPAbsolute) => {
                if !self.jmp_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::JMPIndirect) => {
                if !self.jmp_indirect(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::INX) => {
                if !self.inx(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::INY) => {
                if !self.iny(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::INCZeroPage) => {
                if !self.inc_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::INCZeroPageX) => {
                if !self.inc_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::INCAbsolute) => {
                if !self.inc_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::INCAbsoluteX) => {
                if !self.inc_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDAImmediate) => {
                if !self.lda_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDAZeroPage) => {
                if !self.lda_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDAZeroPageX) => {
                if !self.lda_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDAAbsolute) => {
                if !self.lda_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDAAbsoluteX) => {
                if !self.lda_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDAAbsoluteY) => {
                if !self.lda_absolute_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDAIndirectX) => {
                if !self.lda_indirect_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDAIndirectY) => {
                if !self.lda_indirect_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDXImmediate) => {
                if !self.ldx_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDXZeroPage) => {
                if !self.ldx_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDXZeroPageY) => {
                if !self.ldx_zero_page_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDXAbsolute) => {
                if !self.ldx_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDXAbsoluteY) => {
                if !self.ldx_absolute_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDYImmediate) => {
                if !self.ldy_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDYZeroPage) => {
                if !self.ldy_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDYZeroPageX) => {
                if !self.ldy_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDYAbsolute) => {
                if !self.ldy_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LDYAbsoluteX) => {
                if !self.ldy_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LSRAccumulator) => {
                if !self.lsr_accumulator(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LSRZeroPage) => {
                if !self.lsr_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LSRZeroPageX) => {
                if !self.lsr_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LSRAbsolute) => {
                if !self.lsr_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::LSRAbsoluteX) => {
                if !self.lsr_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::PHA) => {
                if !self.pha(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::PHP) => {
                if !self.php(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::PLA) => {
                if !self.pla(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::PLP) => {
                if !self.plp(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ORAbsoluteY) => {
                if !self.or_absolute_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ORIndirectX) => {
                if !self.or_indirect_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ORIndirectY) => {
                if !self.or_indirect_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ORImmediate) => {
                if !self.or_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ORZeroPage) => {
                if !self.or_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ORZeroPageX) => {
                if !self.or_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ORAbsolute) => {
                if !self.or_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ORAbsoluteX) => {
                if !self.or_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SBCImmediate) => {
                if !self.sbc_immediate(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SBCZeroPage) => {
                if !self.sbc_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SBCZeroPageX) => {
                if !self.sbc_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SBCAbsolute) => {
                if !self.sbc_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SBCAbsoluteX) => {
                if !self.sbc_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SBCAbsoluteY) => {
                if !self.sbc_absolute_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SBCIndirectX) => {
                if !self.sbc_indirect_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SBCIndirectY) => {
                if !self.sbc_indirect_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SEC) => {
                if !self.mod_flag(StatusFlag::Carry, true, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SED) => {
                if !self.mod_flag(StatusFlag::DecimalMode, true, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::SEI) => {
                if !self.mod_flag(StatusFlag::InterruptDisable, true, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STAZeroPage) => {
                if !self.sta_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STAZeroPageX) => {
                if !self.sta_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STAAbsolute) => {
                if !self.sta_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STAAbsoluteX) => {
                if !self.sta_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STAAbsoluteY) => {
                if !self.sta_absolute_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STAIndirectX) => {
                if !self.sta_indirect_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STAIndirectY) => {
                if !self.sta_indirect_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STXZeroPage) => {
                if !self.stx_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STXZeroPageY) => {
                if !self.stx_zero_page_y(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STXAbsolute) => {
                if !self.stx_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STYZeroPage) => {
                if !self.sty_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STYZeroPageX) => {
                if !self.sty_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::STYAbsolute) => {
                if !self.sty_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::TAX) => {
                if !self.transfer_register(Registers::A, Registers::X, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::TAY) => {
                if !self.transfer_register(Registers::A, Registers::Y, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::TXA) => {
                if !self.transfer_register(Registers::X, Registers::A, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::TYA) => {
                if !self.transfer_register(Registers::Y, Registers::A, cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::TSX) => {
                if !self.tsx(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::TXS) => {
                if !self.txs(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::JSRAbsolute) => {
                if !self.jsr_absolute(cycles) {
                    return Flow::Stop;
                }
                ////println!("JSR ${:04X}", sub_addr);
            }
            Some(Opcode::ROLAccumulator) => {
                if !self.rol_accumulator(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ROLZeroPage) => {
                if !self.rol_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ROLZeroPageX) => {
                if !self.rol_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ROLAbsolute) => {
                if !self.rol_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::ROLAbsoluteX) => {
                if !self.rol_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::RORAccumulator) => {
                if !self.ror_accumulator(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::RORZeroPage) => {
                if !self.ror_zero_page(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::RORZeroPageX) => {
                if !self.ror_zero_page_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::RORAbsolute) => {
                if !self.ror_absolute(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::RORAbsoluteX) => {
                if !self.ror_absolute_x(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::RTS) => {
                if !self.rts(cycles) {
                    return Flow::Stop;
                }

                ////println!("RTS to ${:04X}", self.pc);
            }
            Some(Opcode::RTI) => {
                if !self.rti(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::NOP) => {
                if !self.nop(cycles) {
                    return Flow::Stop;
                }
            }
            Some(Opcode::BRK) => {
                if !self.brk(cycles) {
                    return Flow::Stop;
                }
            }
            None => return Flow::Illegal(opcode),
        }
        Flow::Continue
    }

    pub fn sp_to_addr(&self) -> u16 {
        0x0100 | self.sp as u16
    }

    pub fn read_byte(&mut self, addr: u16, cycles: &mut u32) -> u8 {
        *cycles = cycles.saturating_sub(1);
        self.memory.read(addr)
    }

    pub fn write_byte(&mut self, addr: u16, value: u8, cycles: &mut u32) {
        *cycles = cycles.saturating_sub(1);
        self.memory.write(addr, value);
    }

    /// Read a 16-bit pointer stored in the zero page. The high byte comes
    /// from `(ptr + 1) & $FF`, so a pointer at `$FF` wraps to `$00`.
    pub fn read_zero_page_word(&mut self, ptr: u8, cycles: &mut u32) -> u16 {
        let low = self.read_byte(ptr as u16, cycles) as u16;
        let high = self.read_byte(ptr.wrapping_add(1) as u16, cycles) as u16;
        (high << 8) | low
    }

    pub fn read_word(&mut self, addr: u16, cycles: &mut u32) -> u16 {
        let low = self.read_byte(addr, cycles) as u16;
        let high = self.read_byte(addr.wrapping_add(1), cycles) as u16;
        (high << 8) | low
    }

    pub fn fetch_addr_indirect_x(&mut self, cycles: &mut u32) -> u16 {
        let zp_addr = self.fetch_byte(cycles);
        self.read_byte(zp_addr as u16, cycles); // read while X is added
        let effective_addr_lo = zp_addr.wrapping_add(self.x);
        let lo = self.read_byte(effective_addr_lo as u16, cycles) as u16;
        let hi = self.read_byte((effective_addr_lo.wrapping_add(1) & 0x00FF) as u16, cycles) as u16;
        (hi << 8) | lo
    }

    /// (ind),Y's address; `store` for a store, which always takes the
    /// index cycle (index_cycle).
    pub fn fetch_addr_indirect_y(&mut self, store: bool, cycles: &mut u32) -> u16 {
        let zp_addr = self.fetch_byte(cycles);
        let lo = self.read_byte(zp_addr as u16, cycles) as u16;
        let hi = self.read_byte((zp_addr.wrapping_add(1) & 0x00FF) as u16, cycles) as u16;
        let base = (hi << 8) | lo;
        let addr = base.wrapping_add(self.y as u16);
        self.index_cycle(base, addr, store, cycles);
        addr
    }

    /// The cycle a one-byte instruction spends reading the byte after it,
    /// which it ignores. (A 6502 reads or writes every cycle.)
    pub fn read_next_ignored(&mut self, cycles: &mut u32) {
        self.read_byte(self.pc, cycles);
    }

    /// The cycle a 6502 spends when an index carries into the high byte:
    /// a read at the address with the high byte not yet fixed. A read takes
    /// it only when the page is crossed; a store or a change in place
    /// always (`always`).
    pub fn index_cycle(&mut self, base: u16, addr: u16, always: bool, cycles: &mut u32) {
        if always || (base & 0xFF00) != (addr & 0xFF00) {
            self.read_byte((base & 0xFF00) | (addr & 0x00FF), cycles);
        }
    }

    pub fn fetch_byte(&mut self, cycles: &mut u32) -> u8 {
        let byte = self.memory.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        *cycles = cycles.saturating_sub(1);

        byte
    }

    pub fn fetch_word(&mut self, cycles: &mut u32) -> u16 {
        let low = self.fetch_byte(cycles) as u16;
        let high = self.fetch_byte(cycles) as u16;
        (high << 8) | low
    }

    pub fn push_byte(&mut self, value: u8, cycles: &mut u32) {
        let addr = self.sp_to_addr();
        self.memory.write(addr, value);
        self.sp = self.sp.wrapping_sub(1); // decrement SP after push
        *cycles = cycles.saturating_sub(1);
    }

    pub fn pop_byte(&mut self, cycles: &mut u32) -> u8 {
        self.sp = self.sp.wrapping_add(1); // increment SP before pop
        let addr = self.sp_to_addr();
        let value = self.memory.read(addr);
        *cycles = cycles.saturating_sub(1);
        value
    }

    pub fn push_word(&mut self, value: u16, cycles: &mut u32) {
        let low = (value & 0xFF) as u8;
        let high = (value >> 8) as u8;

        self.push_byte(high, cycles);
        self.push_byte(low, cycles);
    }
    pub fn pop_word(&mut self, cycles: &mut u32) -> u16 {
        let low = self.pop_byte(cycles) as u16;
        let high = self.pop_byte(cycles) as u16;
        (high << 8) | low
    }
}
