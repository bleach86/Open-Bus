//! The machine-language monitor: a debugger on the host's side, after VICE's,
//! that looks into the machine without disturbing it. With `--monitor PORT`
//! the emulator listens on 127.0.0.1:PORT; a client attaching (say, `nc
//! localhost PORT`) stops the CPU, and the monitor takes commands until it
//! is told to go on. `h` lists them.
//!
//! Three parts: the `Link` between a client's thread and the CPU's (lines
//! in, text out, a flag to stop at); the `Watch` the bus keeps, which says
//! before each instruction whether to stop (a breakpoint, a step, a watched
//! address written, a client asking); and the `session`, run on the CPU's
//! thread while it is stopped. Reading memory goes through the bus's
//! `peek`, which leaves ports as they are.

use crate::cpu::CPU;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// The system ROM's labels, from the build (ld65 -Ln).
const SYSTEM_LABELS: &str = include_str!(concat!(env!("OUT_DIR"), "/system.lbl"));

const HELP: &str = "\
r                 registers; r a 12, r pc c000 to set one
d [addr] [n]      disassemble (from PC)
m addr [to]       memory; > addr bytes... to write
v addr [n]        VRAM
io                the chips' registers
b [addr]          a breakpoint there, or gone if there was one; b alone lists
w addr            stop when addr is written, or not any more
bc                no breakpoints or watches
s [n]             step n instructions
n                 step, over a jsr
f                 run to the next frame
g [addr]          go on (from addr)
l name            labels: name.lbl on the drive, or a file
q                 go on, the monitor's breakpoints and watches gone
Addresses are hex ($ optional), labels (_main, or main) or pc.
";

/// Between a client's thread and the CPU's.
pub struct Link {
    stop: AtomicBool,
    commands: Mutex<Option<Receiver<String>>>,
    out: Mutex<Option<Box<dyn Write + Send>>>,
    attached: AtomicBool,
    /// Where `l NAME` finds NAME.lbl: the drive.
    drive: Option<PathBuf>,
}

impl Link {
    pub fn new(drive: Option<PathBuf>) -> Arc<Link> {
        Arc::new(Link {
            stop: AtomicBool::new(false),
            commands: Mutex::new(None),
            out: Mutex::new(None),
            attached: AtomicBool::new(false),
            drive,
        })
    }

    /// A client attaches: what the monitor says goes to `out`, and its
    /// lines go in through the sender returned; dropping that detaches it.
    /// The CPU stops at its next instruction.
    pub fn attach(&self, out: Box<dyn Write + Send>) -> Sender<String> {
        let (lines, commands) = channel();
        *self.commands.lock().unwrap() = Some(commands);
        *self.out.lock().unwrap() = Some(out);
        self.attached.store(true, Ordering::Release);
        self.stop.store(true, Ordering::Release);
        lines
    }

    /// Listen on 127.0.0.1:`port`, a client at a time.
    pub fn listen(self: &Arc<Link>, port: u16) -> std::io::Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let link = Arc::clone(self);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                if link.attached.load(Ordering::Acquire) {
                    let mut stream = stream;
                    let _ = stream.write_all(b"The monitor has a client already.\n");
                    continue;
                }
                let Ok(out) = stream.try_clone() else { continue };
                let lines = link.attach(Box::new(out));
                let link = Arc::clone(&link);
                std::thread::spawn(move || {
                    for line in BufReader::new(stream).lines() {
                        let Ok(line) = line else { break };
                        if lines.send(line).is_err() {
                            break;
                        }
                        // Typed while the machine runs: it stops to listen.
                        link.stop.store(true, Ordering::Release);
                    }
                    link.attached.store(false, Ordering::Release);
                });
            }
        });
        Ok(())
    }

    fn say(&self, text: &str) {
        if let Some(out) = self.out.lock().unwrap().as_mut() {
            let _ = out.write_all(text.as_bytes());
            let _ = out.flush();
        }
    }

    /// The client's next line; None once it has gone.
    fn next_command(&self) -> Option<String> {
        let commands = self.commands.lock().unwrap();
        commands.as_ref()?.recv().ok()
    }
}

/// Labels and their addresses: the system ROM's, and a cart's when loaded.
#[derive(Default)]
struct Labels {
    names: HashMap<u16, String>,
    addresses: HashMap<String, u16>,
}

impl Labels {
    /// ld65's -Ln label file: "al 00C012 .name" a line. cc65's numbered
    /// local labels (L0012) and the linker's own (__NAME__) are left out.
    fn load(&mut self, text: &str) {
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            let ["al", address, name] = words[..] else { continue };
            let name = name.trim_start_matches('.');
            let numbered = name.len() == 5 && name.starts_with('L') && name[1..].chars().all(|c| c.is_ascii_hexdigit());
            let Ok(address) = u32::from_str_radix(address, 16) else { continue };
            if numbered || name.starts_with("__") || address > 0xFFFF {
                continue;
            }
            let address = address as u16;
            // C's names (_main) over the assembler's, where both are there.
            let better = self.names.get(&address).is_none_or(|old| name.starts_with('_') && !old.starts_with('_'));
            if better {
                self.names.insert(address, name.to_string());
            }
            self.addresses.insert(name.to_string(), address);
        }
    }

    fn address(&self, word: &str) -> Option<u16> {
        self.addresses
            .get(word)
            .or_else(|| self.addresses.get(&format!("_{word}")))
            .copied()
            .or_else(|| u16::from_str_radix(word.trim_start_matches('$'), 16).ok())
    }
}

/// What the monitor waits for, kept with the bus.
pub struct Watch {
    link: Arc<Link>,
    breakpoints: Box<[u64; 1024]>,
    watches: Vec<u16>,
    written: Option<(u16, u8)>,
    steps: u32,
    over: Option<u16>,
    frame: bool,
    frame_ended: bool,
    labels: Labels,
    rom_labels: Labels,
}

impl Watch {
    pub fn new(link: Arc<Link>) -> Watch {
        let mut labels = Labels::default();
        labels.load(SYSTEM_LABELS);
        let mut rom_labels = Labels::default();
        rom_labels.load(SYSTEM_LABELS);
        Watch {
            link,
            breakpoints: Box::new([0; 1024]),
            watches: Vec::new(),
            written: None,
            steps: 0,
            over: None,
            frame: false,
            frame_ended: false,
            labels,
            rom_labels,
        }
    }

    fn breakpoint(&self, pc: u16) -> bool {
        self.breakpoints[pc as usize >> 6] >> (pc & 63) & 1 != 0
    }

    fn toggle_breakpoint(&mut self, pc: u16) -> bool {
        self.breakpoints[pc as usize >> 6] ^= 1 << (pc & 63);
        self.breakpoint(pc)
    }

    /// Stop before the instruction at `pc`? (After a stop, the CPU runs the
    /// instruction it stopped before without asking again.)
    pub fn wants(&mut self, pc: u16) -> bool {
        let mut stop = false;
        if self.link.stop.load(Ordering::Relaxed) {
            self.link.stop.store(false, Ordering::Relaxed);
            stop = true;
        }
        if self.steps > 0 {
            self.steps -= 1;
            stop |= self.steps == 0;
        }
        stop || self.breakpoint(pc) || self.over == Some(pc) || self.frame_ended || self.written.is_some()
    }

    pub fn wrote(&mut self, addr: u16, value: u8) {
        if !self.watches.is_empty() && self.watches.contains(&addr) {
            self.written = Some((addr, value));
        }
    }

    pub fn frame_ended(&mut self) {
        if std::mem::take(&mut self.frame) {
            self.frame_ended = true;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Implied,
    Accumulator,
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    Indirect,
    IndirectX,
    IndirectY,
    Relative,
}
use Mode::*;

/// The NMOS 6502's documented opcodes.
const OPCODES: [(&str, Mode); 256] = [
    ("brk", Implied), ("ora", IndirectX), ("???", Implied), ("???", Implied),
    ("???", Implied), ("ora", ZeroPage), ("asl", ZeroPage), ("???", Implied),
    ("php", Implied), ("ora", Immediate), ("asl", Accumulator), ("???", Implied),
    ("???", Implied), ("ora", Absolute), ("asl", Absolute), ("???", Implied),
    ("bpl", Relative), ("ora", IndirectY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("ora", ZeroPageX), ("asl", ZeroPageX), ("???", Implied),
    ("clc", Implied), ("ora", AbsoluteY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("ora", AbsoluteX), ("asl", AbsoluteX), ("???", Implied),
    ("jsr", Absolute), ("and", IndirectX), ("???", Implied), ("???", Implied),
    ("bit", ZeroPage), ("and", ZeroPage), ("rol", ZeroPage), ("???", Implied),
    ("plp", Implied), ("and", Immediate), ("rol", Accumulator), ("???", Implied),
    ("bit", Absolute), ("and", Absolute), ("rol", Absolute), ("???", Implied),
    ("bmi", Relative), ("and", IndirectY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("and", ZeroPageX), ("rol", ZeroPageX), ("???", Implied),
    ("sec", Implied), ("and", AbsoluteY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("and", AbsoluteX), ("rol", AbsoluteX), ("???", Implied),
    ("rti", Implied), ("eor", IndirectX), ("???", Implied), ("???", Implied),
    ("???", Implied), ("eor", ZeroPage), ("lsr", ZeroPage), ("???", Implied),
    ("pha", Implied), ("eor", Immediate), ("lsr", Accumulator), ("???", Implied),
    ("jmp", Absolute), ("eor", Absolute), ("lsr", Absolute), ("???", Implied),
    ("bvc", Relative), ("eor", IndirectY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("eor", ZeroPageX), ("lsr", ZeroPageX), ("???", Implied),
    ("cli", Implied), ("eor", AbsoluteY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("eor", AbsoluteX), ("lsr", AbsoluteX), ("???", Implied),
    ("rts", Implied), ("adc", IndirectX), ("???", Implied), ("???", Implied),
    ("???", Implied), ("adc", ZeroPage), ("ror", ZeroPage), ("???", Implied),
    ("pla", Implied), ("adc", Immediate), ("ror", Accumulator), ("???", Implied),
    ("jmp", Indirect), ("adc", Absolute), ("ror", Absolute), ("???", Implied),
    ("bvs", Relative), ("adc", IndirectY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("adc", ZeroPageX), ("ror", ZeroPageX), ("???", Implied),
    ("sei", Implied), ("adc", AbsoluteY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("adc", AbsoluteX), ("ror", AbsoluteX), ("???", Implied),
    ("???", Implied), ("sta", IndirectX), ("???", Implied), ("???", Implied),
    ("sty", ZeroPage), ("sta", ZeroPage), ("stx", ZeroPage), ("???", Implied),
    ("dey", Implied), ("???", Implied), ("txa", Implied), ("???", Implied),
    ("sty", Absolute), ("sta", Absolute), ("stx", Absolute), ("???", Implied),
    ("bcc", Relative), ("sta", IndirectY), ("???", Implied), ("???", Implied),
    ("sty", ZeroPageX), ("sta", ZeroPageX), ("stx", ZeroPageY), ("???", Implied),
    ("tya", Implied), ("sta", AbsoluteY), ("txs", Implied), ("???", Implied),
    ("???", Implied), ("sta", AbsoluteX), ("???", Implied), ("???", Implied),
    ("ldy", Immediate), ("lda", IndirectX), ("ldx", Immediate), ("???", Implied),
    ("ldy", ZeroPage), ("lda", ZeroPage), ("ldx", ZeroPage), ("???", Implied),
    ("tay", Implied), ("lda", Immediate), ("tax", Implied), ("???", Implied),
    ("ldy", Absolute), ("lda", Absolute), ("ldx", Absolute), ("???", Implied),
    ("bcs", Relative), ("lda", IndirectY), ("???", Implied), ("???", Implied),
    ("ldy", ZeroPageX), ("lda", ZeroPageX), ("ldx", ZeroPageY), ("???", Implied),
    ("clv", Implied), ("lda", AbsoluteY), ("tsx", Implied), ("???", Implied),
    ("ldy", AbsoluteX), ("lda", AbsoluteX), ("ldx", AbsoluteY), ("???", Implied),
    ("cpy", Immediate), ("cmp", IndirectX), ("???", Implied), ("???", Implied),
    ("cpy", ZeroPage), ("cmp", ZeroPage), ("dec", ZeroPage), ("???", Implied),
    ("iny", Implied), ("cmp", Immediate), ("dex", Implied), ("???", Implied),
    ("cpy", Absolute), ("cmp", Absolute), ("dec", Absolute), ("???", Implied),
    ("bne", Relative), ("cmp", IndirectY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("cmp", ZeroPageX), ("dec", ZeroPageX), ("???", Implied),
    ("cld", Implied), ("cmp", AbsoluteY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("cmp", AbsoluteX), ("dec", AbsoluteX), ("???", Implied),
    ("cpx", Immediate), ("sbc", IndirectX), ("???", Implied), ("???", Implied),
    ("cpx", ZeroPage), ("sbc", ZeroPage), ("inc", ZeroPage), ("???", Implied),
    ("inx", Implied), ("sbc", Immediate), ("nop", Implied), ("???", Implied),
    ("cpx", Absolute), ("sbc", Absolute), ("inc", Absolute), ("???", Implied),
    ("beq", Relative), ("sbc", IndirectY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("sbc", ZeroPageX), ("inc", ZeroPageX), ("???", Implied),
    ("sed", Implied), ("sbc", AbsoluteY), ("???", Implied), ("???", Implied),
    ("???", Implied), ("sbc", AbsoluteX), ("inc", AbsoluteX), ("???", Implied),
];

fn length(mode: Mode) -> u16 {
    match mode {
        Implied | Accumulator => 1,
        Absolute | AbsoluteX | AbsoluteY | Indirect => 3,
        _ => 2,
    }
}

/// The monitor's view of the stopped machine.
struct Session<'a> {
    cpu: &'a mut CPU,
    link: Arc<Link>,
}

impl Session<'_> {
    fn watch(&mut self) -> &mut Watch {
        self.cpu.memory.monitor_watch().expect("a monitor")
    }

    fn peek(&self, addr: u16) -> Option<u8> {
        self.cpu.memory.peek(addr)
    }

    fn byte(&self, addr: u16) -> u8 {
        self.peek(addr).unwrap_or(0)
    }

    fn name(&mut self, addr: u16) -> Option<String> {
        self.watch().labels.names.get(&addr).cloned()
    }

    /// An address as written, `$C012`, with its label if it has one.
    fn address(&mut self, addr: u16, digits: usize) -> String {
        match self.name(addr) {
            Some(name) => name,
            None if digits == 2 => format!("${addr:02X}"),
            None => format!("${addr:04X}"),
        }
    }

    fn parse_address(&mut self, word: &str) -> Result<u16, String> {
        if word == "pc" {
            return Ok(self.cpu.pc);
        }
        self.watch().labels.address(word).ok_or_else(|| format!("{word}: not an address or a label"))
    }

    fn registers(&mut self) -> String {
        let cpu = &*self.cpu;
        let s = cpu.status.to_byte();
        let flags: String =
            "NV-BDIZC".chars().enumerate().map(|(i, c)| if s >> (7 - i) & 1 != 0 { c } else { '.' }).collect();
        let line = self.byte(0xFF1C);
        let (bank0, bank1) = (self.byte(0xFF30), self.byte(0xFF31));
        let (pc, a, x, y, sp) = (cpu.pc, cpu.a, cpu.x, cpu.y, cpu.sp);
        let at = self.name(pc).map(|name| format!(" ({name})")).unwrap_or_default();
        format!(
            "pc ${pc:04X}{at}  a ${a:02X}  x ${x:02X}  y ${y:02X}  sp ${sp:02X}  {flags}  line {line}  banks {bank0} {bank1}\n"
        )
    }

    /// `count` instructions from `addr`, PC marked; and where it ends.
    fn disassemble(&mut self, mut addr: u16, count: usize) -> (String, u16) {
        let mut out = String::new();
        for _ in 0..count {
            if let Some(name) = self.name(addr) {
                out += &format!("{name}:\n");
            }
            let opcode = self.byte(addr);
            let (mnemonic, mode) = OPCODES[opcode as usize];
            let size = length(mode);
            let low = self.byte(addr.wrapping_add(1));
            let word = u16::from_le_bytes([low, self.byte(addr.wrapping_add(2))]);
            let operand = match mode {
                Implied => String::new(),
                Accumulator => "a".into(),
                Immediate => format!("#${low:02X}"),
                ZeroPage => self.address(low as u16, 2),
                ZeroPageX => format!("{},x", self.address(low as u16, 2)),
                ZeroPageY => format!("{},y", self.address(low as u16, 2)),
                Absolute => self.address(word, 4),
                AbsoluteX => format!("{},x", self.address(word, 4)),
                AbsoluteY => format!("{},y", self.address(word, 4)),
                Indirect => format!("({})", self.address(word, 4)),
                IndirectX => format!("({},x)", self.address(low as u16, 2)),
                IndirectY => format!("({}),y", self.address(low as u16, 2)),
                Relative => {
                    let target = addr.wrapping_add(2).wrapping_add(low as i8 as u16);
                    self.address(target, 4)
                }
            };
            let bytes: Vec<String> = (0..size).map(|i| format!("{:02X}", self.byte(addr.wrapping_add(i)))).collect();
            let mark = if addr == self.cpu.pc { '>' } else { ' ' };
            out += &format!("{mark} ${addr:04X}  {:<9} {mnemonic} {operand}\n", bytes.join(" "));
            addr = addr.wrapping_add(size);
        }
        (out, addr)
    }

    fn dump(&mut self, from: u16, to: u16, vram: bool) -> String {
        let mut out = String::new();
        let mut line = from & !15;
        loop {
            let mut hex = String::new();
            let mut text = String::new();
            for i in 0..16u16 {
                let addr = line.wrapping_add(i);
                let value = if addr < from || addr > to {
                    None
                } else if vram {
                    Some(self.cpu.memory.vdp.peek_vram(addr))
                } else {
                    self.peek(addr)
                };
                match value {
                    Some(b) => {
                        hex += &format!("{b:02X} ");
                        text.push(if (0x20..0x7F).contains(&b) { b as char } else { '.' });
                    }
                    None => {
                        hex += if addr >= from && addr <= to { "-- " } else { "   " };
                        text.push(' ');
                    }
                }
            }
            out += &format!("${line:04X}  {hex} {text}\n");
            if line.wrapping_add(15) >= to || line.wrapping_add(16) < line {
                break;
            }
            line = line.wrapping_add(16);
        }
        out
    }

    fn chips(&mut self) -> String {
        let p = |s: &Self, a: u16| s.byte(a);
        let word = |s: &Self, a: u16| u16::from_le_bytes([s.byte(a), s.byte(a + 1)]);
        let mut out = format!(
            "video  vaddr ${:04X} vctrl ${:02X} scroll {},{} b {},{} backdrop {} virq ${:02X} vstatus ${:02X} raster {} line {}\n",
            word(self, 0xFF10),
            p(self, 0xFF13),
            word(self, 0xFF14),
            word(self, 0xFF16),
            word(self, 0xFF1D),
            p(self, 0xFF1F),
            p(self, 0xFF18),
            p(self, 0xFF19),
            p(self, 0xFF1A),
            p(self, 0xFF1B),
            p(self, 0xFF1C)
        );
        out += &format!(
            "cart   banks {} {} ctl ${:02X} save addr ${:04X}    pad ${:04X}    frame {}\n",
            p(self, 0xFF30),
            p(self, 0xFF31),
            p(self, 0xFF32),
            word(self, 0xFF34),
            word(self, 0xFF20),
            p(self, 0xFF01)
        );
        for v in 0..4u16 {
            let at = 0xFF80 + v * 8;
            out += &format!(
                "voice {v} freq ${:04X} pulse {:4} ctrl ${:02X} ad ${:02X} sr ${:02X} volume {}\n",
                word(self, at),
                word(self, at + 2),
                p(self, at + 4),
                p(self, at + 5),
                p(self, at + 6),
                p(self, at + 7)
            );
        }
        out += &format!(
            "filter cutoff {} ${:02X} mode ${:02X}  osc3 ${:02X} env3 ${:02X}\n",
            word(self, 0xFFA0),
            p(self, 0xFFA2),
            p(self, 0xFFA3),
            p(self, 0xFFA4),
            p(self, 0xFFA5)
        );
        out
    }

    /// One command. True when the machine goes on.
    fn run(&mut self, line: &str) -> Result<bool, String> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let arg = |i: usize| words.get(i).copied();
        match words.first().copied().unwrap_or("") {
            "" => {}
            "h" | "?" | "help" => self.link.say(HELP),
            "r" => {
                if let (Some(reg), Some(value)) = (arg(1), arg(2)) {
                    let value = u16::from_str_radix(value.trim_start_matches('$'), 16).map_err(|e| e.to_string())?;
                    let cpu = &mut *self.cpu;
                    match reg {
                        "pc" => cpu.pc = value,
                        "a" => cpu.a = value as u8,
                        "x" => cpu.x = value as u8,
                        "y" => cpu.y = value as u8,
                        "sp" => cpu.sp = value as u8,
                        "p" => cpu.status.from_byte(value as u8),
                        _ => return Err(format!("{reg}: pc, a, x, y, sp or p")),
                    }
                }
                let text = self.registers();
                self.link.say(&text);
            }
            "d" => {
                let from = match arg(1) {
                    Some(word) => self.parse_address(word)?,
                    None => self.cpu.pc,
                };
                let count = arg(2).and_then(|n| n.parse().ok()).unwrap_or(16);
                let (text, _) = self.disassemble(from, count);
                self.link.say(&text);
            }
            "m" | "v" => {
                let from = self.parse_address(arg(1).ok_or("an address")?)?;
                let to = match arg(2) {
                    Some(word) if words[0] == "m" => self.parse_address(word)?,
                    Some(count) => from.saturating_add(count.parse::<u16>().map_err(|e| e.to_string())?.saturating_sub(1)),
                    None => from.saturating_add(127),
                };
                let text = self.dump(from, to, words[0] == "v");
                self.link.say(&text);
            }
            ">" => {
                let mut addr = self.parse_address(arg(1).ok_or("an address")?)?;
                for word in &words[2..] {
                    let value = u8::from_str_radix(word.trim_start_matches('$'), 16).map_err(|e| e.to_string())?;
                    self.cpu.memory.write(addr, value);
                    addr = addr.wrapping_add(1);
                }
            }
            "io" => {
                let text = self.chips();
                self.link.say(&text);
            }
            "b" => match arg(1) {
                Some(word) => {
                    let addr = self.parse_address(word)?;
                    let on = self.watch().toggle_breakpoint(addr);
                    let at = self.address(addr, 4);
                    self.link.say(&format!("{} breakpoint at {at}\n", if on { "A" } else { "No" }));
                }
                None => {
                    let watch = self.watch();
                    let points: Vec<u16> = (0..=0xFFFFu16).filter(|&a| watch.breakpoint(a)).collect();
                    let watches = watch.watches.clone();
                    let mut text = String::new();
                    for a in points {
                        text += &format!("break {}\n", self.address(a, 4));
                    }
                    for a in watches {
                        text += &format!("watch {}\n", self.address(a, 4));
                    }
                    self.link.say(if text.is_empty() { "No breakpoints or watches.\n" } else { &text });
                }
            },
            "w" => {
                let addr = self.parse_address(arg(1).ok_or("an address")?)?;
                let watch = self.watch();
                let on = match watch.watches.iter().position(|&a| a == addr) {
                    Some(i) => {
                        watch.watches.remove(i);
                        false
                    }
                    None => {
                        watch.watches.push(addr);
                        true
                    }
                };
                let at = self.address(addr, 4);
                self.link.say(&format!("{} {at}\n", if on { "Watching" } else { "Not watching" }));
            }
            "bc" => {
                let watch = self.watch();
                watch.breakpoints.fill(0);
                watch.watches.clear();
            }
            "s" => {
                let steps = arg(1).and_then(|n| n.parse().ok()).unwrap_or(1u32).max(1);
                self.watch().steps = steps;
                return Ok(true);
            }
            "n" => {
                let pc = self.cpu.pc;
                if self.byte(pc) == 0x20 {
                    self.watch().over = Some(pc.wrapping_add(3));
                } else {
                    self.watch().steps = 1;
                }
                return Ok(true);
            }
            "f" => {
                self.watch().frame = true;
                return Ok(true);
            }
            "g" => {
                if let Some(word) = arg(1) {
                    self.cpu.pc = self.parse_address(word)?;
                }
                return Ok(true);
            }
            "l" => {
                let name = arg(1).ok_or("a cart's name, or a label file")?;
                let path = self
                    .link
                    .drive
                    .as_ref()
                    .map(|drive| drive.join(format!("{name}.lbl")))
                    .filter(|path| path.is_file())
                    .unwrap_or_else(|| PathBuf::from(name));
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let watch = self.watch();
                let mut labels = Labels::default();
                labels.load(SYSTEM_LABELS);
                labels.load(&text);
                let count = labels.names.len() - watch.rom_labels.names.len().min(labels.names.len());
                watch.labels = labels;
                self.link.say(&format!("{count} labels from {}\n", path.display()));
            }
            "q" => {
                let watch = self.watch();
                watch.breakpoints.fill(0);
                watch.watches.clear();
                return Ok(true);
            }
            other => return Err(format!("{other}: not a command; h lists them")),
        }
        Ok(false)
    }
}

/// The CPU stopped for the monitor: say why and where, then take commands
/// until one sends it on (or the client goes).
pub fn session(cpu: &mut CPU) {
    let link = Arc::clone(&cpu.memory.monitor_watch().expect("a monitor").link);
    let mut session = Session { cpu, link: Arc::clone(&link) };
    let pc = session.cpu.pc;
    let watch = session.watch();
    let why = if let Some((addr, value)) = watch.written.take() {
        format!("${value:02X} written to ${addr:04X}")
    } else if watch.breakpoint(pc) {
        "Breakpoint".into()
    } else if std::mem::take(&mut watch.frame_ended) {
        "A new frame".into()
    } else {
        "Stopped".into()
    };
    watch.steps = 0;
    watch.over = None;
    watch.frame = false;
    let registers = session.registers();
    let (code, _) = session.disassemble(pc, 1);
    link.say(&format!("{why}.\n{registers}{code}"));
    loop {
        link.say("(monitor) ");
        let Some(line) = link.next_command() else {
            // The client went: the machine goes on as if it never came.
            let watch = session.watch();
            watch.breakpoints.fill(0);
            watch.watches.clear();
            break;
        };
        match session.run(&line) {
            Ok(true) => break,
            Ok(false) => {}
            Err(error) => link.say(&format!("{error}\n")),
        }
    }
    link.stop.store(false, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_from_ld65_leave_out_the_numbered_and_the_linkers() {
        let mut labels = Labels::default();
        labels.load("al 00C012 .waitframe\nal 00C012 ._bios_waitframe\nal 001234 .L001F\nal 000300 .__RAM_START__\n");
        assert_eq!(labels.names.get(&0xC012).map(String::as_str), Some("_bios_waitframe"), "C's name over the assembler's");
        assert_eq!(labels.address("bios_waitframe"), Some(0xC012), "the C name without its _");
        assert_eq!(labels.address("$c000"), Some(0xC000));
        assert!(labels.names.get(&0x1234).is_none() && labels.names.get(&0x0300).is_none());
    }

    #[test]
    fn the_opcode_table_has_the_documented_151() {
        assert_eq!(OPCODES.iter().filter(|(name, _)| *name != "???").count(), 151);
        assert_eq!(OPCODES[0x20], ("jsr", Absolute));
        assert_eq!(OPCODES[0xB1], ("lda", IndirectY));
        assert_eq!(OPCODES[0x6C], ("jmp", Indirect));
    }
}
