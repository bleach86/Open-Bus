//! A 16550 UART, as each of a TL16C2550's two channels is: a serial port at
//! 8 registers, its baud rate its 14.7456 MHz crystal's over 16 and its
//! divisor, run with the CPU's cycles. Its other end is the host's: what's
//! to come in, `incoming`, a byte at a time as fast as its baud rate lets
//! it, and only while RTS is up if the other end's to wait for it
//! (`flow`); what's gone out, `outgoing`; and the other end's lines, CTS,
//! DSR, RI and DCD (`set_lines`).
//!
//! Its registers: 0, the byte received, read (RBR), or to send, written
//! (THR); with LCR's bit 7 (DLAB) set, the divisor's low byte (DLL). 1,
//! the interrupts let in (IER): bit 0 a byte received, 1 the transmitter
//! empty, 2 the receiver's errors, 3 the modem's lines changed; with DLAB,
//! the divisor's high byte (DLM). 2, read, which interrupt it is (IIR):
//! bit 0 none, bits 3 to 1 the most pressing, `$06` an overrun, `$04` the
//! bytes received at the FIFO's trigger, `$0C` some waiting, none come
//! for 4 bytes' time, `$02` the transmitter empty, `$00` a line changed;
//! bits 7 and 6 set with the FIFOs on. Written, the FIFOs' control (FCR):
//! bit 0 on, 1 and 2 the receiver's and the transmitter's emptied, 7 and
//! 6 the receiver's trigger, 1, 4, 8 or 14 bytes. 3, the line's control
//! (LCR): bits 1 and 0 the data bits, 5 to 8; 2 the stop bits; 3 parity;
//! 7 DLAB. 4, the modem's control (MCR): bit 0 DTR, 1 RTS, 2 OUT1, 3 OUT2,
//! 4 loopback, 5 auto flow control (AFE), RTS dropped as the received
//! bytes reach the trigger, and nothing sent while CTS is down. 5, the
//! line's status (LSR): bit 0 a byte received, 1 an overrun (a byte come
//! with the receiver full, and lost), cleared as it's read; 5 the
//! transmitter's holding register (or FIFO) empty, 6 that and its shift
//! register both. 6, the modem's status (MSR): bits 4 to 7 CTS, DSR, RI,
//! DCD, set while they're up; 0 to 3 CTS, DSR and DCD changed, and RI
//! ended, since it was last read, which clears them. 7, a byte of scratch.
//!
//! In loopback, what's sent comes back in, and the lines are MCR's: RTS's
//! CTS, DTR's DSR, OUT1's RI, OUT2's DCD. The other end never errs, so
//! there's no parity, framing or break error.

use std::collections::VecDeque;

/// Its crystal, over 16 a divisor of 1's baud rate, 921,600.
const CRYSTAL_HZ: u64 = 14_745_600;
const FIFO: usize = 16;

/// IER's interrupts.
const RECEIVED: u8 = 0x01;
const EMPTIED: u8 = 0x02;
const ERRORS: u8 = 0x04;
const LINES: u8 = 0x08;

/// MCR's bits.
const DTR: u8 = 0x01;
const RTS: u8 = 0x02;
const OUT1: u8 = 0x04;
const OUT2: u8 = 0x08;
const LOOPBACK: u8 = 0x10;
const AFE: u8 = 0x20;

/// MSR's lines, and LCR's DLAB.
const CTS: u8 = 0x10;
const DSR: u8 = 0x20;
const RI: u8 = 0x40;
const DCD: u8 = 0x80;
const DLAB: u8 = 0x80;

#[derive(Clone, Debug)]
pub struct Uart16550 {
    divisor_low: u8,
    divisor_high: u8,
    ier: u8,
    lcr: u8,
    mcr: u8,
    scratch: u8,
    fifos: bool,
    trigger: usize,
    received: VecDeque<u8>,
    sending: VecDeque<u8>,
    /// The byte being sent, and its cycles still to go.
    shifting: Option<(u8, u64)>,
    /// The cycles till the byte coming in from the other end is in.
    receiving: Option<u64>,
    overrun: bool,
    /// The transmitter's interrupt, since it emptied: till IIR's read
    /// with it the one it says, or a byte's written.
    emptied: bool,
    /// Cycles since a byte was taken from the receiver or came into it.
    idle: u64,
    /// The other end's lines, as MSR has them, and their changes.
    lines: u8,
    changes: u8,
    clock_hz: u32,
    /// What's come from the other end, to be received.
    pub incoming: VecDeque<u8>,
    /// What's been sent.
    pub outgoing: Vec<u8>,
    /// Whether the other end waits for RTS to send, as a modem with
    /// hardware flow control does; else it's received as it comes.
    pub flow: bool,
}

impl Uart16550 {
    /// One clocked with a CPU of `clock_hz`, as it's reset.
    pub fn new(clock_hz: u32) -> Uart16550 {
        Uart16550 {
            divisor_low: 0,
            divisor_high: 0,
            ier: 0,
            lcr: 0,
            mcr: 0,
            scratch: 0,
            fifos: false,
            trigger: 1,
            received: VecDeque::new(),
            sending: VecDeque::new(),
            shifting: None,
            receiving: None,
            overrun: false,
            emptied: false,
            idle: 0,
            lines: 0,
            changes: 0,
            clock_hz,
            incoming: VecDeque::new(),
            outgoing: Vec::new(),
            flow: false,
        }
    }

    /// Reset, as its RESET pin has it: its registers, but for the scratch
    /// and the divisor; what it had to send, and had received, gone.
    pub fn reset(&mut self) {
        let (low, high, scratch) = (self.divisor_low, self.divisor_high, self.scratch);
        let (incoming, lines, flow) = (std::mem::take(&mut self.incoming), self.lines, self.flow);
        *self = Uart16550::new(self.clock_hz);
        (self.divisor_low, self.divisor_high, self.scratch) = (low, high, scratch);
        (self.incoming, self.lines, self.flow) = (incoming, lines, flow);
    }

    fn divisor(&self) -> u64 {
        match (self.divisor_high as u64) << 8 | self.divisor_low as u64 {
            0 => 0x1_0000,
            divisor => divisor,
        }
    }

    /// Its baud rate, as it's set.
    pub fn baud(&self) -> u32 {
        (CRYSTAL_HZ / 16 / self.divisor()) as u32
    }

    /// The CPU's cycles a byte takes: its start bit, data bits, parity
    /// and stop bits, at its rate.
    fn byte_cycles(&self) -> u64 {
        let data = 5 + (self.lcr & 3) as u64;
        let parity = (self.lcr >> 3 & 1) as u64;
        let stop = if self.lcr & 4 != 0 { 2 } else { 1 };
        let bits = 1 + data + parity + stop;
        (self.clock_hz as u64 * bits * 16 * self.divisor()).div_ceil(CRYSTAL_HZ)
    }

    fn room(&self) -> usize {
        if self.fifos { FIFO } else { 1 }
    }

    /// DTR, as MCR has it.
    pub fn dtr(&self) -> bool {
        self.mcr & DTR != 0
    }

    /// RTS, as the other end sees it: MCR's, dropped by auto flow control
    /// as the bytes received reach the trigger.
    pub fn rts(&self) -> bool {
        let full = self.mcr & AFE != 0 && self.received.len() >= self.trigger;
        self.mcr & RTS != 0 && !full && self.mcr & LOOPBACK == 0
    }

    /// The other end's lines: CTS, DSR, RI and DCD, up or not.
    pub fn set_lines(&mut self, cts: bool, dsr: bool, ri: bool, dcd: bool) {
        let before = self.msr_lines();
        self.lines = (cts as u8) << 4 | (dsr as u8) << 5 | (ri as u8) << 6 | (dcd as u8) << 7;
        self.changed(before);
    }

    /// MSR's changes, its lines as they were `before`: CTS, DSR and DCD
    /// changed, RI ended.
    fn changed(&mut self, before: u8) {
        let after = self.msr_lines();
        let changed = before ^ after;
        self.changes |= (changed & CTS != 0) as u8
            | ((changed & DSR != 0) as u8) << 1
            | ((changed & RI != 0 && after & RI == 0) as u8) << 2
            | ((changed & DCD != 0) as u8) << 3;
    }

    /// The lines as MSR reads them: MCR's, in loopback.
    fn msr_lines(&self) -> u8 {
        if self.mcr & LOOPBACK == 0 {
            return self.lines;
        }
        let from = |bit: u8, line: u8| if self.mcr & bit != 0 { line } else { 0 };
        from(RTS, CTS) | from(DTR, DSR) | from(OUT1, RI) | from(OUT2, DCD)
    }

    /// A byte into the receiver: lost, and an overrun, if it's full.
    fn receive(&mut self, byte: u8) {
        if self.received.len() < self.room() {
            self.received.push_back(byte);
        } else {
            self.overrun = true;
        }
        self.idle = 0;
    }

    /// Whether the other end's bytes are let in: RTS up, if it waits for it.
    fn taking(&self) -> bool {
        !self.incoming.is_empty() && self.mcr & LOOPBACK == 0 && (!self.flow || self.rts())
    }

    /// Whether the transmitter's to wait: auto flow control, CTS down.
    fn held(&self) -> bool {
        self.mcr & AFE != 0 && self.mcr & LOOPBACK == 0 && self.lines & CTS == 0
    }

    /// The next byte to send into the shift register, if there's one and
    /// it may go: the transmitter's interrupt, as it empties.
    fn start_sending(&mut self) {
        if self.shifting.is_some() || self.held() {
            return;
        }
        if let Some(byte) = self.sending.pop_front() {
            self.shifting = Some((byte, self.byte_cycles()));
            if self.sending.is_empty() {
                self.emptied = true;
            }
        }
    }

    /// `cycles` gone by: what's being sent sent, and what's coming in
    /// received, at its rate.
    pub fn run(&mut self, mut cycles: u64) {
        while cycles > 0 {
            self.start_sending();
            let sending = self.shifting.map_or(u64::MAX, |(_, left)| left);
            let receiving = if self.taking() {
                Some(self.receiving.unwrap_or_else(|| self.byte_cycles()))
            } else {
                None
            };
            let step = cycles.min(sending).min(receiving.unwrap_or(u64::MAX));
            cycles -= step;
            self.idle = self.idle.saturating_add(step);
            if let Some((byte, left)) = &mut self.shifting {
                *left -= step;
                if *left == 0 {
                    let byte = *byte;
                    self.shifting = None;
                    if self.mcr & LOOPBACK != 0 {
                        self.receive(byte);
                    } else {
                        self.outgoing.push(byte);
                    }
                }
            }
            self.receiving = receiving.map(|left| left - step);
            if self.receiving == Some(0) {
                self.receiving = None;
                let byte = self.incoming.pop_front().unwrap();
                self.receive(byte);
            }
        }
        self.start_sending();
    }

    /// The cycles until it's next to do something: a byte sent or
    /// received, or the receiver's wait for more timed out.
    pub fn due(&self) -> u64 {
        let mut due = u64::MAX;
        if let Some((_, left)) = self.shifting {
            due = left;
        } else if !self.sending.is_empty() && !self.held() {
            due = 1;
        }
        if self.taking() {
            due = due.min(self.receiving.unwrap_or_else(|| self.byte_cycles()));
        }
        if self.fifos && self.ier & RECEIVED != 0 && !self.received.is_empty() {
            let timeout = self.byte_cycles() * 4;
            if self.idle < timeout {
                due = due.min(timeout - self.idle);
            }
        }
        due
    }

    /// The interrupt it's asking for, as IIR has it, the most pressing:
    /// none, `$01`.
    fn interrupt(&self) -> u8 {
        let received = !self.received.is_empty();
        if self.ier & ERRORS != 0 && self.overrun {
            0x06
        } else if self.ier & RECEIVED != 0
            && received
            && (!self.fifos || self.received.len() >= self.trigger)
        {
            0x04
        } else if self.ier & RECEIVED != 0
            && self.fifos
            && received
            && self.idle >= self.byte_cycles() * 4
        {
            0x0C
        } else if self.ier & EMPTIED != 0 && self.emptied {
            0x02
        } else if self.ier & LINES != 0 && self.changes != 0 {
            0x00
        } else {
            0x01
        }
    }

    /// Whether it's holding its interrupt line.
    pub fn irq(&self) -> bool {
        self.interrupt() != 0x01
    }

    /// Register `reg`, 0 to 7, read.
    pub fn read(&mut self, reg: u8) -> u8 {
        let dlab = self.lcr & DLAB != 0;
        match reg & 7 {
            0 if dlab => self.divisor_low,
            0 => {
                self.idle = 0;
                self.received.pop_front().unwrap_or(0)
            }
            1 if dlab => self.divisor_high,
            1 => self.ier,
            2 => {
                let interrupt = self.interrupt();
                if interrupt == 0x02 {
                    self.emptied = false;
                }
                interrupt | if self.fifos { 0xC0 } else { 0 }
            }
            3 => self.lcr,
            4 => self.mcr,
            5 => {
                let empty = self.sending.is_empty();
                let status = !self.received.is_empty() as u8
                    | (self.overrun as u8) << 1
                    | (empty as u8) << 5
                    | ((empty && self.shifting.is_none()) as u8) << 6;
                self.overrun = false;
                status
            }
            6 => self.msr_lines() | std::mem::take(&mut self.changes),
            _ => self.scratch,
        }
    }

    /// Register `reg`, 0 to 7, written.
    pub fn write(&mut self, reg: u8, value: u8) {
        let dlab = self.lcr & DLAB != 0;
        match reg & 7 {
            0 if dlab => self.divisor_low = value,
            0 => {
                if self.sending.len() < self.room() {
                    self.sending.push_back(value);
                }
                self.emptied = false;
                self.start_sending();
            }
            1 if dlab => self.divisor_high = value,
            1 => {
                // The transmitter's interrupt let in while it's empty: one,
                // at once.
                if value & EMPTIED != 0 && self.ier & EMPTIED == 0 && self.sending.is_empty() {
                    self.emptied = true;
                }
                self.ier = value & 0x0F;
            }
            2 => {
                let on = value & 1 != 0;
                if on != self.fifos || value & 2 != 0 {
                    self.received.clear();
                }
                if on != self.fifos || value & 4 != 0 {
                    self.sending.clear();
                }
                self.fifos = on;
                self.trigger = if on {
                    [1, 4, 8, 14][(value >> 6) as usize]
                } else {
                    1
                };
            }
            3 => self.lcr = value,
            4 => {
                let before = self.msr_lines();
                self.mcr = value & 0x3F;
                self.changed(before);
            }
            5 | 6 => {}
            _ => self.scratch = value,
        }
    }
}
