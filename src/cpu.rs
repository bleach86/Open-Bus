use crate::instructions::op_codes::Opcode;
use crate::ram::Memory;
use std::{
    thread::sleep,
    time::{Duration, Instant},
};


/// Result of executing one instruction.
pub enum Flow {
    Continue,
    Illegal(u8),
}

/// How an instruction finds its operand. Each makes the bus cycles a 6502
/// makes for it, a read or a write every cycle, so the cycles an
/// instruction takes are the accesses it makes.
#[derive(Clone, Copy)]
pub enum Mode {
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    IndirectX,
    IndirectY,
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
        use Mode::*;
        use Opcode::*;
        // An instruction reading an operand; writing a register; changing
        // memory (or A) in place; on a register, after the implied cycle.
        macro_rules! read {
            ($mode:expr, $op:ident) => {{
                let value = self.operand($mode, cycles);
                self.$op(value);
            }};
        }
        macro_rules! load {
            ($mode:expr, $reg:ident) => {{
                let value = self.operand($mode, cycles);
                self.$reg = self.load(value);
            }};
        }
        macro_rules! modify {
            ($mode:expr, $op:ident) => {
                self.modify($mode, CPU::$op, cycles)
            };
        }
        macro_rules! accumulator {
            ($op:ident) => {{
                self.implied(cycles);
                self.a = self.$op(self.a);
            }};
        }
        macro_rules! implied {
            ($($body:tt)*) => {{
                self.implied(cycles);
                $($body)*
            }};
        }

        let Some(op) = Opcode::from_byte(opcode) else {
            return Flow::Illegal(opcode);
        };
        match op {
            LDAImmediate => load!(Immediate, a),
            LDAZeroPage => load!(ZeroPage, a),
            LDAZeroPageX => load!(ZeroPageX, a),
            LDAAbsolute => load!(Absolute, a),
            LDAAbsoluteX => load!(AbsoluteX, a),
            LDAAbsoluteY => load!(AbsoluteY, a),
            LDAIndirectX => load!(IndirectX, a),
            LDAIndirectY => load!(IndirectY, a),
            LDXImmediate => load!(Immediate, x),
            LDXZeroPage => load!(ZeroPage, x),
            LDXZeroPageY => load!(ZeroPageY, x),
            LDXAbsolute => load!(Absolute, x),
            LDXAbsoluteY => load!(AbsoluteY, x),
            LDYImmediate => load!(Immediate, y),
            LDYZeroPage => load!(ZeroPage, y),
            LDYZeroPageX => load!(ZeroPageX, y),
            LDYAbsolute => load!(Absolute, y),
            LDYAbsoluteX => load!(AbsoluteX, y),

            STAZeroPage => self.store(ZeroPage, self.a, cycles),
            STAZeroPageX => self.store(ZeroPageX, self.a, cycles),
            STAAbsolute => self.store(Absolute, self.a, cycles),
            STAAbsoluteX => self.store(AbsoluteX, self.a, cycles),
            STAAbsoluteY => self.store(AbsoluteY, self.a, cycles),
            STAIndirectX => self.store(IndirectX, self.a, cycles),
            STAIndirectY => self.store(IndirectY, self.a, cycles),
            STXZeroPage => self.store(ZeroPage, self.x, cycles),
            STXZeroPageY => self.store(ZeroPageY, self.x, cycles),
            STXAbsolute => self.store(Absolute, self.x, cycles),
            STYZeroPage => self.store(ZeroPage, self.y, cycles),
            STYZeroPageX => self.store(ZeroPageX, self.y, cycles),
            STYAbsolute => self.store(Absolute, self.y, cycles),

            ADCImmediate => read!(Immediate, adc),
            ADCZeroPage => read!(ZeroPage, adc),
            ADCZeroPageX => read!(ZeroPageX, adc),
            ADCAbsolute => read!(Absolute, adc),
            ADCAbsoluteX => read!(AbsoluteX, adc),
            ADCAbsoluteY => read!(AbsoluteY, adc),
            ADCIndirectX => read!(IndirectX, adc),
            ADCIndirectY => read!(IndirectY, adc),
            SBCImmediate => read!(Immediate, sbc),
            SBCZeroPage => read!(ZeroPage, sbc),
            SBCZeroPageX => read!(ZeroPageX, sbc),
            SBCAbsolute => read!(Absolute, sbc),
            SBCAbsoluteX => read!(AbsoluteX, sbc),
            SBCAbsoluteY => read!(AbsoluteY, sbc),
            SBCIndirectX => read!(IndirectX, sbc),
            SBCIndirectY => read!(IndirectY, sbc),
            ANDImmediate => read!(Immediate, and),
            ANDZeroPage => read!(ZeroPage, and),
            ANDZeroPageX => read!(ZeroPageX, and),
            ANDAbsolute => read!(Absolute, and),
            ANDAbsoluteX => read!(AbsoluteX, and),
            ANDAbsoluteY => read!(AbsoluteY, and),
            ANDIndirectX => read!(IndirectX, and),
            ANDIndirectY => read!(IndirectY, and),
            ORImmediate => read!(Immediate, ora),
            ORZeroPage => read!(ZeroPage, ora),
            ORZeroPageX => read!(ZeroPageX, ora),
            ORAbsolute => read!(Absolute, ora),
            ORAbsoluteX => read!(AbsoluteX, ora),
            ORAbsoluteY => read!(AbsoluteY, ora),
            ORIndirectX => read!(IndirectX, ora),
            ORIndirectY => read!(IndirectY, ora),
            EORImmediate => read!(Immediate, eor),
            EORZeroPage => read!(ZeroPage, eor),
            EORZeroPageX => read!(ZeroPageX, eor),
            EORAbsolute => read!(Absolute, eor),
            EORAbsoluteX => read!(AbsoluteX, eor),
            EORAbsoluteY => read!(AbsoluteY, eor),
            EORIndirectX => read!(IndirectX, eor),
            EORIndirectY => read!(IndirectY, eor),
            BITZeroPage => read!(ZeroPage, bit),
            BITAbsolute => read!(Absolute, bit),
            CMPImmediate | CMPZeroPage | CMPZeroPageX | CMPAbsolute | CMPAbsoluteX | CMPAbsoluteY | CMPIndirectX
            | CMPIndirectY | CPXImmediate | CPXZeroPage | CPXAbsolute | CPYImmediate | CPYZeroPage | CPYAbsolute => {
                let (register, mode) = match op {
                    CMPImmediate => (self.a, Immediate),
                    CMPZeroPage => (self.a, ZeroPage),
                    CMPZeroPageX => (self.a, ZeroPageX),
                    CMPAbsolute => (self.a, Absolute),
                    CMPAbsoluteX => (self.a, AbsoluteX),
                    CMPAbsoluteY => (self.a, AbsoluteY),
                    CMPIndirectX => (self.a, IndirectX),
                    CMPIndirectY => (self.a, IndirectY),
                    CPXImmediate => (self.x, Immediate),
                    CPXZeroPage => (self.x, ZeroPage),
                    CPXAbsolute => (self.x, Absolute),
                    CPYImmediate => (self.y, Immediate),
                    CPYZeroPage => (self.y, ZeroPage),
                    _ => (self.y, Absolute),
                };
                let value = self.operand(mode, cycles);
                self.compare(register, value);
            }

            ASLAccumulator => accumulator!(asl),
            ASLZeroPage => modify!(ZeroPage, asl),
            ASLZeroPageX => modify!(ZeroPageX, asl),
            ASLAbsolute => modify!(Absolute, asl),
            ASLAbsoluteX => modify!(AbsoluteX, asl),
            LSRAccumulator => accumulator!(lsr),
            LSRZeroPage => modify!(ZeroPage, lsr),
            LSRZeroPageX => modify!(ZeroPageX, lsr),
            LSRAbsolute => modify!(Absolute, lsr),
            LSRAbsoluteX => modify!(AbsoluteX, lsr),
            ROLAccumulator => accumulator!(rol),
            ROLZeroPage => modify!(ZeroPage, rol),
            ROLZeroPageX => modify!(ZeroPageX, rol),
            ROLAbsolute => modify!(Absolute, rol),
            ROLAbsoluteX => modify!(AbsoluteX, rol),
            RORAccumulator => accumulator!(ror),
            RORZeroPage => modify!(ZeroPage, ror),
            RORZeroPageX => modify!(ZeroPageX, ror),
            RORAbsolute => modify!(Absolute, ror),
            RORAbsoluteX => modify!(AbsoluteX, ror),
            INCZeroPage => modify!(ZeroPage, inc),
            INCZeroPageX => modify!(ZeroPageX, inc),
            INCAbsolute => modify!(Absolute, inc),
            INCAbsoluteX => modify!(AbsoluteX, inc),
            DECZeroPage => modify!(ZeroPage, dec),
            DECZeroPageX => modify!(ZeroPageX, dec),
            DECAbsolute => modify!(Absolute, dec),
            DECAbsoluteX => modify!(AbsoluteX, dec),

            INX => implied!(self.x = self.inc(self.x)),
            INY => implied!(self.y = self.inc(self.y)),
            DEX => implied!(self.x = self.dec(self.x)),
            DEY => implied!(self.y = self.dec(self.y)),
            TAX => implied!(self.x = self.load(self.a)),
            TAY => implied!(self.y = self.load(self.a)),
            TXA => implied!(self.a = self.load(self.x)),
            TYA => implied!(self.a = self.load(self.y)),
            TSX => implied!(self.x = self.load(self.sp)),
            TXS => implied!(self.sp = self.x),
            CLC => implied!(self.status.carry = false),
            SEC => implied!(self.status.carry = true),
            CLI => implied!(self.status.interrupt_disable = false),
            SEI => implied!(self.status.interrupt_disable = true),
            CLD => implied!(self.status.decimal_mode = false),
            SED => implied!(self.status.decimal_mode = true),
            CLV => implied!(self.status.overflow = false),
            NOP => implied!(),

            PHA => self.push(self.a, cycles),
            // PHP always pushes with B (bit 4) set.
            PHP => self.push(self.status.to_byte() | 0x10, cycles),
            PLA => {
                let value = self.pull(cycles);
                self.a = self.load(value);
            }
            PLP => {
                let value = self.pull(cycles);
                self.status.from_byte(value);
            }

            BCC => self.branch(!self.status.carry, cycles),
            BCS => self.branch(self.status.carry, cycles),
            BNE => self.branch(!self.status.zero, cycles),
            BEQ => self.branch(self.status.zero, cycles),
            BPL => self.branch(!self.status.negative, cycles),
            BMI => self.branch(self.status.negative, cycles),
            BVC => self.branch(!self.status.overflow, cycles),
            BVS => self.branch(self.status.overflow, cycles),

            JMPAbsolute => self.jmp_absolute(cycles),
            JMPIndirect => self.jmp_indirect(cycles),
            JSRAbsolute => self.jsr(cycles),
            RTS => self.rts(cycles),
            RTI => self.rti(cycles),
            BRK => self.brk(cycles),
        }
        Flow::Continue
    }

    /// N and Z from a result.
    pub fn set_nz(&mut self, value: u8) {
        self.status.zero = value == 0;
        self.status.negative = value & 0x80 != 0;
    }

    /// The cycle a one-byte instruction spends reading the byte after it,
    /// which it then ignores.
    pub fn implied(&mut self, cycles: &mut u32) {
        self.read_byte(self.pc, cycles);
    }

    /// The operand's address, by the bus cycles a 6502 takes to work it
    /// out. Indexing that carries into the high byte costs a cycle, reading
    /// at the address with the high byte not yet fixed: always for a store
    /// or a change in place (`fixed`), only when it carries for a read.
    /// Zero page indexing reads the unindexed address while it adds.
    fn address(&mut self, mode: Mode, fixed: bool, cycles: &mut u32) -> u16 {
        let indexed = |cpu: &mut CPU, base: u16, index: u8, cycles: &mut u32| {
            let address = base.wrapping_add(index as u16);
            if fixed || address & 0xFF00 != base & 0xFF00 {
                cpu.read_byte(base & 0xFF00 | address & 0x00FF, cycles);
            }
            address
        };
        match mode {
            Mode::Immediate => {
                let address = self.pc;
                self.pc = self.pc.wrapping_add(1);
                address
            }
            Mode::ZeroPage => self.fetch_byte(cycles) as u16,
            Mode::ZeroPageX | Mode::ZeroPageY => {
                let base = self.fetch_byte(cycles);
                self.read_byte(base as u16, cycles);
                let index = if matches!(mode, Mode::ZeroPageX) { self.x } else { self.y };
                base.wrapping_add(index) as u16
            }
            Mode::Absolute => self.fetch_word(cycles),
            Mode::AbsoluteX => {
                let base = self.fetch_word(cycles);
                indexed(self, base, self.x, cycles)
            }
            Mode::AbsoluteY => {
                let base = self.fetch_word(cycles);
                indexed(self, base, self.y, cycles)
            }
            Mode::IndirectX => {
                let base = self.fetch_byte(cycles);
                self.read_byte(base as u16, cycles);
                let pointer = base.wrapping_add(self.x);
                self.read_zero_page_word(pointer, cycles)
            }
            Mode::IndirectY => {
                let pointer = self.fetch_byte(cycles);
                let base = self.read_zero_page_word(pointer, cycles);
                indexed(self, base, self.y, cycles)
            }
        }
    }

    /// An operand to read.
    pub fn operand(&mut self, mode: Mode, cycles: &mut u32) -> u8 {
        let address = self.address(mode, false, cycles);
        self.read_byte(address, cycles)
    }

    /// A store.
    pub fn store(&mut self, mode: Mode, value: u8, cycles: &mut u32) {
        let address = self.address(mode, true, cycles);
        self.write_byte(address, value, cycles);
    }

    /// A change in place: read, the old value written back while the new
    /// one is worked out, the new one written.
    pub fn modify(&mut self, mode: Mode, change: fn(&mut CPU, u8) -> u8, cycles: &mut u32) {
        let address = self.address(mode, true, cycles);
        let value = self.read_byte(address, cycles);
        self.write_byte(address, value, cycles);
        let result = change(self, value);
        self.write_byte(address, result, cycles);
    }

    pub fn stack_address(&self) -> u16 {
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
        let addr = self.stack_address();
        self.memory.write(addr, value);
        self.sp = self.sp.wrapping_sub(1); // decrement SP after push
        *cycles = cycles.saturating_sub(1);
    }

    pub fn pop_byte(&mut self, cycles: &mut u32) -> u8 {
        self.sp = self.sp.wrapping_add(1); // increment SP before pop
        let addr = self.stack_address();
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
