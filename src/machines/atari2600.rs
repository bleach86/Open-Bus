//! The Atari 2600: its CPU, a 6507 (a 6502 with 13 address lines, no
//! interrupts), at 1.19 MHz (the NTSC colour crystal's third; PAL's,
//! 1.18 MHz); the TIA, its picture, sound and fire buttons; the RIOT, its
//! RAM, timer, joysticks and switches; and the cartridge.
//!
//! The CPU's memory map, its 13 bits: the cartridge where A12's set ($1000
//! to $1FFF); else the TIA where A7's clear, its registers again every 64
//! bytes; else the RIOT, its RAM where A9's clear ($80 to $FF, and again at
//! $180, the stack's), its registers where it's set ($280 up). What a read
//! of the TIA doesn't drive is the data bus's, as the last access left it.
//!
//! The TIA's three color clocks each CPU cycle are this cycle's, before its
//! access. WSYNC, written, holds RDY: the CPU, at its next read, waits out
//! the line, as many cycles as it has left, its read on the next's first.
//!
//! The controls: in each port a joystick, paddles, a driving controller or
//! a keypad (controllers.rs), as Stella's properties for the game say, or
//! as its code says; the first player the window's first gamepad and its
//! keys, and the pointer, the second its second gamepad; a keypad, the
//! keyboard, a key typed held KEYPAD_FRAMES, and on as it repeats. A gamepad's Start is the
//! console's Reset switch, its Select the Select switch; and the Reset
//! button the Reset switch too. The difficulty switches (as the properties
//! set them, else B) and the colour switch, F5, F7 and F8, and in the menu.
//!
//! Its picture's layout, a TV's: as its frames' lines come to in its first
//! second, VSYNC to VSYNC, run before it starts, as Stella does, 312 a PAL
//! TV's. Its region, NTSC or PAL, the console's, its clock and its colours:
//! as the properties' format says, else PAL if the game's name or its
//! layout says so ("PAL60" games, European ones of 262 lines, a PAL
//! console's on an NTSC TV's).
//!
//! A game the properties say flickers, drawing things on alternate frames,
//! is shown as a TV's phosphor shows it: each dot the brighter of what's
//! drawn and what was, faded by half.
//!
//! Its sound, the TIA's, sampled at the host's rate and high-passed, as the
//! NES's is.

pub mod cartridge;
pub mod controllers;
pub mod database;
pub mod frame;
pub mod objects;
pub mod palette;
pub mod riot;
pub mod sound;
pub mod supercharger;
pub mod tia;

#[cfg(test)]
mod tests {
    mod cartridge;
    mod controllers;
    mod machine;
    mod riot;
    mod tia;
}

use crate::cpu::bus::Bus;
use crate::cpu::{CPU, Variant};
use crate::host::display::{Gamepad, Key, SAMPLE_RATE};
use crate::host::files;
use crate::host::machine::{Input, Machine, Picture};
#[cfg(not(target_arch = "wasm32"))]
use crate::host::run::Host;
#[cfg(not(target_arch = "wasm32"))]
use crate::host::save_state;
use crate::host::sound::{HighPass, LowPass, Sampler};
#[cfg(not(target_arch = "wasm32"))]
use crate::host::window::Window;
use crate::host::zip::crc32;
use crate::machines::region::Region;
use cartridge::{Cartridge, Kind};
use controllers::{Column, Controller, KEYPAD_PULL_UP, Paddle, Wheel};
use riot::Riot;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tia::Tia;

/// The CPU's clock: NTSC's colour crystal's 3,579,545 Hz, a third; PAL's
/// 3,546,894 Hz, a third.
pub const NTSC_CLOCK_HZ: u32 = 1_193_182;
pub const PAL_CLOCK_HZ: u32 = 1_182_298;

/// How loud it is, of the host's full volume.
const VOLUME: f32 = 0.7;
/// The frames a keypad's key typed is held, for a game reading its keypad
/// now and then to see it: a tenth of a second; held on, it's typed again
/// as it repeats. Not as long as any key's held, which would keep it held
/// while the stick's keys are, as a joystick's moved beside it.
const KEYPAD_FRAMES: u8 = 6;
/// The output's filters' corners: a low-pass, a cycle at a time, for what
/// the sampling would fold back; the high-pass, a sample at a time.
const LOW_PASS: f32 = 15_000.0;
const HIGH_PASS: f32 = 90.0;

/// The function keys of its switches: the difficulties, and colour.
const LEFT_DIFFICULTY_KEY: u8 = 5;
const RIGHT_DIFFICULTY_KEY: u8 = 7;
const COLOUR_KEY: u8 = 8;

/// The CPU's clock on a console of `region`.
pub fn clock_hz(region: Region) -> u32 {
    match region {
        Region::Ntsc => NTSC_CLOCK_HZ,
        Region::Pal => PAL_CLOCK_HZ,
    }
}

/// A dot's width, of its height, as a TV shows it: a color clock's.
pub fn dot_shape(region: Region) -> f32 {
    match region {
        Region::Ntsc => 12.0 / 7.0,
        Region::Pal => 2.08,
    }
}

/// Its sound, when it's made.
struct Sound {
    low_pass: LowPass,
    sampler: Sampler<1>,
    high_pass: HighPass,
}

/// The console's switches, as they're set: each difficulty A (not B), and
/// colour (not black and white).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Switches {
    pub difficulty_a: [bool; 2],
    pub colour: bool,
}

impl Default for Switches {
    fn default() -> Switches {
        Switches {
            difficulty_a: [false; 2],
            colour: true,
        }
    }
}

/// In a save state, all but its ROM and what's the host's (the picture
/// and sound, the controls), which the machine it's loaded into has.
#[derive(Serialize, Deserialize)]
pub struct Atari2600 {
    pub tia: Tia,
    pub riot: Riot,
    pub cartridge: Cartridge,
    /// The last byte on the data bus.
    data_bus: u8,
    /// The cycles left that WSYNC holds the CPU for.
    wsync_cycles: u32,
    pub cycles: u64,
    pub frames: u64,
    #[serde(skip)]
    frame_ended: bool,
    /// Its region, the console's clock and colours, and its picture's
    /// layout, from its game: a state's loaded into one of the same.
    #[serde(skip)]
    pub region: Region,
    #[serde(skip)]
    pub layout: Region,
    /// What's in its ports; its ports swapped, the first player's the
    /// right; its paddles, which player turns each and their buttons; its
    /// wheels.
    #[serde(skip)]
    pub controllers: [Controller; 2],
    #[serde(skip)]
    pub swap_ports: bool,
    #[serde(skip)]
    paddles: [Paddle; 4],
    #[serde(skip)]
    pub paddle_players: [Option<usize>; 4],
    #[serde(skip)]
    paddle_fire: [bool; 4],
    #[serde(skip)]
    wheels: [Wheel; 2],
    /// Each keypad's keys held, as `controllers::columns` has them; and the
    /// frames more they're held.
    #[serde(skip)]
    keypad_keys: [u16; 2],
    #[serde(skip)]
    keypad_frames: u8,
    /// Its phosphor's blend, if it's shown with one; and the frame it was
    /// last blended for.
    #[serde(skip)]
    pub phosphor: Option<f32>,
    #[serde(skip)]
    blended: Option<u64>,
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    pub gamepads: [Gamepad; 2],
    #[serde(skip)]
    reset_held: bool,
    #[serde(skip)]
    pub switches: Switches,
    #[serde(skip)]
    picture: Vec<u32>,
    #[serde(skip)]
    sound: Option<Sound>,
    #[serde(skip)]
    samples: Vec<f32>,
}

impl Atari2600 {
    /// A console of `region` with `cartridge` in it, its picture's layout
    /// `layout`'s; its RAM all 0s and its timer at 0, as Stella has them.
    pub fn new(cartridge: Cartridge, region: Region, layout: Region) -> Atari2600 {
        let mut atari = Atari2600 {
            tia: Tia::new(layout),
            riot: Riot::new(vec![0; 128], 0),
            cartridge,
            data_bus: 0,
            wsync_cycles: 0,
            cycles: 0,
            frames: 0,
            frame_ended: false,
            region,
            layout,
            controllers: [Controller::Joystick; 2],
            swap_ports: false,
            paddles: [Paddle::new(); 4],
            paddle_players: [None; 4],
            paddle_fire: [false; 4],
            wheels: [Wheel::default(); 2],
            keypad_keys: [0; 2],
            keypad_frames: 0,
            phosphor: None,
            blended: None,
            name: String::new(),
            gamepads: [Gamepad::default(); 2],
            reset_held: false,
            switches: Switches::default(),
            picture: Vec::new(),
            sound: None,
            samples: Vec::new(),
        };
        atari.controls_to_pins();
        atari
    }

    /// The players' gamepads as the ports have them, swapped or not.
    fn port_pads(&self) -> [Gamepad; 2] {
        let [first, second] = self.gamepads;
        if self.swap_ports {
            [second, first]
        } else {
            [first, second]
        }
    }

    /// The controllers and switches as the RIOT's pins and the TIA's inputs
    /// have them, 0 for each pressed or set.
    fn controls_to_pins(&mut self) {
        let stick = |pad: Gamepad| {
            let (up, down) = (pad.up && !pad.down, pad.down && !pad.up);
            let (left, right) = (pad.left && !pad.right, pad.right && !pad.left);
            !(up as u8 | (down as u8) << 1 | (left as u8) << 2 | (right as u8) << 3) & 0x0F
        };
        let pads = self.port_pads();
        let mut pins = 0;
        for (port, pad) in pads.into_iter().enumerate() {
            let pin_bits = match self.controllers[port] {
                Controller::Joystick => stick(pad),
                // A's button on its right pin, B's on its left; 0 pressed.
                Controller::Paddles { .. } => {
                    let [a, b] = [self.paddle_fire[2 * port], self.paddle_fire[2 * port + 1]];
                    !((a as u8) << 3 | (b as u8) << 2) & 0x0F
                }
                Controller::Driving => 0x0C | self.wheels[port].pins(),
                // Its rows, driven by the game.
                Controller::Keypad => 0x0F,
            };
            pins |= pin_bits << (4 * (1 - port));
            let joystick = matches!(
                self.controllers[port],
                Controller::Joystick | Controller::Driving
            );
            self.tia.fire[port] = joystick && (pad.a || pad.b);
        }
        self.riot.pins_a = pins;
        self.keypads_to_inputs();
        let first = self.gamepads[0];
        let reset = self.reset_held || first.start;
        let switches = self.switches;
        self.riot.pins_b = 0x34
            | !reset as u8
            | (!first.select as u8) << 1
            | (switches.colour as u8) << 3
            | (switches.difficulty_a[0] as u8) << 6
            | (switches.difficulty_a[1] as u8) << 7;
    }

    /// How many keypads are plugged in, for their keys to be told of.
    pub fn keypads(&self) -> usize {
        self.controllers
            .iter()
            .filter(|&&controller| controller == Controller::Keypad)
            .count()
    }

    /// Each keypad's columns, as its keys held and the rows the game drives
    /// have them, on the TIA's inputs: the first two the paddles', the
    /// third the fire button's.
    fn keypads_to_inputs(&mut self) {
        let driven = self.riot.driven_a();
        for port in 0..2 {
            if self.controllers[port] != Controller::Keypad {
                continue;
            }
            let rows = (driven >> (4 * (1 - port))) & 0x0F;
            let columns = controllers::columns(self.keypad_keys[port], rows);
            for (n, column) in columns[..2].iter().enumerate() {
                let resistance = if *column == Column::Open {
                    KEYPAD_PULL_UP
                } else {
                    0
                };
                self.tia
                    .keypad_column(2 * port + n, *column == Column::Ground, resistance);
            }
            self.tia.fire[port] = columns[2] == Column::Ground;
        }
    }

    fn sound_cycle(&mut self) {
        let Some(sound) = &mut self.sound else { return };
        let output = sound.low_pass.filter(self.tia.sound.output());
        if let Some([sample]) = sound.sampler.cycle([output]) {
            let sample = sound.high_pass.filter(sample);
            self.samples.extend([sample * VOLUME; 2]);
        }
    }
}

impl Bus for Atari2600 {
    fn read(&mut self, addr: u16) -> u8 {
        let addr = addr & 0x1FFF;
        self.cartridge.seen(addr);
        let value = if addr & 0x1000 != 0 {
            self.cartridge.read_on(addr, &mut self.riot.ram)
        } else if addr & 0x80 == 0 {
            self.tia.read(addr, self.data_bus)
        } else {
            self.riot.read(addr)
        };
        self.cartridge.access(addr, value, false);
        self.data_bus = value;
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        let addr = addr & 0x1FFF;
        self.cartridge.seen(addr);
        self.data_bus = value;
        if addr & 0x1000 != 0 {
            self.cartridge.write(addr, value);
        } else if addr & 0x80 == 0 {
            if self.tia.write(addr, value) {
                // The rest of the line, in whole cycles.
                let left = (tia::LINE - self.tia.hctr) % tia::LINE;
                self.wsync_cycles = left as u32 / 3;
            }
        } else {
            self.riot.write(addr, value);
            // SWCHA or SWACNT: a keypad's rows driven.
            if addr & 0x0206 == 0x0200 {
                self.keypads_to_inputs();
            }
        }
        self.cartridge.access(addr, value, true);
    }

    /// A cycle's three color clocks, and the RIOT's cycle, before its
    /// access.
    fn tick(&mut self) {
        self.cycles += 1;
        for _ in 0..3 {
            self.tia.clock();
        }
        self.riot.cycle();
        self.cartridge.cycle();
        self.sound_cycle();
        if self.tia.frame_ready {
            self.tia.frame_ready = false;
            self.frames += 1;
            self.frame_ended = true;
        }
    }

    /// WSYNC's hold on RDY, a cycle at a time.
    fn halt(&mut self, _addr: u16) -> bool {
        if self.wsync_cycles == 0 {
            return false;
        }
        self.wsync_cycles -= 1;
        true
    }

    fn hand_back(&mut self) -> bool {
        std::mem::take(&mut self.frame_ended)
    }
}

impl Machine for Atari2600 {
    fn picture(&mut self) -> Picture<'_> {
        let colours = palette::palette(self.region);
        let drawn = self
            .tia
            .picture
            .iter()
            .map(|&colour| colours[colour as usize >> 1]);
        match self.phosphor {
            None => {
                self.picture.clear();
                self.picture.extend(drawn);
            }
            // Blended once a frame, however often it's looked at.
            Some(blend) if self.blended != Some(self.frames) => {
                self.blended = Some(self.frames);
                self.picture.resize(self.tia.picture.len(), 0);
                for (shown, drawn) in self.picture.iter_mut().zip(drawn) {
                    *shown = phosphor(drawn, *shown, blend);
                }
            }
            Some(_) => {}
        }
        Picture {
            pixels: &self.picture,
            width: tia::WIDTH,
            height: self.tia.frame.height as usize,
        }
    }

    fn title(&self) -> String {
        match self.layout {
            Region::Ntsc => format!("Atari 2600   {}", self.name),
            Region::Pal => format!("Atari 2600   {}   PAL, 50 Hz", self.name),
        }
    }

    fn debug(&self) -> Vec<String> {
        let region = match self.region {
            Region::Ntsc => "NTSC",
            Region::Pal => "PAL",
        };
        vec![
            format!("ATARI 2600 {region} FRAME {}", self.frames),
            format!("CARTRIDGE {}", self.cartridge.name()),
            format!("LINES {}", self.tia.frame.lines_last_frame),
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

    /// The controls: the paddles turned, their buttons; the wheels turned;
    /// then the pins.
    fn input(&mut self, input: Input) {
        self.gamepads = [input.gamepad, input.gamepad_2];
        let height = self.tia.frame.height as f32;
        for paddle in 0..4 {
            let Some(player) = self.paddle_players[paddle] else {
                continue;
            };
            let Controller::Paddles { other_axis, .. } = self.controllers[paddle / 2] else {
                continue;
            };
            let pad = self.gamepads[player];
            let axis = |minus: bool, plus: bool| plus as i8 as f32 - minus as i8 as f32;
            let stick = match (player, other_axis) {
                (0, false) => input.joystick.x,
                (0, true) => input.joystick.y,
                (_, false) => axis(pad.left, pad.right),
                (_, true) => axis(pad.up, pad.down),
            };
            let pointer = input.pointer.at.filter(|_| player == 0).map(|(x, y)| {
                if other_axis {
                    y / height
                } else {
                    x / tia::WIDTH as f32
                }
            });
            self.paddles[paddle].turn(stick, pointer);
            let clicked = player == 0 && input.pointer.buttons.iter().any(|&held| held);
            self.paddle_fire[paddle] = pad.a || pad.b || clicked;
        }
        for (port, pad) in self.port_pads().into_iter().enumerate() {
            if self.controllers[port] == Controller::Driving {
                self.wheels[port].turn(pad.left, pad.right);
            }
        }
        for (n, paddle) in self.paddles.iter().enumerate() {
            let resistance = match self.controllers[n / 2] {
                Controller::Paddles { reversed, .. } => Some(paddle.resistance(reversed)),
                Controller::Keypad => continue,
                _ => None,
            };
            self.tia.paddle(n, resistance);
        }
        // The keypads' keys let go, once they've been held long enough to
        // be seen.
        if self.keypad_frames > 0 {
            self.keypad_frames -= 1;
        } else {
            self.keypad_keys = [0; 2];
        }
        self.controls_to_pins();
    }

    /// A key typed, a keypad's if it's one: held from now.
    fn key(&mut self, key: Key) {
        let Key::Char(typed) = key else { return };
        let ports: Vec<usize> = (0..2)
            .filter(|&port| self.controllers[port] == Controller::Keypad)
            .collect();
        let Some((port, key)) = controllers::keypad_key(typed as char, ports.len() == 2) else {
            return;
        };
        let Some(&port) = ports.get(port) else { return };
        self.keypad_keys[port] |= key;
        self.keypad_frames = KEYPAD_FRAMES;
        self.keypads_to_inputs();
    }

    fn rewinds(&self) -> bool {
        true
    }

    fn menu(&self) -> Vec<(&'static str, u8)> {
        vec![
            ("LEFT DIFFICULTY", LEFT_DIFFICULTY_KEY),
            ("RIGHT DIFFICULTY", RIGHT_DIFFICULTY_KEY),
            ("COLOUR OR BLACK AND WHITE", COLOUR_KEY),
        ]
    }

    /// F5 and F7, the difficulty switches; F8, colour or black and white.
    fn function_key(&mut self, key: u8) -> Option<String> {
        let switches = &mut self.switches;
        let said = match key {
            LEFT_DIFFICULTY_KEY | RIGHT_DIFFICULTY_KEY => {
                let n = (key == RIGHT_DIFFICULTY_KEY) as usize;
                switches.difficulty_a[n] = !switches.difficulty_a[n];
                let side = ["Left", "Right"][n];
                let ab = if switches.difficulty_a[n] { "A" } else { "B" };
                format!("{side} difficulty: {ab}")
            }
            COLOUR_KEY => {
                switches.colour = !switches.colour;
                if switches.colour {
                    "Colour".to_string()
                } else {
                    "Black and white".to_string()
                }
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

    fn identity(&self) -> String {
        let region = if self.region == Region::Pal {
            ", PAL"
        } else {
            ""
        };
        format!("Atari 2600, ROM {:08X}{region}", crc32(&self.cartridge.rom))
    }

    fn keep(&mut self, running: &mut Atari2600) {
        use std::mem::take;
        self.cartridge.take_rom(&mut running.cartridge);
        self.tia.keep(&mut running.tia);
        self.region = running.region;
        self.layout = running.layout;
        self.controllers = running.controllers;
        self.swap_ports = running.swap_ports;
        self.paddles = running.paddles;
        self.paddle_players = running.paddle_players;
        self.wheels = running.wheels;
        self.phosphor = running.phosphor;
        self.name = take(&mut running.name);
        self.gamepads = running.gamepads;
        self.switches = running.switches;
        self.picture = take(&mut running.picture);
        self.sound = running.sound.take();
        self.samples = take(&mut running.samples);
        self.controls_to_pins();
    }
}

/// The ROM at `path`, or in the zip there, the first .a26 or .bin in it.
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
        name.ends_with(".a26") || name.ends_with(".bin")
    };
    let (_, image) = crate::host::zip::file(&bytes, is_game)?;
    Ok(image)
}

/// What's known of the game at `path` without running it, its ROM `image`
/// if it could be had, as `machines::facts` has it: its CRC-32, its
/// cartridge, and why it won't load, if it won't.
pub fn facts_of(path: &Path, image: Result<Vec<u8>, String>) -> String {
    let fact = |image: &[u8]| -> Result<[String; 2], String> {
        let known = database::find(&crate::host::md5::md5(image));
        let kind = Kind::named(known.map_or("", |known| known.kind))?;
        let cartridge = Cartridge::new(image, kind, NTSC_CLOCK_HZ)?;
        Ok([
            format!("{:08X}", crc32(image)),
            format!("ATARI 2600 {}", cartridge.name()),
        ])
    };
    let [crc, board, problem] = match image.and_then(|image| fact(&image)) {
        Ok([crc, board]) => [crc, board, String::new()],
        Err(problem) => ["".into(), "".into(), problem],
    };
    let problem = problem.replace(['\t', '\n'], " ");
    format!("{crc}\t{board}\t0\t0\t{problem}\t{}", path.display())
}

/// A dot `drawn` as the phosphor shows it, `shown` the last, `blend` how
/// much of it's left: each of its colours the brighter.
fn phosphor(drawn: u32, shown: u32, blend: f32) -> u32 {
    (0..3).fold(0, |colour, n| {
        let shift = n * 8;
        let drawn = (drawn >> shift) & 0xFF;
        let faded = (((shown >> shift) & 0xFF) as f32 * blend) as u32;
        colour | drawn.max(faded) << shift
    })
}

/// The console of `region`, its picture's layout `layout`'s, with `image`
/// in it, its cartridge as `known` says, reset.
fn console(
    image: &[u8],
    known: Option<&database::Known>,
    region: Region,
    layout: Region,
) -> Result<CPU<Atari2600>, String> {
    let kind = Kind::named(known.map_or("", |known| known.kind))?;
    let mut cartridge = Cartridge::new(image, kind, clock_hz(region))?;
    cartridge.start_bank = known.and_then(|known| known.start_bank);
    cartridge.reset();
    let mut cpu = CPU::with_bus(Atari2600::new(cartridge, region, layout));
    cpu.variant = Variant::Nmos6502;
    cpu.clock_hz = clock_hz(region);
    cpu.reset();
    Ok(cpu)
}

/// The layout of the frames `image` makes in its first second: PAL's if
/// they're nearer its 312 lines than NTSC's 262, counted from VSYNC to VSYNC
/// however short it is.
fn layout_of(image: &[u8], known: Option<&database::Known>, region: Region) -> Region {
    let Ok(mut cpu) = console(image, known, region, Region::Ntsc) else {
        return Region::Ntsc;
    };
    let mut lines: Vec<u32> = (0..60)
        .map(|_| {
            cpu.run_frame();
            cpu.memory.tia.frame.lines_between_vsyncs
        })
        .skip(30)
        .collect();
    lines.sort_unstable();
    if lines[lines.len() / 2] > 287 {
        Region::Pal
    } else {
        Region::Ntsc
    }
}

/// The 2600, on whichever host, with the game at `rom` (a ROM, or a zip of
/// one), its region `region`, or as Stella's properties or its name say.
pub fn build(rom: &str, region: Option<Region>) -> Result<CPU<Atari2600>, String> {
    let path = Path::new(rom);
    let image = image(path).map_err(|e| format!("{rom}: {e}"))?;
    let known = database::find(&crate::host::md5::md5(&image));
    let formatted = match known.map_or("", |known| known.format) {
        "NTSC" | "NTSC50" => Some(Region::Ntsc),
        "PAL" | "PAL60" | "SECAM" => Some(Region::Pal),
        _ => None,
    };
    let named = path
        .file_name()
        .and_then(|name| Region::from_name(&name.to_string_lossy()));
    let guess = region.or(formatted).or(named).unwrap_or_default();
    let layout = layout_of(&image, known, guess);
    // PAL's if its name or its frames say so: a game of 312 lines is a PAL
    // game, whatever it's named, as Stella has it.
    let pal = named == Some(Region::Pal) || layout == Region::Pal;
    let region = region
        .or(formatted)
        .unwrap_or(if pal { Region::Pal } else { Region::Ntsc });
    let mut cpu = console(&image, known, region, layout).map_err(|e| format!("{rom}: {e}"))?;
    let atari = &mut cpu.memory;
    for (port, controller) in atari.controllers.iter_mut().enumerate() {
        let named = known.and_then(|known| known.controllers[port]);
        *controller = named.map_or_else(|| Controller::found(&image, port == 1), Controller::named);
    }
    if let Some(known) = known {
        atari.swap_ports = known.swap_ports;
        for (switch, said) in atari
            .switches
            .difficulty_a
            .iter_mut()
            .zip(known.difficulty_a)
        {
            *switch = said.unwrap_or(*switch);
        }
        if known.phosphor {
            atari.phosphor = Some(known.blend.map_or(0.5, |blend| blend as f32 / 100.0));
        }
    }
    // The paddles, the first player's the first, the second's the next.
    let mut players = 0..2;
    let swap = known.is_some_and(|known| known.swap_paddles);
    for port in 0..2 {
        if matches!(atari.controllers[port], Controller::Paddles { .. }) {
            let (a, b) = (players.next(), players.next());
            let [a, b] = if swap { [b, a] } else { [a, b] };
            atari.paddle_players[2 * port..2 * port + 2].copy_from_slice(&[a, b]);
        }
    }
    atari.controls_to_pins();
    atari.name = path
        .file_stem()
        .map_or(String::new(), |name| name.to_string_lossy().into_owned());
    Ok(cpu)
}

/// Frames a second, `atari`'s: a line's 76 cycles, as many lines as its
/// layout's TV has.
pub fn frames_per_second(atari: &Atari2600) -> f64 {
    let lines = match atari.layout {
        Region::Ntsc => 262.0,
        Region::Pal => 312.0,
    };
    clock_hz(atari.region) as f64 / 76.0 / lines
}

/// How big its picture's shown, its layout `layout`'s: each dot as that
/// TV shows it, 3 times over.
pub fn size(layout: Region) -> (u32, u32) {
    let height = Tia::new(layout).frame.height;
    (
        (tia::WIDTH as f32 * dot_shape(layout) * 3.0).round() as u32,
        height * 3,
    )
}

/// The 2600 as `build` has it, run in a window, `fullscreen` or not.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(rom: &str, fullscreen: bool, region: Option<Region>) {
    let cpu = build(rom, region).unwrap_or_else(|e| panic!("{e}"));
    let layout = cpu.memory.layout;
    let height = cpu.memory.tia.frame.height as usize;
    let title = cpu.memory.title();
    let window = Window::new(&title, tia::WIDTH, height, size(layout), fullscreen);
    Host::new(cpu, Box::new(window), save_state::path(Path::new(rom))).run();
}
