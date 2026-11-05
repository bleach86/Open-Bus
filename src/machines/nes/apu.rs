//! The APU, the 2A03's sound: two pulse channels, a triangle, noise, and
//! the DMC, its samples read from memory; a frame counter clocking their
//! envelopes, sweeps and length counters, and its IRQ; their outputs mixed
//! as the 2A03's DACs mix them, not in a line.
//!
//! Each channel's timer counts CPU cycles down to its next step: a pulse's
//! its period's twice, (t + 1) x 2, a step of its 8 of its duty's wave; the
//! triangle's its period, (t + 1), a step of its 32, up and down; noise's
//! from its table, a shift of its 15-bit shift register, bit 0 and bit 1
//! (or 6, its short mode) XORed in at the top, bit 0 set its silence; the
//! DMC's from its table, a bit of its sample byte, its 7-bit output 2 up or
//! down.
//!
//! The frame counter, in CPU cycles from its start: in mode 0, a quarter
//! frame at 7457 and 22371, a half (a quarter too) at 14913 and 29829, its
//! IRQ flag set at 29828, 29829 and 29830, then over; in mode 1, the
//! quarters and halves at 7457, 14913, 22371 and 37281, no IRQ. A quarter
//! clocks the envelopes and the triangle's linear counter; a half, the
//! sweeps and the length counters too. Written, $4017 starts it over 3 or
//! 4 cycles later, as the write is in an APU cycle or between; mode 1
//! clocks a half then at once.
//!
//! A length counter, loaded from its table by a channel's fourth register
//! (if the channel's enabled in $4015), counts down each half frame unless
//! halted, silencing the channel at 0. A load in the very cycle a half
//! frame clocks it is ignored if the clock took it from what it was; a halt
//! takes effect after the cycle's clock.
//!
//! The DMC reads its sample a byte at a time by DMA, the CPU halted on a
//! read for 3 or 4 cycles: when its buffer is empty and there's more of the
//! sample, from $C000 + 64 x its address register, 16 x its length's + 1
//! bytes, past $FFFF on at $8000; then over, if it loops, or its IRQ flag
//! set if that's enabled.

use serde::{Deserialize, Serialize};

const LENGTHS: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
const DUTIES: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1, 0, 0, 0],
    [1, 0, 0, 1, 1, 1, 1, 1],
];
const TRIANGLE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
];
const NOISE_PERIODS: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];
const DMC_RATES: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];

/// The frame counter's steps, in CPU cycles from its start, mode 0's and
/// mode 1's; and what each clocks: a quarter frame, a half, or nothing.
const STEPS: [[u32; 6]; 2] = [
    [7457, 14913, 22371, 29828, 29829, 29830],
    [7457, 14913, 22371, 29829, 37281, 37282],
];
#[derive(Clone, Copy, PartialEq)]
enum Frame {
    Quarter,
    Half,
    None,
}
const FRAMES: [Frame; 6] = [
    Frame::Quarter,
    Frame::Half,
    Frame::Quarter,
    Frame::None,
    Frame::Half,
    Frame::None,
];

/// A length counter, and its load and halt to come, at the cycle's end.
#[derive(Default, Serialize, Deserialize)]
struct Length {
    counter: u8,
    halt: bool,
    new_halt: bool,
    enabled: bool,
    reload: u8,
    before_reload: u8,
}

impl Length {
    fn load(&mut self, index: u8) {
        if self.enabled {
            self.reload = LENGTHS[index as usize];
            self.before_reload = self.counter;
        }
    }

    fn clock(&mut self) {
        if self.counter > 0 && !self.halt {
            self.counter -= 1;
        }
    }

    /// The cycle's end: a load made, unless a clock took the counter from
    /// what it was when the load was written; the halt as written.
    fn settle(&mut self) {
        if self.reload != 0 {
            if self.counter == self.before_reload {
                self.counter = self.reload;
            }
            self.reload = 0;
        }
        self.halt = self.new_halt;
    }

    fn enable(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.counter = 0;
        }
    }
}

/// An envelope: its volume, or its decay from 15 down, a step each time
/// its divider counts through the volume, over again if it loops.
#[derive(Default, Serialize, Deserialize)]
struct Envelope {
    constant: bool,
    looping: bool,
    volume: u8,
    start: bool,
    divider: u8,
    decay: u8,
}

impl Envelope {
    fn write(&mut self, value: u8) {
        self.looping = value & 0x20 != 0;
        self.constant = value & 0x10 != 0;
        self.volume = value & 0x0F;
    }

    fn clock(&mut self) {
        if self.start {
            self.start = false;
            self.decay = 15;
            self.divider = self.volume;
        } else if self.divider == 0 {
            self.divider = self.volume;
            if self.decay > 0 {
                self.decay -= 1;
            } else if self.looping {
                self.decay = 15;
            }
        } else {
            self.divider -= 1;
        }
    }

    fn output(&self) -> u8 {
        if self.constant {
            self.volume
        } else {
            self.decay
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Pulse {
    /// Pulse 1's sweep negates in ones' complement, 2's in two's.
    second: bool,
    duty: u8,
    step: u8,
    period: u16,
    timer: u16,
    envelope: Envelope,
    length: Length,
    sweep_enabled: bool,
    sweep_period: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_reload: bool,
    sweep_divider: u8,
}

impl Pulse {
    fn write(&mut self, register: u16, value: u8) {
        match register & 3 {
            0 => {
                self.duty = value >> 6;
                self.length.new_halt = value & 0x20 != 0;
                self.envelope.write(value);
            }
            1 => {
                self.sweep_enabled = value & 0x80 != 0;
                self.sweep_period = (value >> 4) & 7;
                self.sweep_negate = value & 0x08 != 0;
                self.sweep_shift = value & 7;
                self.sweep_reload = true;
            }
            2 => self.period = (self.period & 0x700) | value as u16,
            _ => {
                self.period = (self.period & 0xFF) | (value as u16 & 7) << 8;
                self.length.load(value >> 3);
                self.step = 0;
                self.envelope.start = true;
            }
        }
    }

    /// The period the sweep would change it to.
    fn sweep_target(&self) -> u16 {
        let change = self.period >> self.sweep_shift;
        if !self.sweep_negate {
            self.period + change
        } else if self.second {
            self.period.saturating_sub(change)
        } else {
            self.period.saturating_sub(change + 1)
        }
    }

    /// Silent, its period too short, or the sweep's to come too long.
    fn muted(&self) -> bool {
        self.period < 8 || (!self.sweep_negate && self.sweep_target() > 0x7FF)
    }

    fn half_frame(&mut self) {
        if self.sweep_divider == 0 && self.sweep_enabled && self.sweep_shift > 0 && !self.muted() {
            self.period = self.sweep_target();
        }
        if self.sweep_divider == 0 || self.sweep_reload {
            self.sweep_divider = self.sweep_period;
            self.sweep_reload = false;
        } else {
            self.sweep_divider -= 1;
        }
        self.length.clock();
    }

    fn cycle(&mut self) {
        if self.timer == 0 {
            self.timer = (self.period + 1) * 2 - 1;
            self.step = (self.step + 7) & 7;
        } else {
            self.timer -= 1;
        }
    }

    fn output(&self) -> u8 {
        if self.muted()
            || self.length.counter == 0
            || DUTIES[self.duty as usize][self.step as usize] == 0
        {
            0
        } else {
            self.envelope.output()
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Triangle {
    period: u16,
    timer: u16,
    step: u8,
    length: Length,
    linear: u8,
    linear_reload: u8,
    linear_reloading: bool,
    control: bool,
}

impl Triangle {
    fn write(&mut self, register: u16, value: u8) {
        match register & 3 {
            0 => {
                self.control = value & 0x80 != 0;
                self.length.new_halt = self.control;
                self.linear_reload = value & 0x7F;
            }
            1 => {}
            2 => self.period = (self.period & 0x700) | value as u16,
            _ => {
                self.period = (self.period & 0xFF) | (value as u16 & 7) << 8;
                self.length.load(value >> 3);
                self.linear_reloading = true;
            }
        }
    }

    fn quarter_frame(&mut self) {
        if self.linear_reloading {
            self.linear = self.linear_reload;
        } else if self.linear > 0 {
            self.linear -= 1;
        }
        if !self.control {
            self.linear_reloading = false;
        }
    }

    fn cycle(&mut self) {
        if self.timer == 0 {
            self.timer = self.period;
            if self.length.counter > 0 && self.linear > 0 {
                self.step = (self.step + 1) & 31;
            }
        } else {
            self.timer -= 1;
        }
    }

    fn output(&self) -> u8 {
        TRIANGLE[self.step as usize]
    }
}

#[derive(Serialize, Deserialize)]
struct Noise {
    short: bool,
    period: u16,
    timer: u16,
    shift: u16,
    envelope: Envelope,
    length: Length,
}

impl Noise {
    fn write(&mut self, register: u16, value: u8) {
        match register & 3 {
            0 => {
                self.length.new_halt = value & 0x20 != 0;
                self.envelope.write(value);
            }
            1 => {}
            2 => {
                self.short = value & 0x80 != 0;
                self.period = NOISE_PERIODS[value as usize & 0x0F];
            }
            _ => {
                self.length.load(value >> 3);
                self.envelope.start = true;
            }
        }
    }

    fn cycle(&mut self) {
        if self.timer == 0 {
            self.timer = self.period - 1;
            let tap = if self.short { 6 } else { 1 };
            let feedback = (self.shift ^ (self.shift >> tap)) & 1;
            self.shift = (self.shift >> 1) | feedback << 14;
        } else {
            self.timer -= 1;
        }
    }

    fn output(&self) -> u8 {
        if self.shift & 1 != 0 || self.length.counter == 0 {
            0
        } else {
            self.envelope.output()
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Dmc {
    irq_enabled: bool,
    looping: bool,
    rate: u16,
    timer: u16,
    output: u8,
    sample_address: u16,
    sample_length: u16,
    /// Where it's reading the sample, and how much is left.
    address: u16,
    remaining: u16,
    buffer: Option<u8>,
    shift: u8,
    bits: u8,
    silent: bool,
    irq: bool,
}

impl Dmc {
    fn write(&mut self, register: u16, value: u8) {
        match register & 3 {
            0 => {
                self.irq_enabled = value & 0x80 != 0;
                if !self.irq_enabled {
                    self.irq = false;
                }
                self.looping = value & 0x40 != 0;
                self.rate = DMC_RATES[value as usize & 0x0F];
            }
            1 => self.output = value & 0x7F,
            2 => self.sample_address = 0xC000 | (value as u16) << 6,
            _ => self.sample_length = (value as u16) << 4 | 1,
        }
    }

    fn restart(&mut self) {
        self.address = self.sample_address;
        self.remaining = self.sample_length;
    }

    fn cycle(&mut self) {
        if self.timer > 0 {
            self.timer -= 1;
            return;
        }
        self.timer = self.rate - 1;
        if !self.silent {
            if self.shift & 1 != 0 {
                if self.output <= 125 {
                    self.output += 2;
                }
            } else if self.output >= 2 {
                self.output -= 2;
            }
        }
        self.shift >>= 1;
        self.bits -= 1;
        if self.bits == 0 {
            self.bits = 8;
            match self.buffer.take() {
                Some(byte) => {
                    self.silent = false;
                    self.shift = byte;
                }
                None => self.silent = true,
            }
        }
    }

    /// The address of the byte it wants read, if it does: its buffer
    /// empty, and more of the sample.
    fn wants(&self) -> Option<u16> {
        (self.buffer.is_none() && self.remaining > 0).then_some(self.address)
    }

    /// The byte read for it, by DMA.
    fn fill(&mut self, byte: u8) {
        self.buffer = Some(byte);
        self.address = if self.address == 0xFFFF {
            0x8000
        } else {
            self.address + 1
        };
        self.remaining -= 1;
        if self.remaining == 0 {
            if self.looping {
                self.restart();
            } else if self.irq_enabled {
                self.irq = true;
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Apu {
    pulses: [Pulse; 2],
    triangle: Triangle,
    noise: Noise,
    dmc: Dmc,
    // The frame counter: its mode, its IRQ inhibited, its flag; its cycles
    // and step; a write of $4017 to come, and the cycles till it does; and
    // the cycles a clock blocks another.
    mode: u8,
    irq_inhibit: bool,
    frame_irq: bool,
    frame_cycle: u32,
    frame_step: usize,
    written: Option<u8>,
    write_delay: u8,
    block: u8,
    /// The last value written to $4017, for RESET.
    last_4017: u8,
}

impl Apu {
    pub fn new() -> Self {
        let mut apu = Apu {
            pulses: [
                Pulse::default(),
                Pulse {
                    second: true,
                    ..Pulse::default()
                },
            ],
            triangle: Triangle::default(),
            noise: Noise {
                short: false,
                period: NOISE_PERIODS[0],
                timer: 0,
                shift: 1,
                envelope: Envelope::default(),
                length: Length::default(),
            },
            dmc: Dmc {
                irq_enabled: false,
                looping: false,
                rate: DMC_RATES[0],
                timer: DMC_RATES[0] - 1,
                output: 0,
                sample_address: 0xC000,
                sample_length: 1,
                address: 0xC000,
                remaining: 0,
                buffer: None,
                shift: 0,
                bits: 8,
                silent: true,
                irq: false,
            },
            mode: 0,
            irq_inhibit: false,
            frame_irq: false,
            frame_cycle: 0,
            frame_step: 0,
            written: None,
            write_delay: 0,
            block: 0,
            last_4017: 0,
        };
        // At power-on, as if $4017 had been written 0 9 to 12 cycles before
        // the CPU's first instruction, its reset's 7 cycles before it: so 2
        // cycles before them.
        apu.write_frame_counter(0, false);
        apu.frame_counter();
        apu.frame_counter();
        apu
    }

    /// RESET, held: the channels disabled, the pulses' and noise's length
    /// counters cleared and not halted, the triangle's as they are; the
    /// DMC stopped; the IRQ flags cleared; the frame counter to start over
    /// as $4017 was last written, once it's let go.
    pub fn reset(&mut self) {
        let [first, second] = &mut self.pulses;
        for length in [
            &mut first.length,
            &mut second.length,
            &mut self.noise.length,
        ] {
            *length = Length::default();
        }
        self.triangle.length.enabled = false;
        self.dmc.remaining = 0;
        self.dmc.irq = false;
        self.frame_irq = false;
        self.write_frame_counter(self.last_4017, false);
    }

    /// The IRQ line: the frame counter's, the DMC's.
    pub fn irq(&self) -> bool {
        self.frame_irq || self.dmc.irq
    }

    /// $4015 read: each length counter's not 0, the DMC's sample not done,
    /// the frame and DMC IRQ flags; bit 5 the bus's, `open_bus`. The frame
    /// IRQ flag cleared.
    pub fn status(&mut self, open_bus: u8) -> u8 {
        let lengths = [
            &self.pulses[0].length,
            &self.pulses[1].length,
            &self.triangle.length,
            &self.noise.length,
        ];
        let mut status = lengths.iter().enumerate().fold(0, |status, (n, length)| {
            status | ((length.counter > 0) as u8) << n
        });
        status |= ((self.dmc.remaining > 0) as u8) << 4
            | (self.frame_irq as u8) << 6
            | (self.dmc.irq as u8) << 7;
        self.frame_irq = false;
        status | (open_bus & 0x20)
    }

    /// A register written, $4000 to $4017 but $4014 and $4016; `odd`, the
    /// write's cycle.
    pub fn write(&mut self, addr: u16, value: u8, odd: bool) {
        match addr {
            0x4000..=0x4003 => self.pulses[0].write(addr, value),
            0x4004..=0x4007 => self.pulses[1].write(addr, value),
            0x4008..=0x400B => self.triangle.write(addr, value),
            0x400C..=0x400F => self.noise.write(addr, value),
            0x4010..=0x4013 => self.dmc.write(addr, value),
            0x4015 => {
                self.pulses[0].length.enable(value & 0x01 != 0);
                self.pulses[1].length.enable(value & 0x02 != 0);
                self.triangle.length.enable(value & 0x04 != 0);
                self.noise.length.enable(value & 0x08 != 0);
                self.dmc.irq = false;
                if value & 0x10 == 0 {
                    self.dmc.remaining = 0;
                } else if self.dmc.remaining == 0 {
                    self.dmc.restart();
                }
            }
            0x4017 => self.write_frame_counter(value, odd),
            _ => {}
        }
    }

    /// $4017 written: its IRQ inhibit at once, the flag cleared with it;
    /// the rest 3 cycles on, or 4 if `odd`.
    fn write_frame_counter(&mut self, value: u8, odd: bool) {
        self.last_4017 = value;
        self.irq_inhibit = value & 0x40 != 0;
        if self.irq_inhibit {
            self.frame_irq = false;
        }
        self.written = Some(value);
        self.write_delay = if odd { 4 } else { 3 };
    }

    fn quarter_frame(&mut self) {
        self.pulses[0].envelope.clock();
        self.pulses[1].envelope.clock();
        self.noise.envelope.clock();
        self.triangle.quarter_frame();
    }

    fn half_frame(&mut self) {
        self.quarter_frame();
        self.pulses[0].half_frame();
        self.pulses[1].half_frame();
        self.triangle.length.clock();
        self.noise.length.clock();
    }

    /// A CPU cycle: the frame counter, the channels' timers; then the
    /// length counters' loads and halts.
    pub fn cycle(&mut self) {
        self.frame_counter();
        for pulse in &mut self.pulses {
            pulse.cycle();
        }
        self.triangle.cycle();
        self.noise.cycle();
        self.dmc.cycle();
        let [first, second] = &mut self.pulses;
        for length in [
            &mut first.length,
            &mut second.length,
            &mut self.triangle.length,
            &mut self.noise.length,
        ] {
            length.settle();
        }
    }

    fn frame_counter(&mut self) {
        self.frame_cycle += 1;
        let mode = self.mode as usize;
        if self.frame_cycle == STEPS[mode][self.frame_step] {
            if mode == 0 && self.frame_step >= 3 && !self.irq_inhibit {
                self.frame_irq = true;
            }
            let frame = FRAMES[self.frame_step];
            if frame != Frame::None && self.block == 0 {
                if frame == Frame::Half {
                    self.half_frame()
                } else {
                    self.quarter_frame()
                }
                self.block = 2;
            }
            self.frame_step += 1;
            if self.frame_step == 6 {
                self.frame_step = 0;
                self.frame_cycle = 0;
            }
        }
        if let Some(value) = self.written {
            self.write_delay -= 1;
            if self.write_delay == 0 {
                self.mode = value >> 7;
                self.written = None;
                self.frame_step = 0;
                self.frame_cycle = 0;
                if self.mode == 1 && self.block == 0 {
                    self.half_frame();
                    self.block = 2;
                }
            }
        }
        self.block = self.block.saturating_sub(1);
    }

    /// The address the DMC wants read, if it does.
    pub fn dmc_wants(&self) -> Option<u16> {
        self.dmc.wants()
    }

    /// The DMC's byte, read for it.
    pub fn dmc_fill(&mut self, byte: u8) {
        self.dmc.fill(byte);
    }

    /// The channels mixed, as the 2A03's two DACs do: 0 to 1.
    pub fn output(&self) -> f32 {
        let pulses = (self.pulses[0].output() + self.pulses[1].output()) as f32;
        let pulse = if pulses == 0.0 {
            0.0
        } else {
            95.88 / (8128.0 / pulses + 100.0)
        };
        let (triangle, noise, dmc) = (
            self.triangle.output() as f32,
            self.noise.output() as f32,
            self.dmc.output as f32,
        );
        let sum = triangle / 8227.0 + noise / 12241.0 + dmc / 22638.0;
        let tnd = if sum == 0.0 {
            0.0
        } else {
            159.79 / (1.0 / sum + 100.0)
        };
        pulse + tnd
    }
}
