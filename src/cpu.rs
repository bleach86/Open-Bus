use crate::instructions::op_codes::Opcode;
use crate::ram::Memory;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::{
    io::{IsTerminal, Read, stdin},
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

const NS_PER_CYCLE: u64 = 125; // 8 MHz
const PACE_INTERVAL_CYCLES: u64 = 4_000; // check every 0.5 ms of emulated time

#[derive(Default, Clone)]
pub struct CPU {
    pub pc: u16,                // Program Counter
    pub sp: u8,                 // Stack Pointer (8-bit, page $0100)
    pub a: u8,                  // Accumulator
    pub x: u8,                  // X Register
    pub y: u8,                  // Y Register
    pub status: StatusRegister, // Status Register
    pub memory: Memory,         // Memory
    pub irq: bool,              // IRQ line: held by a device until it's served
    pub nmi: bool,              // NMI: an edge, taken once
}

impl CPU {
    pub fn new() -> Self {
        CPU::default()
    }

    pub fn reset(&mut self) {
        let mut cycles = u32::MAX;
        self.pc = self.read_word(0xFFFC, &mut cycles); // the reset vector
        self.sp = 0xFF; // typical reset value on 6502
        self.a = 0;
        self.x = 0;
        self.y = 0;

        self.status = StatusRegister::default();
        self.status.interrupt_disable = true; // reset sets I, as on the 6502
    }

    pub fn execute_forever(&mut self) {
        let terminal = &mut Terminal::new();

        println!("PC set to {:04X}", self.pc);
        //std::process::exit(0);

        let cycles = u32::MAX;

        self.execute_inner(cycles, terminal, true);
    }

    pub fn execute(&mut self, mut cycles: u32) {
        let terminal = &mut Terminal::new();

        println!("PC set to {:04X}", self.pc);
        //std::process::exit(0);

        self.execute_inner(cycles, terminal, false);
    }

    /// The run loop: an instruction at a time, for `cycles` cycles. As on
    /// the 6502, an instruction once fetched runs all its cycles: the last
    /// one can end past `cycles`.
    fn execute_inner(&mut self, cycles: u32, terminal: &mut Terminal, run_forever: bool) {
        let mut total_cycles: u64 = 0;
        let mut next_pace: u64 = PACE_INTERVAL_CYCLES;
        let start = Instant::now();
        // The cycle counter: every cycle counts it on, wrapping round as a
        // hardware counter does.
        let mut clock: u32 = 0;

        while run_forever || total_cycles < cycles as u64 {
            let cycle_start = clock;
            let i_before = self.status.interrupt_disable;

            let opcode = self.fetch_byte(&mut clock);

            match self.exec_opcode(opcode, &mut clock) {
                Flow::Continue => {}
                Flow::Illegal(opcode) => {
                    println!("Unimplemented opcode: ${:02X}", opcode);
                    break;
                }
            }

            terminal.poll_input(self);

            // The 6502 looks at IRQ before an instruction's last cycle: the I
            // that CLI, SEI and PLP set on that cycle counts from the next.
            let masked = match Opcode::from_byte(opcode) {
                Some(Opcode::CLI | Opcode::SEI | Opcode::PLP) => i_before,
                _ => self.status.interrupt_disable,
            };

            // Interrupts are taken between instructions; NMI first.
            if self.nmi {
                self.interrupt(0xFFFA, &mut clock);
                self.nmi = false;
            } else if self.irq && !masked {
                self.interrupt(0xFFFE, &mut clock);
            }

            let used = clock.wrapping_sub(cycle_start) as u64;
            total_cycles += used;

            if total_cycles >= next_pace {
                next_pace = total_cycles + PACE_INTERVAL_CYCLES;
                let target = Duration::from_nanos(total_cycles * NS_PER_CYCLE);
                let elapsed = start.elapsed();
                if target > elapsed {
                    let ahead = target - elapsed;
                    if ahead > Duration::from_micros(300) {
                        sleep(ahead - Duration::from_micros(200));
                    }
                    while start.elapsed() < target {
                        std::hint::spin_loop();
                    }
                }
            }
        }
    }

    /// One instruction, already fetched: what execute_inner's loop does next.
    pub(crate) fn exec_opcode(&mut self, opcode: u8, cycles: &mut u32) -> Flow {
        match Opcode::from_byte(opcode) {
            Some(Opcode::ADCImmediate) => {
                self.adc_immediate(cycles);
            }
            Some(Opcode::ADCZeroPage) => {
                self.adc_zero_page(cycles);
            }
            Some(Opcode::ADCZeroPageX) => {
                self.adc_zero_page_x(cycles);
            }
            Some(Opcode::ADCAbsolute) => {
                self.adc_absolute(cycles);
            }
            Some(Opcode::ADCAbsoluteX) => {
                self.adc_absolute_x(cycles);
            }
            Some(Opcode::ADCAbsoluteY) => {
                self.adc_absolute_y(cycles);
            }
            Some(Opcode::ADCIndirectX) => {
                self.adc_indirect_x(cycles);
            }
            Some(Opcode::ADCIndirectY) => {
                self.adc_indirect_y(cycles);
            }
            Some(Opcode::ANDImmediate) => {
                self.and_immediate(cycles);
            }
            Some(Opcode::ANDZeroPage) => {
                self.and_zero_page(cycles);
            }
            Some(Opcode::ANDZeroPageX) => {
                self.and_zero_page_x(cycles);
            }
            Some(Opcode::ANDAbsolute) => {
                self.and_absolute(cycles);
            }
            Some(Opcode::ANDAbsoluteX) => {
                self.and_absolute_x(cycles);
            }
            Some(Opcode::ANDAbsoluteY) => {
                self.and_absolute_y(cycles);
            }
            Some(Opcode::ANDIndirectX) => {
                self.and_indirect_x(cycles);
            }
            Some(Opcode::ANDIndirectY) => {
                self.and_indirect_y(cycles);
            }
            Some(Opcode::ASLAccumulator) => {
                self.asl_accumulator(cycles);
            }
            Some(Opcode::ASLZeroPage) => {
                self.asl_zero_page(cycles);
            }
            Some(Opcode::ASLZeroPageX) => {
                self.asl_zero_page_x(cycles);
            }
            Some(Opcode::ASLAbsolute) => {
                self.asl_absolute(cycles);
            }
            Some(Opcode::ASLAbsoluteX) => {
                self.asl_absolute_x(cycles);
            }
            Some(Opcode::BCC) => {
                self.branch(cycles, StatusFlag::Carry, Opcode::BCC);
            }
            Some(Opcode::BCS) => {
                self.branch(cycles, StatusFlag::Carry, Opcode::BCS);
            }
            Some(Opcode::BEQ) => {
                self.branch(cycles, StatusFlag::Zero, Opcode::BEQ);
            }
            Some(Opcode::BMI) => {
                self.branch(cycles, StatusFlag::Negative, Opcode::BMI);
            }
            Some(Opcode::BNE) => {
                self.branch(cycles, StatusFlag::Zero, Opcode::BNE);
            }
            Some(Opcode::BPL) => {
                self.branch(cycles, StatusFlag::Negative, Opcode::BPL);
            }
            Some(Opcode::BVC) => {
                self.branch(cycles, StatusFlag::Overflow, Opcode::BVC);
            }
            Some(Opcode::BVS) => {
                self.branch(cycles, StatusFlag::Overflow, Opcode::BVS);
            }
            Some(Opcode::BITZeroPage) => {
                self.bit_zero_page(cycles);
            }
            Some(Opcode::BITAbsolute) => {
                self.bit_absolute(cycles);
            }
            Some(Opcode::CLC) => {
                self.mod_flag(StatusFlag::Carry, false, cycles);
            }
            Some(Opcode::CLD) => {
                self.mod_flag(StatusFlag::DecimalMode, false, cycles);
            }
            Some(Opcode::CLI) => {
                self.mod_flag(StatusFlag::InterruptDisable, false, cycles);
            }
            Some(Opcode::CLV) => {
                self.mod_flag(StatusFlag::Overflow, false, cycles);
            }
            Some(Opcode::CMPImmediate) => {
                self.cmp_immediate(cycles);
            }
            Some(Opcode::CMPZeroPage) => {
                self.cmp_zero_page(cycles);
            }
            Some(Opcode::CMPZeroPageX) => {
                self.cmp_zero_page_x(cycles);
            }
            Some(Opcode::CMPAbsolute) => {
                self.cmp_absolute(cycles);
            }
            Some(Opcode::CMPAbsoluteX) => {
                self.cmp_absolute_x(cycles);
            }
            Some(Opcode::CMPAbsoluteY) => {
                self.cmp_absolute_y(cycles);
            }
            Some(Opcode::CMPIndirectX) => {
                self.cmp_indirect_x(cycles);
            }
            Some(Opcode::CMPIndirectY) => {
                self.cmp_indirect_y(cycles);
            }
            Some(Opcode::CPXImmediate) => {
                self.cpx_immediate(cycles);
            }
            Some(Opcode::CPXZeroPage) => {
                self.cpx_zero_page(cycles);
            }
            Some(Opcode::CPXAbsolute) => {
                self.cpx_absolute(cycles);
            }
            Some(Opcode::CPYImmediate) => {
                self.cpy_immediate(cycles);
            }
            Some(Opcode::CPYZeroPage) => {
                self.cpy_zero_page(cycles);
            }
            Some(Opcode::CPYAbsolute) => {
                self.cpy_absolute(cycles);
            }
            Some(Opcode::DEX) => {
                self.dex(cycles);
            }
            Some(Opcode::DEY) => {
                self.dey(cycles);
            }
            Some(Opcode::DECZeroPage) => {
                self.dec_zero_page(cycles);
            }
            Some(Opcode::DECZeroPageX) => {
                self.dec_zero_page_x(cycles);
            }
            Some(Opcode::DECAbsolute) => {
                self.dec_absolute(cycles);
            }
            Some(Opcode::DECAbsoluteX) => {
                self.dec_absolute_x(cycles);
            }
            Some(Opcode::EORImmediate) => {
                self.eor_immediate(cycles);
            }
            Some(Opcode::EORZeroPage) => {
                self.eor_zero_page(cycles);
            }
            Some(Opcode::EORZeroPageX) => {
                self.eor_zero_page_x(cycles);
            }
            Some(Opcode::EORAbsolute) => {
                self.eor_absolute(cycles);
            }
            Some(Opcode::EORAbsoluteX) => {
                self.eor_absolute_x(cycles);
            }
            Some(Opcode::EORAbsoluteY) => {
                self.eor_absolute_y(cycles);
            }
            Some(Opcode::EORIndirectX) => {
                self.eor_indirect_x(cycles);
            }
            Some(Opcode::EORIndirectY) => {
                self.eor_indirect_y(cycles);
            }
            Some(Opcode::JMPAbsolute) => {
                self.jmp_absolute(cycles);
            }
            Some(Opcode::JMPIndirect) => {
                self.jmp_indirect(cycles);
            }
            Some(Opcode::INX) => {
                self.inx(cycles);
            }
            Some(Opcode::INY) => {
                self.iny(cycles);
            }
            Some(Opcode::INCZeroPage) => {
                self.inc_zero_page(cycles);
            }
            Some(Opcode::INCZeroPageX) => {
                self.inc_zero_page_x(cycles);
            }
            Some(Opcode::INCAbsolute) => {
                self.inc_absolute(cycles);
            }
            Some(Opcode::INCAbsoluteX) => {
                self.inc_absolute_x(cycles);
            }
            Some(Opcode::LDAImmediate) => {
                self.lda_immediate(cycles);
            }
            Some(Opcode::LDAZeroPage) => {
                self.lda_zero_page(cycles);
            }
            Some(Opcode::LDAZeroPageX) => {
                self.lda_zero_page_x(cycles);
            }
            Some(Opcode::LDAAbsolute) => {
                self.lda_absolute(cycles);
            }
            Some(Opcode::LDAAbsoluteX) => {
                self.lda_absolute_x(cycles);
            }
            Some(Opcode::LDAAbsoluteY) => {
                self.lda_absolute_y(cycles);
            }
            Some(Opcode::LDAIndirectX) => {
                self.lda_indirect_x(cycles);
            }
            Some(Opcode::LDAIndirectY) => {
                self.lda_indirect_y(cycles);
            }
            Some(Opcode::LDXImmediate) => {
                self.ldx_immediate(cycles);
            }
            Some(Opcode::LDXZeroPage) => {
                self.ldx_zero_page(cycles);
            }
            Some(Opcode::LDXZeroPageY) => {
                self.ldx_zero_page_y(cycles);
            }
            Some(Opcode::LDXAbsolute) => {
                self.ldx_absolute(cycles);
            }
            Some(Opcode::LDXAbsoluteY) => {
                self.ldx_absolute_y(cycles);
            }
            Some(Opcode::LDYImmediate) => {
                self.ldy_immediate(cycles);
            }
            Some(Opcode::LDYZeroPage) => {
                self.ldy_zero_page(cycles);
            }
            Some(Opcode::LDYZeroPageX) => {
                self.ldy_zero_page_x(cycles);
            }
            Some(Opcode::LDYAbsolute) => {
                self.ldy_absolute(cycles);
            }
            Some(Opcode::LDYAbsoluteX) => {
                self.ldy_absolute_x(cycles);
            }
            Some(Opcode::LSRAccumulator) => {
                self.lsr_accumulator(cycles);
            }
            Some(Opcode::LSRZeroPage) => {
                self.lsr_zero_page(cycles);
            }
            Some(Opcode::LSRZeroPageX) => {
                self.lsr_zero_page_x(cycles);
            }
            Some(Opcode::LSRAbsolute) => {
                self.lsr_absolute(cycles);
            }
            Some(Opcode::LSRAbsoluteX) => {
                self.lsr_absolute_x(cycles);
            }
            Some(Opcode::PHA) => {
                self.pha(cycles);
            }
            Some(Opcode::PHP) => {
                self.php(cycles);
            }
            Some(Opcode::PLA) => {
                self.pla(cycles);
            }
            Some(Opcode::PLP) => {
                self.plp(cycles);
            }
            Some(Opcode::ORAbsoluteY) => {
                self.or_absolute_y(cycles);
            }
            Some(Opcode::ORIndirectX) => {
                self.or_indirect_x(cycles);
            }
            Some(Opcode::ORIndirectY) => {
                self.or_indirect_y(cycles);
            }
            Some(Opcode::ORImmediate) => {
                self.or_immediate(cycles);
            }
            Some(Opcode::ORZeroPage) => {
                self.or_zero_page(cycles);
            }
            Some(Opcode::ORZeroPageX) => {
                self.or_zero_page_x(cycles);
            }
            Some(Opcode::ORAbsolute) => {
                self.or_absolute(cycles);
            }
            Some(Opcode::ORAbsoluteX) => {
                self.or_absolute_x(cycles);
            }
            Some(Opcode::SBCImmediate) => {
                self.sbc_immediate(cycles);
            }
            Some(Opcode::SBCZeroPage) => {
                self.sbc_zero_page(cycles);
            }
            Some(Opcode::SBCZeroPageX) => {
                self.sbc_zero_page_x(cycles);
            }
            Some(Opcode::SBCAbsolute) => {
                self.sbc_absolute(cycles);
            }
            Some(Opcode::SBCAbsoluteX) => {
                self.sbc_absolute_x(cycles);
            }
            Some(Opcode::SBCAbsoluteY) => {
                self.sbc_absolute_y(cycles);
            }
            Some(Opcode::SBCIndirectX) => {
                self.sbc_indirect_x(cycles);
            }
            Some(Opcode::SBCIndirectY) => {
                self.sbc_indirect_y(cycles);
            }
            Some(Opcode::SEC) => {
                self.mod_flag(StatusFlag::Carry, true, cycles);
            }
            Some(Opcode::SED) => {
                self.mod_flag(StatusFlag::DecimalMode, true, cycles);
            }
            Some(Opcode::SEI) => {
                self.mod_flag(StatusFlag::InterruptDisable, true, cycles);
            }
            Some(Opcode::STAZeroPage) => {
                self.sta_zero_page(cycles);
            }
            Some(Opcode::STAZeroPageX) => {
                self.sta_zero_page_x(cycles);
            }
            Some(Opcode::STAAbsolute) => {
                self.sta_absolute(cycles);
            }
            Some(Opcode::STAAbsoluteX) => {
                self.sta_absolute_x(cycles);
            }
            Some(Opcode::STAAbsoluteY) => {
                self.sta_absolute_y(cycles);
            }
            Some(Opcode::STAIndirectX) => {
                self.sta_indirect_x(cycles);
            }
            Some(Opcode::STAIndirectY) => {
                self.sta_indirect_y(cycles);
            }
            Some(Opcode::STXZeroPage) => {
                self.stx_zero_page(cycles);
            }
            Some(Opcode::STXZeroPageY) => {
                self.stx_zero_page_y(cycles);
            }
            Some(Opcode::STXAbsolute) => {
                self.stx_absolute(cycles);
            }
            Some(Opcode::STYZeroPage) => {
                self.sty_zero_page(cycles);
            }
            Some(Opcode::STYZeroPageX) => {
                self.sty_zero_page_x(cycles);
            }
            Some(Opcode::STYAbsolute) => {
                self.sty_absolute(cycles);
            }
            Some(Opcode::TAX) => {
                self.transfer_register(Registers::A, Registers::X, cycles);
            }
            Some(Opcode::TAY) => {
                self.transfer_register(Registers::A, Registers::Y, cycles);
            }
            Some(Opcode::TXA) => {
                self.transfer_register(Registers::X, Registers::A, cycles);
            }
            Some(Opcode::TYA) => {
                self.transfer_register(Registers::Y, Registers::A, cycles);
            }
            Some(Opcode::TSX) => {
                self.tsx(cycles);
            }
            Some(Opcode::TXS) => {
                self.txs(cycles);
            }
            Some(Opcode::JSRAbsolute) => {
                self.jsr_absolute(cycles);
                ////println!("JSR ${:04X}", sub_addr);
            }
            Some(Opcode::ROLAccumulator) => {
                self.rol_accumulator(cycles);
            }
            Some(Opcode::ROLZeroPage) => {
                self.rol_zero_page(cycles);
            }
            Some(Opcode::ROLZeroPageX) => {
                self.rol_zero_page_x(cycles);
            }
            Some(Opcode::ROLAbsolute) => {
                self.rol_absolute(cycles);
            }
            Some(Opcode::ROLAbsoluteX) => {
                self.rol_absolute_x(cycles);
            }
            Some(Opcode::RORAccumulator) => {
                self.ror_accumulator(cycles);
            }
            Some(Opcode::RORZeroPage) => {
                self.ror_zero_page(cycles);
            }
            Some(Opcode::RORZeroPageX) => {
                self.ror_zero_page_x(cycles);
            }
            Some(Opcode::RORAbsolute) => {
                self.ror_absolute(cycles);
            }
            Some(Opcode::RORAbsoluteX) => {
                self.ror_absolute_x(cycles);
            }
            Some(Opcode::RTS) => {
                self.rts(cycles);

                ////println!("RTS to ${:04X}", self.pc);
            }
            Some(Opcode::RTI) => {
                self.rti(cycles);
            }
            Some(Opcode::NOP) => {
                self.nop(cycles);
            }
            Some(Opcode::BRK) => {
                self.brk(cycles);
            }
            None => return Flow::Illegal(opcode),
        }
        Flow::Continue
    }

    pub fn sp_to_addr(&self) -> u16 {
        0x0100 | self.sp as u16
    }

    pub fn read_byte(&mut self, addr: u16, cycles: &mut u32) -> u8 {
        *cycles = cycles.wrapping_add(1);
        self.memory.read(addr)
    }

    pub fn write_byte(&mut self, addr: u16, value: u8, cycles: &mut u32) {
        *cycles = cycles.wrapping_add(1);
        self.memory.write(addr, value);
    }

    pub fn read_word(&mut self, addr: u16, cycles: &mut u32) -> u16 {
        let low = self.read_byte(addr, cycles) as u16;
        let high = self.read_byte(addr.wrapping_add(1), cycles) as u16;
        (high << 8) | low
    }

    pub fn fetch_addr_indirect_x(&mut self, cycles: &mut u32) -> u16 {
        let zp_addr = self.fetch_byte(cycles);
        let effective_addr_lo = zp_addr.wrapping_add(self.x);
        let lo = self.read_byte(effective_addr_lo as u16, cycles) as u16;
        let hi = self.read_byte((effective_addr_lo.wrapping_add(1) & 0x00FF) as u16, cycles) as u16;
        (hi << 8) | lo
    }

    pub fn fetch_addr_indirect_y(&mut self, cycles: &mut u32) -> u16 {
        let zp_addr = self.fetch_byte(cycles);
        let lo = self.read_byte(zp_addr as u16, cycles) as u16;
        let hi = self.read_byte((zp_addr.wrapping_add(1) & 0x00FF) as u16, cycles) as u16;
        let base = (hi << 8) | lo;
        let addr = base.wrapping_add(self.y as u16);

        // Additional cycle if page crossed
        if (base & 0xFF00) != (addr & 0xFF00) {
            *cycles = cycles.wrapping_add(1);
        }
        addr
    }

    pub fn fetch_byte(&mut self, cycles: &mut u32) -> u8 {
        let byte = self.memory.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        *cycles = cycles.wrapping_add(1);

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
        *cycles = cycles.wrapping_add(1);
    }

    pub fn pop_byte(&mut self, cycles: &mut u32) -> u8 {
        self.sp = self.sp.wrapping_add(1); // increment SP before pop
        let addr = self.sp_to_addr();
        let value = self.memory.read(addr);
        *cycles = cycles.wrapping_add(1);
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

#[derive(Default, Clone)]
struct Terminal {
    rx_buffer: Arc<Mutex<VecDeque<u8>>>,
    /// The first Terminal: it set up the host terminal, and puts it back.
    first: bool,
}

impl Terminal {
    fn new() -> Self {
        // One reader of stdin for the process: another Terminal's thread
        // would take this one's keys.
        static RX_BUFFER: OnceLock<Arc<Mutex<VecDeque<u8>>>> = OnceLock::new();

        let mut first = false;
        let rx_buffer = RX_BUFFER
            .get_or_init(|| {
                first = true;

                // A key at a time and not echoed: the machine echoes what it
                // reads. Ctrl-C comes in as a key too.
                stty(&["-icanon", "-echo", "-isig"]);

                let rx_buffer = Arc::new(Mutex::new(VecDeque::new()));
                let rx_clone = Arc::clone(&rx_buffer);

                // Spawn a thread to read stdin asynchronously
                thread::spawn(move || {
                    let stdin = stdin();
                    for byte in stdin.bytes() {
                        if let Ok(b) = byte {
                            if b == 0x03 {
                                // Ctrl-C: off
                                stty(&["icanon", "echo", "isig"]);
                                std::process::exit(0);
                            }
                            rx_clone.lock().unwrap().push_back(b);
                        }
                    }
                });

                rx_buffer
            })
            .clone();

        Self { rx_buffer, first }
    }

    fn poll_input(&mut self, cpu: &mut CPU) {
        if cpu.memory.read(0xFFF9) != 0 {
            return; // Input not requested
        }

        if let Some(ascii) = self.rx_buffer.lock().unwrap().pop_front() {
            cpu.memory.write(0xFFF9, ascii);
            //println!("Emulator input (ASCII): {}", ascii);
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if self.first {
            stty(&["icanon", "echo", "isig"]);
        }
    }
}

/// Set the host terminal's modes; nothing to set when stdin isn't one.
fn stty(settings: &[&str]) {
    if stdin().is_terminal() {
        let _ = std::process::Command::new("stty").args(settings).status();
    }
}
