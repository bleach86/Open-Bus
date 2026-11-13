//! The Atari 7800: its CPU, a 6502 ("Sally") at 1.79 MHz; MARIA, its
//! graphics (maria.rs); the TIA, for its sound and its controllers' buttons;
//! a POKEY, if the cartridge has one, its sound with the TIA's; the RIOT, its timer, joysticks and switches; 4K of RAM; and the
//! cartridge. As a7800 has it, the BIOS left out, as it only checks the
//! cartridge's signature: the cartridge starts it, INPTCTRL unlocked.
//!
//! Its memory map: the TIA at $00 to $1F, and MARIA at $20 to $3F, again at
//! $100, $200 and $300; RAM at $1800 to $27FF, its $2040 to $20FF again at
//! $40 (zero page) and $2140 to $21FF at $140 (the stack's); the RIOT's
//! registers at $280, and its RAM at $480; the cartridge from $4000. The
//! TIA's addresses are INPTCTRL's too, until it's locked: whether MARIA's
//! on, and the TIA's sound written only once it is.
//!
//! Its time is MARIA's clock, 7.16 MHz, 454 of them a line: a CPU cycle 4
//! of them, but 6, the CPU slowed to 1.19 MHz, for an access to the TIA or
//! the RIOT. MARIA's DMA starts 7 cycles into a line and stops the CPU for
//! as long as it takes, whatever cycle it's on; WSYNC stops it to the end of
//! the line. Its picture's dots start 33.5 cycles in, two of MARIA's clocks
//! each, each shown as MARIA's registers are as it is: a change in the
//! middle of a line shows from there.
//!
//! The controls: in each port a joystick of two buttons, the window's first
//! gamepad and its keys the first, its second the second; each button the
//! TIA's INPT0 to INPT3, or INPT4 and INPT5 too as the game drives SWCHB
//! for one button. A gamepad's Start is the console's Reset, its Select the
//! Select button; and the Reset button the Reset switch too. The difficulty
//! switches (A, as a7800 sets them) F5 and F7, and the Pause button F8, and
//! in the menu.
//!
//! The High Score Cartridge (high_score.rs) between it and the game, if its
//! ROM's found, as if it's always plugged in; unless the game's a bankset's
//! of 52K, its own ROM where the High Score Cartridge's would be, or one of
//! the two that write over it.

pub mod cartridge;
pub mod database;
pub mod high_score;
pub mod maria;
pub mod palette;

#[cfg(test)]
mod tests {
    mod cartridge;
    mod machine;
    mod maria;
}

use crate::chips::pokey::Pokey;
use crate::chips::riot6532::Riot;
use crate::chips::tia_sound;
use crate::cpu::bus::Bus;
use crate::cpu::{CPU, Variant};
use crate::host::display::{Gamepad, SAMPLE_RATE};
use crate::host::files;
use crate::host::machine::{Input, Machine, Picture};
#[cfg(not(target_arch = "wasm32"))]
use crate::host::run::Host;
use crate::host::save_state;
use crate::host::sound::{HighPass, LowPass, Sampler};
#[cfg(not(target_arch = "wasm32"))]
use crate::host::window::Window;
use crate::host::zip::crc32;
use crate::machines::region::Region;
use cartridge::{Cartridge, Kind};
use high_score::HighScore;
use maria::{CELLS, DMA_LIMIT, Maria};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The CPU's clock: MARIA's, NTSC's colour crystal's twice, a fourth; PAL's.
pub const NTSC_CLOCK_HZ: u32 = 1_789_772;
pub const PAL_CLOCK_HZ: u32 = 1_773_447;

/// MARIA's clocks: a line's; a CPU cycle's, and one of the TIA's or the
/// RIOT's, slowed.
pub const LINE: u32 = 454;
const CYCLE: u32 = 4;
const SLOW_CYCLE: u32 = 6;
/// Where in a line its DMA starts, 7 cycles in; and its first dot, 33.5.
pub const DMA_START: u32 = 28;
const FIRST_DOT: u32 = 134;

/// Its picture's dots across: two a cell.
pub const WIDTH: usize = 2 * CELLS;
/// The first line shown, as ProSystem shows them.
const TOP: u32 = 26;

/// How loud it is, of the host's full volume.
const VOLUME: f32 = 0.7;
/// The output's filters' corners: a low-pass, a cycle at a time, for what
/// the sampling would fold back; the high-pass, a sample at a time.
const LOW_PASS: f32 = 15_000.0;
const HIGH_PASS: f32 = 90.0;

/// The function keys of its switches: the difficulties, and Pause; and the
/// frames Pause is held, pressed.
const LEFT_DIFFICULTY_KEY: u8 = 5;
const RIGHT_DIFFICULTY_KEY: u8 = 7;
const PAUSE_KEY: u8 = 8;
const PAUSE_FRAMES: u8 = 6;

/// The CPU's clock on a console of `region`.
pub fn clock_hz(region: Region) -> u32 {
    match region {
        Region::Ntsc => NTSC_CLOCK_HZ,
        Region::Pal => PAL_CLOCK_HZ,
    }
}

/// A frame's lines, `region`'s.
pub fn lines(region: Region) -> u32 {
    match region {
        Region::Ntsc => 263,
        Region::Pal => 313,
    }
}

/// The lines shown, `region`'s, as ProSystem shows them.
pub fn height(region: Region) -> usize {
    match region {
        Region::Ntsc => 223,
        Region::Pal => 272,
    }
}

/// A dot's width, of its height, as a TV shows it: half a color clock's.
pub fn dot_shape(region: Region) -> f32 {
    match region {
        Region::Ntsc => 6.0 / 7.0,
        Region::Pal => 1.04,
    }
}

/// Its sound, when it's made.
struct Sound {
    low_pass: LowPass,
    sampler: Sampler<1>,
    high_pass: HighPass,
}

/// In a save state, all but its ROM and what's the host's (the picture
/// and sound, the controls), which the machine it's loaded into has.
#[derive(Serialize, Deserialize)]
pub struct Atari7800 {
    pub maria: Maria,
    pub riot: Riot,
    pub tia: tia_sound::Sound,
    pub cartridge: Cartridge,
    /// The cartridge's POKEY, if it has one, clocked with the CPU.
    pub pokey: Option<Pokey>,
    /// $1800 to $1FFF, then $2000 to $27FF.
    #[serde(with = "save_state::bytes")]
    ram: [u8; 0x1000],
    /// INPTCTRL, and whether it's locked.
    control: u8,
    locked: bool,
    /// Where MARIA is in the line, in its clocks; the line; whether its DMA
    /// has started; and the line's dots shown.
    pub clock: u32,
    pub line: u32,
    dma_started: bool,
    dots_shown: usize,
    /// WSYNC written, the CPU to stop to the line's end; stopped so.
    wsync: bool,
    waiting: bool,
    /// The NMI line, pulsed by MARIA's DLI: the CPU's samples of it to
    /// come that see it, two, for one to, if it's pulsed in the middle of a
    /// cycle, a slow one's last two clocks, after that cycle's.
    nmi: u8,
    /// MARIA's clocks taken besides the CPU's cycles of 4: owed to the CPU's
    /// clock, and the RIOT's, a cycle of each for each 4.
    stolen: u32,
    riot_owed: u32,
    /// MARIA's clocks to the TIA's sound's next two.
    sound_clocks: u32,
    /// Each port's buttons, as the game drives SWCHB: one button, INPT4 or
    /// INPT5's, not two. One at power-on, its pin 6 pulled high.
    one_button: [bool; 2],
    pub frames: u64,
    frame_ended: bool,
    #[serde(skip)]
    pub region: Region,
    #[serde(skip)]
    lines: u32,
    /// The frame being drawn, and the last, as colours.
    #[serde(skip)]
    drawn: Vec<u8>,
    #[serde(skip)]
    shown: Vec<u8>,
    #[serde(skip)]
    picture: Vec<u32>,
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    pub gamepads: [Gamepad; 2],
    #[serde(skip)]
    reset_held: bool,
    #[serde(skip)]
    pause_frames: u8,
    /// The difficulty switches, each A, not B.
    #[serde(skip)]
    pub difficulty_a: [bool; 2],
    #[serde(skip)]
    sound: Option<Sound>,
    #[serde(skip)]
    samples: Vec<f32>,
    /// The High Score Cartridge, if it's plugged in: not a state's, its
    /// RAM every game's.
    #[serde(skip)]
    pub high_score: Option<HighScore>,
}

impl Atari7800 {
    /// A console of `region` with `cartridge` in it, its RAM all 0s.
    pub fn new(cartridge: Cartridge, region: Region) -> Atari7800 {
        let picture = WIDTH * height(region);
        let pokey = cartridge.pokey.map(|_| Pokey::new());
        let mut atari = Atari7800 {
            pokey,
            maria: Maria::new(),
            riot: Riot::new(vec![0; 128], 0),
            tia: tia_sound::Sound::default(),
            cartridge,
            ram: [0; 0x1000],
            control: 0,
            locked: false,
            clock: 0,
            line: 0,
            dma_started: false,
            dots_shown: 0,
            wsync: false,
            waiting: false,
            nmi: 0,
            stolen: 0,
            riot_owed: 0,
            sound_clocks: 0,
            one_button: [true; 2],
            frames: 0,
            frame_ended: false,
            region,
            lines: lines(region),
            drawn: vec![0; picture],
            shown: vec![0; picture],
            picture: Vec::new(),
            name: String::new(),
            gamepads: [Gamepad::default(); 2],
            reset_held: false,
            pause_frames: 0,
            difficulty_a: [true; 2],
            sound: None,
            samples: Vec::new(),
            high_score: None,
        };
        atari.controls_to_pins();
        atari
    }

    /// The controls and switches as the RIOT's pins have them, 0 for each
    /// pressed or set.
    fn controls_to_pins(&mut self) {
        let stick = |pad: Gamepad| {
            let (up, down) = (pad.up && !pad.down, pad.down && !pad.up);
            let (left, right) = (pad.left && !pad.right, pad.right && !pad.left);
            !(up as u8 | (down as u8) << 1 | (left as u8) << 2 | (right as u8) << 3) & 0x0F
        };
        let [first, second] = self.gamepads;
        self.riot.pins_a = stick(first) << 4 | stick(second);
        let reset = self.reset_held || first.start;
        let pause = self.pause_frames > 0;
        self.riot.pins_b = 0x34
            | !reset as u8
            | (!first.select as u8) << 1
            | (!pause as u8) << 3
            | (self.difficulty_a[0] as u8) << 6
            | (self.difficulty_a[1] as u8) << 7;
    }

    /// A read of the TIA's input `register`, $0 to $F: its buttons, the
    /// joystick's 1 and 2 on INPT1 and INPT0 (the right port's INPT3 and
    /// INPT2), high when pressed, in two button mode; on INPT4 and INPT5,
    /// either, low, in one button mode, as ProSystem's tests of a console
    /// found them; the collisions none.
    fn tia_read(&self, register: u8) -> u8 {
        let pressed = |pressed: bool| if pressed { 0x80 } else { 0 };
        let pad = |port: usize| self.gamepads[port];
        let two = |port: usize| !self.one_button[port];
        match register {
            0x0..=0x7 => 0,
            0x8 => pressed(two(0) && pad(0).b),
            0x9 => pressed(two(0) && pad(0).a),
            0xA => pressed(two(1) && pad(1).b),
            0xB => pressed(two(1) && pad(1).a),
            0xC | 0xD => {
                let port = register as usize - 0xC;
                let fire = self.one_button[port] && (pad(port).a || pad(port).b);
                if fire { 0 } else { 0x80 }
            }
            _ => 0xFF,
        }
    }

    /// A write of the TIA's `register`, $00 to $1F: INPTCTRL's, until it's
    /// locked; then its sound's, once MARIA's on.
    fn tia_write(&mut self, register: u8, value: u8) {
        if !self.locked {
            self.locked = value & 0x01 != 0;
            self.control = value;
        }
        if register == 0x01 || self.control & 3 != 3 {
            return;
        }
        let channels = &mut self.tia.channels;
        match register {
            0x15 | 0x16 => channels[register as usize - 0x15].audc(value),
            0x17 | 0x18 => channels[register as usize - 0x17].audf(value),
            0x19 | 0x1A => channels[register as usize - 0x19].audv(value),
            _ => {}
        }
    }

    /// Whether `addr` is the cartridge's POKEY's: $4000 to $7FFF, $0450 to
    /// $045F, or $0800 to $0FFF, as it's put.
    fn pokey_at(&self, addr: u16) -> bool {
        match self.cartridge.pokey {
            Some(0x4000) => (0x4000..0x8000).contains(&addr),
            Some(0x0450) => (0x0450..0x0460).contains(&addr),
            Some(0x0800) => (0x0800..0x1000).contains(&addr),
            _ => false,
        }
    }

    /// The RIOT's port B written: each port's one button mode, as its pin 6
    /// is: two buttons only driven low, one if it's an input, pulled high.
    fn port_b_written(&mut self) {
        let (out, ddr) = self.riot.outputs_b();
        for (port, bit) in [0x04, 0x10].into_iter().enumerate() {
            self.one_button[port] = ddr & bit == 0 || out & bit != 0;
        }
    }

    /// MARIA's clocks gone by, `clocks` of them: the TIA's sound; MARIA's
    /// DMA, where it starts, and the CPU stopped for it; the line's end.
    fn advance(&mut self, clocks: u32) {
        let mut left = clocks;
        while left > 0 {
            let next = if self.dma_started { LINE } else { DMA_START };
            let step = left.min(next - self.clock);
            self.clock_sound(step);
            self.clock += step;
            left -= step;
            if self.clock == DMA_START && !self.dma_started {
                self.dma_started = true;
                let cycles = self.dma();
                // The CPU stopped, unless it already is to the line's end;
                // at MARIA's limit, to the line's end.
                if cycles > 0 && !self.waiting {
                    let stop = if cycles >= DMA_LIMIT {
                        LINE - self.clock
                    } else {
                        cycles.div_ceil(CYCLE) * CYCLE
                    };
                    left += stop;
                    self.steal(stop);
                }
            }
            if self.clock == LINE {
                self.next_line();
            }
        }
    }

    /// `clocks` of MARIA's taken from the CPU, on its clock and the RIOT's.
    fn steal(&mut self, clocks: u32) {
        self.stolen += clocks;
        self.riot_owed += clocks;
        while self.riot_owed >= CYCLE {
            self.riot_owed -= CYCLE;
            self.riot.cycle();
        }
    }

    /// The TIA's sound, `clocks` of MARIA's on: the TIA's clock half
    /// MARIA's, and the output sampled each CPU cycle's worth.
    fn clock_sound(&mut self, clocks: u32) {
        self.sound_clocks += clocks;
        while self.sound_clocks >= CYCLE {
            self.sound_clocks -= CYCLE;
            self.tia.tick();
            self.tia.tick();
            if let Some(pokey) = &mut self.pokey {
                pokey.clock();
            }
            let Some(sound) = &mut self.sound else {
                continue;
            };
            let pokey = self.pokey.as_ref().map_or(0.0, Pokey::output);
            let output = sound.low_pass.filter((self.tia.output() + pokey).min(1.0));
            if let Some([sample]) = sound.sampler.cycle([output]) {
                let sample = sound.high_pass.filter(sample);
                self.samples.extend([sample * VOLUME; 2]);
            }
        }
    }

    /// The line's DMA: its cycles, the CPU's NMI pulsed if it interrupts.
    fn dma(&mut self) -> u32 {
        let (ram, riot, cartridge) = (&self.ram, &self.riot, &self.cartridge);
        let high_score = self.high_score.as_ref();
        let mut read = |addr: u16| peek(ram, riot, cartridge, high_score, addr, true);
        let (cycles, nmi) = self.maria.dma(self.line, self.lines, &mut read);
        if nmi {
            self.nmi = 2;
        }
        cycles
    }

    /// The line's dots shown up to now, as MARIA's registers are.
    fn show_dots(&mut self) {
        let due = (self.clock.saturating_sub(FIRST_DOT) as usize)
            .div_ceil(2)
            .min(CELLS);
        self.show_dots_to(due);
    }

    fn show_dots_to(&mut self, due: usize) {
        if self.line >= TOP && ((self.line - TOP) as usize) < height(self.region) {
            let at = (self.line - TOP) as usize * WIDTH;
            for x in self.dots_shown..due {
                let [left, right] = self.maria.dots(x);
                self.drawn[at + 2 * x] = left;
                self.drawn[at + 2 * x + 1] = right;
            }
        }
        self.dots_shown = self.dots_shown.max(due);
    }

    /// The line ended: the rest of it shown, and the next begun, or the
    /// frame ended.
    fn next_line(&mut self) {
        self.show_dots_to(CELLS);
        self.clock = 0;
        self.dma_started = false;
        self.dots_shown = 0;
        self.line += 1;
        if self.line == self.lines {
            self.line = 0;
            self.frames += 1;
            self.frame_ended = true;
            std::mem::swap(&mut self.drawn, &mut self.shown);
        }
        self.maria.line_started(self.line, self.lines);
    }
}

/// What's read at `addr` of RAM or the cartridge (or the High Score
/// Cartridge, `high_score`, if it's in), by the CPU, or by `maria`'s DMA (a
/// bankset's own image), without what a read does; nothing of the TIA's or
/// MARIA's.
fn peek(
    ram: &[u8; 0x1000],
    riot: &Riot,
    cartridge: &Cartridge,
    high_score: Option<&HighScore>,
    addr: u16,
    maria: bool,
) -> u8 {
    let cartridge_read = |addr| {
        if maria {
            cartridge.maria_read(addr)
        } else {
            cartridge.read(addr)
        }
    };
    match addr {
        0x0040..=0x00FF | 0x0140..=0x01FF => ram[0x800 + addr as usize],
        0x1800..=0x1FFF => ram[addr as usize - 0x1800],
        0x2000..=0x27FF => ram[0x800 + addr as usize - 0x2000],
        _ if is_riot_ram(addr) => riot.ram[addr as usize & 0x7F],
        _ if HighScore::has(addr) && high_score.is_some() => {
            high_score.map_or(0, |high_score| high_score.read(addr))
        }
        0x3000..=0x3FFF if cartridge.from_3000() => cartridge_read(addr),
        0x4000.. if !cartridge.pokey_at(addr) => cartridge_read(addr),
        _ => 0,
    }
}

/// Whether `addr` is the TIA's, $00 to $1F, or MARIA's, $20 to $3F, at
/// $000, $100, $200 or $300.
fn is_tia(addr: u16) -> bool {
    addr < 0x400 && addr & 0xE0 == 0x00
}

fn is_maria(addr: u16) -> bool {
    addr < 0x400 && addr & 0xE0 == 0x20
}

/// Whether `addr` is the RIOT's registers, $280 to $29F and again with A5,
/// A6 and A8; or its RAM, $480 to $4FF and again at $580.
fn is_riot(addr: u16) -> bool {
    addr & !0x0160 & 0xFFE0 == 0x0280
}

fn is_riot_ram(addr: u16) -> bool {
    addr & !0x0100 & 0xFF80 == 0x0480
}

impl Bus for Atari7800 {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            _ if is_tia(addr) => {
                self.advance(SLOW_CYCLE - CYCLE);
                self.stolen += SLOW_CYCLE - CYCLE;
                self.tia_read(addr as u8 & 0x0F)
            }
            _ if is_maria(addr) => self.maria.read(addr as u8 & 0x1F),
            _ if is_riot(addr) || is_riot_ram(addr) => {
                self.advance(SLOW_CYCLE - CYCLE);
                self.stolen += SLOW_CYCLE - CYCLE;
                self.riot.read(addr)
            }
            _ if self.pokey_at(addr) => self
                .pokey
                .as_ref()
                .map_or(0xFF, |pokey| pokey.read(addr as u8)),
            _ => peek(
                &self.ram,
                &self.riot,
                &self.cartridge,
                self.high_score.as_ref(),
                addr,
                false,
            ),
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            _ if is_tia(addr) => {
                self.advance(SLOW_CYCLE - CYCLE);
                self.stolen += SLOW_CYCLE - CYCLE;
                self.tia_write(addr as u8 & 0x1F, value);
            }
            _ if is_maria(addr) => {
                self.show_dots();
                self.wsync |= self.maria.write(addr as u8 & 0x1F, value);
            }
            _ if is_riot(addr) || is_riot_ram(addr) => {
                self.advance(SLOW_CYCLE - CYCLE);
                self.stolen += SLOW_CYCLE - CYCLE;
                self.riot.write(addr, value);
                if is_riot(addr) && addr & 0x06 == 0x02 {
                    self.port_b_written();
                }
            }
            _ if self.pokey_at(addr) => {
                if let Some(pokey) = &mut self.pokey {
                    pokey.write(addr as u8, value);
                }
            }
            0x0040..=0x00FF | 0x0140..=0x01FF => self.ram[0x800 + addr as usize] = value,
            0x1800..=0x1FFF => self.ram[addr as usize - 0x1800] = value,
            0x2000..=0x27FF => self.ram[0x800 + addr as usize - 0x2000] = value,
            0x4000.. => self.cartridge.write(addr, value),
            _ if HighScore::has(addr) => {
                if let Some(high_score) = &mut self.high_score {
                    high_score.write(addr, value);
                }
            }
            _ => {}
        }
    }

    /// A CPU cycle's 4 of MARIA's clocks, and the RIOT's cycle, before its
    /// access; first, if WSYNC was written, the rest of the line stopped.
    fn tick(&mut self) {
        self.nmi = self.nmi.saturating_sub(1);
        if self.wsync {
            self.wsync = false;
            let rest = LINE - self.clock;
            self.waiting = true;
            self.steal(rest);
            self.advance(rest);
            self.waiting = false;
        }
        self.riot.cycle();
        self.advance(CYCLE);
    }

    fn nmi(&self) -> bool {
        self.nmi > 0
    }

    fn stolen(&mut self) -> u32 {
        let cycles = self.stolen / CYCLE;
        self.stolen %= CYCLE;
        cycles
    }

    fn hand_back(&mut self) -> bool {
        std::mem::take(&mut self.frame_ended)
    }
}

impl Machine for Atari7800 {
    fn picture(&mut self) -> Picture<'_> {
        let colours = match self.region {
            Region::Ntsc => &palette::NTSC,
            Region::Pal => &palette::PAL,
        };
        self.picture.clear();
        self.picture
            .extend(self.shown.iter().map(|&colour| colours[colour as usize]));
        Picture {
            pixels: &self.picture,
            width: WIDTH,
            height: height(self.region),
        }
    }

    fn title(&self) -> String {
        match self.region {
            Region::Ntsc => format!("Atari 7800   {}", self.name),
            Region::Pal => format!("Atari 7800   {}   PAL, 50 Hz", self.name),
        }
    }

    fn debug(&self) -> Vec<String> {
        let region = match self.region {
            Region::Ntsc => "NTSC",
            Region::Pal => "PAL",
        };
        vec![
            format!("ATARI 7800 {region} FRAME {}", self.frames),
            format!("CARTRIDGE {}", self.cartridge.name()),
            format!("INPTCTRL {:02X}", self.control),
            format!(
                "HIGH SCORE CARTRIDGE {}",
                if self.high_score.is_some() {
                    "IN"
                } else {
                    "OUT"
                }
            ),
        ]
    }

    fn sound_on(&mut self) {
        let clock_hz = clock_hz(self.region);
        self.sound = Some(Sound {
            low_pass: LowPass::new(LOW_PASS, clock_hz),
            sampler: Sampler::new(clock_hz),
            high_pass: HighPass::new(HIGH_PASS, SAMPLE_RATE),
        });
    }

    fn sound(&mut self) -> &mut Vec<f32> {
        &mut self.samples
    }

    fn input(&mut self, input: Input) {
        self.gamepads = [input.gamepad, input.gamepad_2];
        self.pause_frames = self.pause_frames.saturating_sub(1);
        self.controls_to_pins();
    }

    fn rewinds(&self) -> bool {
        true
    }

    fn menu(&self) -> Vec<(&'static str, u8)> {
        vec![
            ("LEFT DIFFICULTY", LEFT_DIFFICULTY_KEY),
            ("RIGHT DIFFICULTY", RIGHT_DIFFICULTY_KEY),
            ("PAUSE", PAUSE_KEY),
        ]
    }

    /// F5 and F7, the difficulty switches; F8, the Pause button pressed.
    fn function_key(&mut self, key: u8) -> Option<String> {
        let said = match key {
            LEFT_DIFFICULTY_KEY | RIGHT_DIFFICULTY_KEY => {
                let n = (key == RIGHT_DIFFICULTY_KEY) as usize;
                self.difficulty_a[n] = !self.difficulty_a[n];
                let side = ["Left", "Right"][n];
                let ab = if self.difficulty_a[n] { "A" } else { "B" };
                format!("{side} difficulty: {ab}")
            }
            PAUSE_KEY => {
                self.pause_frames = PAUSE_FRAMES;
                "Pause".to_string()
            }
            _ => return None,
        };
        self.controls_to_pins();
        Some(said)
    }

    /// The Reset button: the console's Reset switch, held.
    fn reset_button(&mut self, held: bool) {
        self.reset_held = held;
        self.controls_to_pins();
    }

    /// The High Score Cartridge's RAM kept, a second after it's written, and
    /// as it's put away.
    fn keep_saved(&mut self, closing: bool) {
        if let Some(high_score) = &mut self.high_score
            && (closing || self.frames % 60 == 0)
        {
            high_score.keep_saved();
        }
    }

    fn identity(&self) -> String {
        let region = if self.region == Region::Pal {
            ", PAL"
        } else {
            ""
        };
        format!("Atari 7800, ROM {:08X}{region}", crc32(&self.cartridge.rom))
    }

    fn keep(&mut self, running: &mut Atari7800) {
        use std::mem::take;
        self.cartridge.take_rom(&mut running.cartridge);
        self.region = running.region;
        self.lines = running.lines;
        self.drawn = take(&mut running.drawn);
        self.shown = take(&mut running.shown);
        self.picture = take(&mut running.picture);
        self.name = take(&mut running.name);
        self.gamepads = running.gamepads;
        self.difficulty_a = running.difficulty_a;
        self.sound = running.sound.take();
        self.samples = take(&mut running.samples);
        self.high_score = running.high_score.take();
        self.controls_to_pins();
    }
}

/// An .a78 file's header, if `image` has one: its first 128 bytes, "ATARI7800"
/// from its second.
fn header(image: &[u8]) -> Option<&[u8]> {
    (image.len() > 128 && &image[1..10] == b"ATARI7800").then(|| &image[..128])
}

/// Whether `image`, a ROM with no name to say, is a 7800's and not a 2600's:
/// it has an .a78's header, or ProSystem's database knows it, or it's 16K
/// or more and ends as the 7800's BIOS wants a cartridge to, $FFF8's high
/// nibble $F and bit 0 set, $FFF9's low nibble 3 or 7. (A 2600's few that
/// end so by chance are 4K or 8K.)
pub fn is_7800(image: &[u8]) -> bool {
    if header(image).is_some() || database::find(&crate::host::md5::md5(image)).is_some() {
        return true;
    }
    // $FFF8 and $FFF9, before the vectors.
    let [.., region, start, _, _, _, _, _, _] = image[..] else {
        return false;
    };
    image.len() >= 0x4000 && region & 0xF1 == 0xF1 && start & 0x0B == 0x03
}

/// The ROM at `path`, or in the zip there, the first .a78 or .bin in it.
pub fn image(path: &Path) -> Result<Vec<u8>, String> {
    image_of(path, files::read(path).map_err(|e| e.to_string())?)
}

/// The ROM in `bytes`, the file at `path`'s, or in the zip they are.
pub fn image_of(path: &Path, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let zipped = path
        .extension()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("zip"));
    if !zipped {
        return Ok(bytes);
    }
    let is_game = |name: &str| {
        let name = name.to_lowercase();
        name.ends_with(".a78") || name.ends_with(".bin")
    };
    let (_, image) = crate::host::zip::file(&bytes, is_game)?;
    Ok(image)
}

/// What's known of the game at `path` without running it, its `image` if it
/// could be had, as `machines::facts` has it: its ROM's CRC-32, without its
/// header, its cartridge, and why it won't load, if it won't.
pub fn facts_of(path: &Path, image: Result<Vec<u8>, String>) -> String {
    let fact = |image: &[u8]| -> Result<[String; 2], String> {
        let (cartridge, _) = cartridge(image)?;
        Ok([
            format!("{:08X}", crc32(&cartridge.rom)),
            format!("ATARI 7800 {}", cartridge.name()),
        ])
    };
    let [crc, board, problem] = match image.and_then(|image| fact(&image)) {
        Ok([crc, board]) => [crc, board, String::new()],
        Err(problem) => ["".into(), "".into(), problem],
    };
    let problem = problem.replace(['\t', '\n'], " ");
    format!("{crc}\t{board}\t0\t0\t{problem}\t{}", path.display())
}

/// The cartridge in `image`, as ProSystem's database says (as ProSystem
/// has it, before a header, which is sometimes wrong), or its header, or
/// its size; and whether it says it's PAL.
fn cartridge(image: &[u8]) -> Result<(Cartridge, Option<bool>), String> {
    let headed = header(image);
    // Its size as its header says, not what's after it.
    let rom = match headed {
        Some(header) => {
            let size =
                u32::from_be_bytes([header[49], header[50], header[51], header[52]]) as usize;
            let rom = &image[128..];
            &rom[..if size > 0 {
                size.min(rom.len())
            } else {
                rom.len()
            }]
        }
        None => image,
    };
    let known = database::find(&crate::host::md5::md5(rom));
    let (kind, pokey, pal) = match (known, headed) {
        (Some(known), _) => (known.kind, known.pokey, Some(known.pal)),
        (None, Some(header)) => {
            let (kind, pokey) = Kind::headed(u16::from_be_bytes([header[53], header[54]]));
            (kind, pokey, Some(header[57] & 1 != 0))
        }
        (None, None) => (Kind::sized(rom.len()), None, None),
    };
    Ok((Cartridge::new(rom, kind, pokey)?, pal))
}

/// The 7800, on whichever host, with the game at `rom` (an .a78 or a
/// headerless ROM, or a zip of one), its region `region`, or as its header,
/// ProSystem's database or its name says.
pub fn build(rom: &str, region: Option<Region>) -> Result<CPU<Atari7800>, String> {
    let path = Path::new(rom);
    let image = image(path).map_err(|e| format!("{rom}: {e}"))?;
    let (cartridge, pal) = cartridge(&image).map_err(|e| format!("{rom}: {e}"))?;
    let named = path
        .file_name()
        .and_then(|name| Region::from_name(&name.to_string_lossy()));
    let said = pal.map(|pal| if pal { Region::Pal } else { Region::Ntsc });
    let region = region.or(said).or(named).unwrap_or_default();
    let mut atari = Atari7800::new(cartridge, region);
    let md5 = crate::host::md5::md5(&atari.cartridge.rom);
    if !atari.cartridge.from_3000() && !high_score::left_out_for(&md5) {
        atari.high_score = HighScore::found(path);
    }
    atari.name = path
        .file_stem()
        .map_or(String::new(), |name| name.to_string_lossy().into_owned());
    let mut cpu = CPU::with_bus(atari);
    cpu.variant = Variant::Nmos6502;
    cpu.clock_hz = clock_hz(region);
    cpu.reset();
    Ok(cpu)
}

/// Frames a second, `atari`'s: a line's 454 of MARIA's clocks, 4 a cycle,
/// as many lines as its region's frame has.
pub fn frames_per_second(atari: &Atari7800) -> f64 {
    clock_hz(atari.region) as f64 * CYCLE as f64 / LINE as f64 / atari.lines as f64
}

/// How big its picture's shown, `region`'s: each dot as that TV shows it,
/// 3 times over.
pub fn size(region: Region) -> (u32, u32) {
    (
        (WIDTH as f32 * dot_shape(region) * 3.0).round() as u32,
        height(region) as u32 * 3,
    )
}

/// The 7800 as `build` has it, run in a window, `fullscreen` or not.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(rom: &str, fullscreen: bool, region: Option<Region>) {
    let cpu = build(rom, region).unwrap_or_else(|e| panic!("{e}"));
    let region = cpu.memory.region;
    let title = cpu.memory.title();
    let window = Window::new(&title, WIDTH, height(region), size(region), fullscreen);
    Host::new(cpu, Box::new(window), save_state::path(Path::new(rom))).run();
}
