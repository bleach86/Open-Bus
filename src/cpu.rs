use crate::bus::Bus;
use crate::instructions::op_codes::Opcode;
use crate::ram::Memory;
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

/// What an instruction leaves the run loop to do.
pub enum Flow {
    Continue,
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

const DEFAULT_CLOCK_HZ: u32 = 8_000_000; // 8 MHz
const PACES_PER_SECOND: u64 = 2_000; // check every 0.5 ms of emulated time

/// The 6502, wired to its bus: the machine around it. Plain CPU is this
/// machine's, its Memory.
#[derive(Clone)]
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
    pub clock: u32,             // CPU clock counter
    pub clock_hz: u32,          // its speed: cycles a second
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
            clock: 0,
            clock_hz: DEFAULT_CLOCK_HZ,
        }
    }

    /// Reset, in the 6502's 7 cycles: as an interrupt, but with its three
    /// pushes made reads (S goes down 3, from 0 at power-on to $FD), then the
    /// reset vector.
    pub fn reset(&mut self) {
        self.read_byte(self.pc);
        self.read_byte(self.pc);
        for _ in 0..3 {
            self.read_byte(self.sp_to_addr());
            self.sp = self.sp.wrapping_sub(1);
        }
        self.pc = self.read_word(0xFFFC); // the reset vector
        self.a = 0;
        self.x = 0;
        self.y = 0;

        self.status = StatusRegister::default();
        self.status.interrupt_disable = true; // reset sets I, as on the 6502
    }

    pub fn execute_forever(&mut self) {
        println!("PC set to {:04X}", self.pc);
        //std::process::exit(0);

        let cycles = u32::MAX;

        self.execute_inner(cycles, true);
    }

    pub fn execute(&mut self, cycles: u32) {
        println!("PC set to {:04X}", self.pc);
        //std::process::exit(0);

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
            let i_before = self.status.interrupt_disable;

            let opcode = self.fetch_byte();

            match self.exec_opcode(opcode) {
                Flow::Continue => {}
                Flow::Illegal(opcode) => {
                    println!("Unimplemented opcode: ${:02X}", opcode);
                    break;
                }
            }


            // The 6502 looks at IRQ before an instruction's last cycle: the I
            // that CLI, SEI and PLP set on that cycle counts from the next.
            let masked = match Opcode::from_byte(opcode) {
                Some(Opcode::CLI | Opcode::SEI | Opcode::PLP) => i_before,
                _ => self.status.interrupt_disable,
            };

            // Interrupts are taken between instructions; NMI first.
            if self.nmi {
                self.interrupt(0xFFFA);
                self.nmi = false;
            } else if self.irq && !masked {
                self.interrupt(0xFFFE);
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

    /// One instruction, already fetched: what execute_inner's loop does next.
    pub(crate) fn exec_opcode(&mut self, opcode: u8) -> Flow {
        match Opcode::from_byte(opcode) {
            Some(Opcode::ADCImmediate) => {
                self.adc_immediate();
            }
            Some(Opcode::ADCZeroPage) => {
                self.adc_zero_page();
            }
            Some(Opcode::ADCZeroPageX) => {
                self.adc_zero_page_x();
            }
            Some(Opcode::ADCAbsolute) => {
                self.adc_absolute();
            }
            Some(Opcode::ADCAbsoluteX) => {
                self.adc_absolute_x();
            }
            Some(Opcode::ADCAbsoluteY) => {
                self.adc_absolute_y();
            }
            Some(Opcode::ADCIndirectX) => {
                self.adc_indirect_x();
            }
            Some(Opcode::ADCIndirectY) => {
                self.adc_indirect_y();
            }
            Some(Opcode::ANDImmediate) => {
                self.and_immediate();
            }
            Some(Opcode::ANDZeroPage) => {
                self.and_zero_page();
            }
            Some(Opcode::ANDZeroPageX) => {
                self.and_zero_page_x();
            }
            Some(Opcode::ANDAbsolute) => {
                self.and_absolute();
            }
            Some(Opcode::ANDAbsoluteX) => {
                self.and_absolute_x();
            }
            Some(Opcode::ANDAbsoluteY) => {
                self.and_absolute_y();
            }
            Some(Opcode::ANDIndirectX) => {
                self.and_indirect_x();
            }
            Some(Opcode::ANDIndirectY) => {
                self.and_indirect_y();
            }
            Some(Opcode::ASLAccumulator) => {
                self.asl_accumulator();
            }
            Some(Opcode::ASLZeroPage) => {
                self.asl_zero_page();
            }
            Some(Opcode::ASLZeroPageX) => {
                self.asl_zero_page_x();
            }
            Some(Opcode::ASLAbsolute) => {
                self.asl_absolute();
            }
            Some(Opcode::ASLAbsoluteX) => {
                self.asl_absolute_x();
            }
            Some(Opcode::BCC) => {
                self.branch(StatusFlag::Carry, Opcode::BCC);
            }
            Some(Opcode::BCS) => {
                self.branch(StatusFlag::Carry, Opcode::BCS);
            }
            Some(Opcode::BEQ) => {
                self.branch(StatusFlag::Zero, Opcode::BEQ);
            }
            Some(Opcode::BMI) => {
                self.branch(StatusFlag::Negative, Opcode::BMI);
            }
            Some(Opcode::BNE) => {
                self.branch(StatusFlag::Zero, Opcode::BNE);
            }
            Some(Opcode::BPL) => {
                self.branch(StatusFlag::Negative, Opcode::BPL);
            }
            Some(Opcode::BVC) => {
                self.branch(StatusFlag::Overflow, Opcode::BVC);
            }
            Some(Opcode::BVS) => {
                self.branch(StatusFlag::Overflow, Opcode::BVS);
            }
            Some(Opcode::BITZeroPage) => {
                self.bit_zero_page();
            }
            Some(Opcode::BITAbsolute) => {
                self.bit_absolute();
            }
            Some(Opcode::CLC) => {
                self.mod_flag(StatusFlag::Carry, false);
            }
            Some(Opcode::CLD) => {
                self.mod_flag(StatusFlag::DecimalMode, false);
            }
            Some(Opcode::CLI) => {
                self.mod_flag(StatusFlag::InterruptDisable, false);
            }
            Some(Opcode::CLV) => {
                self.mod_flag(StatusFlag::Overflow, false);
            }
            Some(Opcode::CMPImmediate) => {
                self.cmp_immediate();
            }
            Some(Opcode::CMPZeroPage) => {
                self.cmp_zero_page();
            }
            Some(Opcode::CMPZeroPageX) => {
                self.cmp_zero_page_x();
            }
            Some(Opcode::CMPAbsolute) => {
                self.cmp_absolute();
            }
            Some(Opcode::CMPAbsoluteX) => {
                self.cmp_absolute_x();
            }
            Some(Opcode::CMPAbsoluteY) => {
                self.cmp_absolute_y();
            }
            Some(Opcode::CMPIndirectX) => {
                self.cmp_indirect_x();
            }
            Some(Opcode::CMPIndirectY) => {
                self.cmp_indirect_y();
            }
            Some(Opcode::CPXImmediate) => {
                self.cpx_immediate();
            }
            Some(Opcode::CPXZeroPage) => {
                self.cpx_zero_page();
            }
            Some(Opcode::CPXAbsolute) => {
                self.cpx_absolute();
            }
            Some(Opcode::CPYImmediate) => {
                self.cpy_immediate();
            }
            Some(Opcode::CPYZeroPage) => {
                self.cpy_zero_page();
            }
            Some(Opcode::CPYAbsolute) => {
                self.cpy_absolute();
            }
            Some(Opcode::DEX) => {
                self.dex();
            }
            Some(Opcode::DEY) => {
                self.dey();
            }
            Some(Opcode::DECZeroPage) => {
                self.dec_zero_page();
            }
            Some(Opcode::DECZeroPageX) => {
                self.dec_zero_page_x();
            }
            Some(Opcode::DECAbsolute) => {
                self.dec_absolute();
            }
            Some(Opcode::DECAbsoluteX) => {
                self.dec_absolute_x();
            }
            Some(Opcode::EORImmediate) => {
                self.eor_immediate();
            }
            Some(Opcode::EORZeroPage) => {
                self.eor_zero_page();
            }
            Some(Opcode::EORZeroPageX) => {
                self.eor_zero_page_x();
            }
            Some(Opcode::EORAbsolute) => {
                self.eor_absolute();
            }
            Some(Opcode::EORAbsoluteX) => {
                self.eor_absolute_x();
            }
            Some(Opcode::EORAbsoluteY) => {
                self.eor_absolute_y();
            }
            Some(Opcode::EORIndirectX) => {
                self.eor_indirect_x();
            }
            Some(Opcode::EORIndirectY) => {
                self.eor_indirect_y();
            }
            Some(Opcode::JMPAbsolute) => {
                self.jmp_absolute();
            }
            Some(Opcode::JMPIndirect) => {
                self.jmp_indirect();
            }
            Some(Opcode::INX) => {
                self.inx();
            }
            Some(Opcode::INY) => {
                self.iny();
            }
            Some(Opcode::INCZeroPage) => {
                self.inc_zero_page();
            }
            Some(Opcode::INCZeroPageX) => {
                self.inc_zero_page_x();
            }
            Some(Opcode::INCAbsolute) => {
                self.inc_absolute();
            }
            Some(Opcode::INCAbsoluteX) => {
                self.inc_absolute_x();
            }
            Some(Opcode::LDAImmediate) => {
                self.lda_immediate();
            }
            Some(Opcode::LDAZeroPage) => {
                self.lda_zero_page();
            }
            Some(Opcode::LDAZeroPageX) => {
                self.lda_zero_page_x();
            }
            Some(Opcode::LDAAbsolute) => {
                self.lda_absolute();
            }
            Some(Opcode::LDAAbsoluteX) => {
                self.lda_absolute_x();
            }
            Some(Opcode::LDAAbsoluteY) => {
                self.lda_absolute_y();
            }
            Some(Opcode::LDAIndirectX) => {
                self.lda_indirect_x();
            }
            Some(Opcode::LDAIndirectY) => {
                self.lda_indirect_y();
            }
            Some(Opcode::LDXImmediate) => {
                self.ldx_immediate();
            }
            Some(Opcode::LDXZeroPage) => {
                self.ldx_zero_page();
            }
            Some(Opcode::LDXZeroPageY) => {
                self.ldx_zero_page_y();
            }
            Some(Opcode::LDXAbsolute) => {
                self.ldx_absolute();
            }
            Some(Opcode::LDXAbsoluteY) => {
                self.ldx_absolute_y();
            }
            Some(Opcode::LDYImmediate) => {
                self.ldy_immediate();
            }
            Some(Opcode::LDYZeroPage) => {
                self.ldy_zero_page();
            }
            Some(Opcode::LDYZeroPageX) => {
                self.ldy_zero_page_x();
            }
            Some(Opcode::LDYAbsolute) => {
                self.ldy_absolute();
            }
            Some(Opcode::LDYAbsoluteX) => {
                self.ldy_absolute_x();
            }
            Some(Opcode::LSRAccumulator) => {
                self.lsr_accumulator();
            }
            Some(Opcode::LSRZeroPage) => {
                self.lsr_zero_page();
            }
            Some(Opcode::LSRZeroPageX) => {
                self.lsr_zero_page_x();
            }
            Some(Opcode::LSRAbsolute) => {
                self.lsr_absolute();
            }
            Some(Opcode::LSRAbsoluteX) => {
                self.lsr_absolute_x();
            }
            Some(Opcode::PHA) => {
                self.pha();
            }
            Some(Opcode::PHP) => {
                self.php();
            }
            Some(Opcode::PLA) => {
                self.pla();
            }
            Some(Opcode::PLP) => {
                self.plp();
            }
            Some(Opcode::ORAbsoluteY) => {
                self.or_absolute_y();
            }
            Some(Opcode::ORIndirectX) => {
                self.or_indirect_x();
            }
            Some(Opcode::ORIndirectY) => {
                self.or_indirect_y();
            }
            Some(Opcode::ORImmediate) => {
                self.or_immediate();
            }
            Some(Opcode::ORZeroPage) => {
                self.or_zero_page();
            }
            Some(Opcode::ORZeroPageX) => {
                self.or_zero_page_x();
            }
            Some(Opcode::ORAbsolute) => {
                self.or_absolute();
            }
            Some(Opcode::ORAbsoluteX) => {
                self.or_absolute_x();
            }
            Some(Opcode::SBCImmediate) => {
                self.sbc_immediate();
            }
            Some(Opcode::SBCZeroPage) => {
                self.sbc_zero_page();
            }
            Some(Opcode::SBCZeroPageX) => {
                self.sbc_zero_page_x();
            }
            Some(Opcode::SBCAbsolute) => {
                self.sbc_absolute();
            }
            Some(Opcode::SBCAbsoluteX) => {
                self.sbc_absolute_x();
            }
            Some(Opcode::SBCAbsoluteY) => {
                self.sbc_absolute_y();
            }
            Some(Opcode::SBCIndirectX) => {
                self.sbc_indirect_x();
            }
            Some(Opcode::SBCIndirectY) => {
                self.sbc_indirect_y();
            }
            Some(Opcode::SEC) => {
                self.mod_flag(StatusFlag::Carry, true);
            }
            Some(Opcode::SED) => {
                self.mod_flag(StatusFlag::DecimalMode, true);
            }
            Some(Opcode::SEI) => {
                self.mod_flag(StatusFlag::InterruptDisable, true);
            }
            Some(Opcode::STAZeroPage) => {
                self.sta_zero_page();
            }
            Some(Opcode::STAZeroPageX) => {
                self.sta_zero_page_x();
            }
            Some(Opcode::STAAbsolute) => {
                self.sta_absolute();
            }
            Some(Opcode::STAAbsoluteX) => {
                self.sta_absolute_x();
            }
            Some(Opcode::STAAbsoluteY) => {
                self.sta_absolute_y();
            }
            Some(Opcode::STAIndirectX) => {
                self.sta_indirect_x();
            }
            Some(Opcode::STAIndirectY) => {
                self.sta_indirect_y();
            }
            Some(Opcode::STXZeroPage) => {
                self.stx_zero_page();
            }
            Some(Opcode::STXZeroPageY) => {
                self.stx_zero_page_y();
            }
            Some(Opcode::STXAbsolute) => {
                self.stx_absolute();
            }
            Some(Opcode::STYZeroPage) => {
                self.sty_zero_page();
            }
            Some(Opcode::STYZeroPageX) => {
                self.sty_zero_page_x();
            }
            Some(Opcode::STYAbsolute) => {
                self.sty_absolute();
            }
            Some(Opcode::TAX) => {
                self.transfer_register(Registers::A, Registers::X);
            }
            Some(Opcode::TAY) => {
                self.transfer_register(Registers::A, Registers::Y);
            }
            Some(Opcode::TXA) => {
                self.transfer_register(Registers::X, Registers::A);
            }
            Some(Opcode::TYA) => {
                self.transfer_register(Registers::Y, Registers::A);
            }
            Some(Opcode::TSX) => {
                self.tsx();
            }
            Some(Opcode::TXS) => {
                self.txs();
            }
            Some(Opcode::JSRAbsolute) => {
                self.jsr_absolute();
                ////println!("JSR ${:04X}", sub_addr);
            }
            Some(Opcode::ROLAccumulator) => {
                self.rol_accumulator();
            }
            Some(Opcode::ROLZeroPage) => {
                self.rol_zero_page();
            }
            Some(Opcode::ROLZeroPageX) => {
                self.rol_zero_page_x();
            }
            Some(Opcode::ROLAbsolute) => {
                self.rol_absolute();
            }
            Some(Opcode::ROLAbsoluteX) => {
                self.rol_absolute_x();
            }
            Some(Opcode::RORAccumulator) => {
                self.ror_accumulator();
            }
            Some(Opcode::RORZeroPage) => {
                self.ror_zero_page();
            }
            Some(Opcode::RORZeroPageX) => {
                self.ror_zero_page_x();
            }
            Some(Opcode::RORAbsolute) => {
                self.ror_absolute();
            }
            Some(Opcode::RORAbsoluteX) => {
                self.ror_absolute_x();
            }
            Some(Opcode::RTS) => {
                self.rts();

                ////println!("RTS to ${:04X}", self.pc);
            }
            Some(Opcode::RTI) => {
                self.rti();
            }
            Some(Opcode::NOP) => {
                self.nop();
            }
            Some(Opcode::BRK) => {
                self.brk();
            }
            None => return Flow::Illegal(opcode),
        }
        Flow::Continue
    }

    pub fn sp_to_addr(&self) -> u16 {
        0x0100 | self.sp as u16
    }

    pub fn advance_clock(&mut self) {
        self.clock = self.clock.wrapping_add(1);
        self.memory.tick();
    }

    pub fn read_byte(&mut self, addr: u16) -> u8 {
        self.advance_clock();
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

        // Additional cycle if page crossed: the address is read first with
        // the high byte not yet carried into
        if (base & 0xFF00) != (addr & 0xFF00) {
            self.read_byte((base & 0xFF00) | (addr & 0x00FF));
        }
        addr
    }

    pub fn fetch_byte(&mut self) -> u8 {
        let byte = self.memory.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        self.advance_clock();

        byte
    }

    pub fn fetch_word(&mut self) -> u16 {
        let low = self.fetch_byte() as u16;
        let high = self.fetch_byte() as u16;
        (high << 8) | low
    }

    pub fn push_byte(&mut self, value: u8) {
        let addr = self.sp_to_addr();
        self.memory.write(addr, value);
        self.sp = self.sp.wrapping_sub(1); // decrement SP after push
        self.advance_clock();
    }

    pub fn pop_byte(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1); // increment SP before pop
        let addr = self.sp_to_addr();
        let value = self.memory.read(addr);
        self.advance_clock();
        value
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
