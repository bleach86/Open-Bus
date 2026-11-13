//! Skink (a working name): a 6502 computer of our own, made for GeckOS to
//! run on, as little as possible to port: a 6502 at 2 MHz, 64K of RAM, an
//! 80 by 25 screen of text in it, a keyboard that gives ASCII, an interrupt
//! each frame, a 6522 VIA and a 6551 ACIA.
//!
//! Its memory map: RAM all of it, but for its I/O page.
//!
//! | `$0000-$7FFF` | RAM: zero page, stack, and the rest       |
//! | `$8000-$87CF` | the screen's characters, a row of 80 at a time |
//! | `$8800-$8FCF` | their colours: foreground low, background high |
//! | `$9000-$90FF` | I/O                                        |
//! | `$9100-$FFFF` | RAM: the boot image at its top, loaded as it's turned on |
//!
//! Its I/O: `$9000`, the interrupts: bit 7, a frame's begun (60 a second),
//! a write clearing it; `$9001`, which are on: bit 7, the frame's.
//! `$9010-$901F`, the VIA; `$9020`, a key: the next typed, as ASCII (and
//! GeckOS's codes for the arrows, $80 to $83), 0 if there's none, a read
//! taking it; `$9021`, bit 7 a key waiting; `$9030-$9033`, the ACIA. What
//! else there is reads $FF.
//!
//! A character's bit 7 shows it inverse; its colour's 16, as GeckOS
//! numbers them, the colours coming up light grey on black. The font's
//! ours, 8 by 8, the host's.
//!
//! Its boot image: a ROM image, in RAM to the top of memory, its RESET
//! vector where it starts.

#[cfg(test)]
mod tests {
    mod machine;
}

use crate::chips::acia6551::Acia6551;
use crate::chips::via6522::Via6522;
use crate::cpu::bus::Bus;
use crate::cpu::{CPU, Variant};
use crate::host::display::Key;
use crate::host::font;
use crate::host::machine::{Input, Machine, Picture};
#[cfg(not(target_arch = "wasm32"))]
use crate::host::run::Host;
use crate::host::save_state;
#[cfg(not(target_arch = "wasm32"))]
use crate::host::window::Window;
use crate::host::zip::crc32;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;

pub const CLOCK_HZ: u32 = 2_000_000;
/// A frame's cycles, 60 frames a second.
pub const FRAME_CYCLES: u32 = CLOCK_HZ / 60;

/// The screen: its size, in characters, and where they and their colours
/// are.
pub const COLUMNS: usize = 80;
pub const ROWS: usize = 25;
pub const SCREEN: usize = 0x8000;
pub const COLOURS: usize = 0x8800;
/// A character's dots, across and down; the picture's.
const CELL: usize = 8;
pub const WIDTH: usize = COLUMNS * CELL;
pub const HEIGHT: usize = ROWS * CELL;

/// The I/O page's registers.
const INTERRUPTS: u16 = 0x9000;
const INTERRUPTS_ON: u16 = 0x9001;
const VIA: u16 = 0x9010;
const KEY: u16 = 0x9020;
const KEY_WAITING: u16 = 0x9021;
const ACIA: u16 = 0x9030;
/// The interrupts' bit for a frame.
const FRAME: u8 = 0x80;
/// The keys held to be read, at most.
const KEYS_HELD: usize = 32;
/// The colours as they come up: light grey on black.
const COLOURS_ON: u8 = 0x0E;

/// The 16 colours, as GeckOS numbers them: black, dark and light grey,
/// blue, green, cyan, red, purple, brown, yellow, white.
pub const PALETTE: [u32; 16] = [
    0x000000, 0x555555, 0x0000AA, 0x5555FF, 0x00AA00, 0x55FF55, 0x00AAAA, 0x55FFFF, 0xAA0000,
    0xFF5555, 0xAA00AA, 0xFF55FF, 0xAA5500, 0xFFFF55, 0xAAAAAA, 0xFFFFFF,
];

/// In a save state, all but what's the host's: the keys waiting, the
/// picture, the serial port's other end.
#[derive(Serialize, Deserialize)]
pub struct Skink {
    pub ram: Vec<u8>,
    pub via: Via6522,
    pub acia: Acia6551,
    interrupts: u8,
    interrupts_on: u8,
    /// The frame's cycles so far, and the frames.
    cycles: u32,
    pub frames: u64,
    frame_ended: bool,
    #[serde(skip)]
    keys: VecDeque<u8>,
    #[serde(skip)]
    reset_held: bool,
    #[serde(skip)]
    picture: Vec<u32>,
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    image_crc: u32,
    #[serde(skip)]
    samples: Vec<f32>,
}

impl Skink {
    /// One turned on with `image` loaded to the top of memory, the rest of
    /// RAM 0s, the colours light grey on black.
    pub fn new(image: &[u8]) -> Result<Skink, String> {
        let top = 0x10000 - image.len();
        if image.is_empty() || top < 0x9100 {
            return Err(format!(
                "a boot image of {} bytes: it's to be $9100 to $FFFF, at most {}",
                image.len(),
                0x10000 - 0x9100
            ));
        }
        let mut ram = vec![0; 0x10000];
        ram[COLOURS..COLOURS + COLUMNS * ROWS].fill(COLOURS_ON);
        ram[top..].copy_from_slice(image);
        Ok(Skink {
            ram,
            via: Via6522::new(),
            acia: Acia6551::new(CLOCK_HZ),
            interrupts: 0,
            interrupts_on: 0,
            cycles: 0,
            frames: 0,
            frame_ended: false,
            keys: VecDeque::new(),
            reset_held: false,
            picture: Vec::new(),
            name: String::new(),
            image_crc: crc32(image),
            samples: Vec::new(),
        })
    }

    /// A key typed: as ASCII, or GeckOS's code for an arrow; dropped if
    /// too many are waiting.
    fn typed(&mut self, code: u8) {
        if self.keys.len() < KEYS_HELD {
            self.keys.push_back(code);
        }
    }

    /// The screen's row `row`, as text.
    fn row_text(&self, row: usize) -> String {
        let at = SCREEN + row * COLUMNS;
        self.ram[at..at + COLUMNS]
            .iter()
            .map(|&c| match c & 0x7F {
                c @ 0x20..=0x7E => c as char,
                _ => ' ',
            })
            .collect::<String>()
            .trim_end()
            .to_string()
    }
}

/// Whether `addr` is the I/O page's.
fn is_io(addr: u16) -> bool {
    addr & 0xFF00 == 0x9000
}

impl Bus for Skink {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            INTERRUPTS => self.interrupts,
            INTERRUPTS_ON => self.interrupts_on,
            VIA..=0x901F => self.via.read(addr as u8 & 0x0F),
            KEY => self.keys.pop_front().unwrap_or(0),
            KEY_WAITING => {
                if self.keys.is_empty() {
                    0
                } else {
                    0x80
                }
            }
            ACIA..=0x9033 => self.acia.read(addr as u8 & 3),
            _ if is_io(addr) => 0xFF,
            _ => self.ram[addr as usize],
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            INTERRUPTS => self.interrupts = 0,
            INTERRUPTS_ON => self.interrupts_on = value,
            VIA..=0x901F => self.via.write(addr as u8 & 0x0F, value),
            ACIA..=0x9033 => self.acia.write(addr as u8 & 3, value),
            _ if is_io(addr) => {}
            _ => self.ram[addr as usize] = value,
        }
    }

    /// A cycle: the VIA's and the ACIA's; a frame begun, its interrupt.
    fn tick(&mut self) {
        self.via.tick();
        self.acia.tick();
        self.cycles += 1;
        if self.cycles == FRAME_CYCLES {
            self.cycles = 0;
            self.frames += 1;
            self.interrupts |= FRAME;
            self.frame_ended = true;
        }
    }

    fn reset_held(&self) -> bool {
        self.reset_held
    }

    fn irq(&self) -> bool {
        self.interrupts & self.interrupts_on != 0 || self.via.irq() || self.acia.irq()
    }

    fn hand_back(&mut self) -> bool {
        std::mem::take(&mut self.frame_ended)
    }
}

impl Machine for Skink {
    fn picture(&mut self) -> Picture<'_> {
        self.picture.clear();
        self.picture.resize(WIDTH * HEIGHT, 0);
        for cell in 0..COLUMNS * ROWS {
            let (char, colour) = (self.ram[SCREEN + cell], self.ram[COLOURS + cell]);
            let (mut fg, mut bg) = (
                PALETTE[colour as usize & 0x0F],
                PALETTE[colour as usize >> 4],
            );
            if char & 0x80 != 0 {
                (fg, bg) = (bg, fg);
            }
            let (column, row) = (cell % COLUMNS, cell / COLUMNS);
            for line in 0..CELL {
                let dots = font::ascii(char, line);
                let at = (row * CELL + line) * WIDTH + column * CELL;
                for x in 0..CELL {
                    self.picture[at + x] = if dots >> x & 1 != 0 { fg } else { bg };
                }
            }
        }
        Picture {
            pixels: &self.picture,
            width: WIDTH,
            height: HEIGHT,
        }
    }

    fn title(&self) -> String {
        format!("Skink   {}", self.name)
    }

    fn sound_on(&mut self) {}

    fn sound(&mut self) -> &mut Vec<f32> {
        &mut self.samples
    }

    fn debug(&self) -> Vec<String> {
        vec![
            format!("SKINK FRAME {}", self.frames),
            format!(
                "INTERRUPTS {:02X} ON {:02X} KEYS {}",
                self.interrupts,
                self.interrupts_on,
                self.keys.len()
            ),
        ]
    }

    /// A key: Return, Backspace and Escape as ASCII has them, the arrows as
    /// GeckOS's codes.
    fn key(&mut self, key: Key) {
        let code = match key {
            Key::Char(c) => c,
            Key::Left => 0x80,
            Key::Right => 0x81,
            Key::Up => 0x82,
            Key::Down => 0x83,
        };
        self.typed(code);
    }

    fn keyboard(&self) -> bool {
        true
    }

    fn type_text(&mut self, text: &str) {
        for c in text.replace("\r\n", "\n").chars() {
            match c {
                '\n' | '\r' => self.typed(b'\r'),
                '\t' => self.typed(b'\t'),
                c if c.is_ascii() && !c.is_ascii_control() => self.typed(c as u8),
                _ => {}
            }
        }
    }

    fn screen_text(&self) -> Option<String> {
        let rows: Vec<String> = (0..ROWS).map(|row| self.row_text(row)).collect();
        let first = rows.iter().position(|row| !row.is_empty())?;
        let last = rows.iter().rposition(|row| !row.is_empty())?;
        Some(rows[first..=last].join("\n"))
    }

    fn input(&mut self, _input: Input) {}

    fn reset_button(&mut self, held: bool) {
        self.reset_held = held;
    }

    fn identity(&self) -> String {
        format!("Skink, image {:08X}", self.image_crc)
    }

    fn keep(&mut self, running: &mut Skink) {
        use std::mem::take;
        self.keys = take(&mut running.keys);
        self.picture = take(&mut running.picture);
        self.name = take(&mut running.name);
        self.image_crc = running.image_crc;
        self.acia.incoming = take(&mut running.acia.incoming);
        self.acia.outgoing = take(&mut running.acia.outgoing);
        self.acia.connected = running.acia.connected;
    }
}

/// Skink with the boot image at `image`, turned on.
pub fn build(image: &str) -> Result<CPU<Skink>, String> {
    let path = Path::new(image);
    let bytes = crate::host::files::read(path).map_err(|e| format!("{image}: {e}"))?;
    let mut skink = Skink::new(&bytes).map_err(|e| format!("{image}: {e}"))?;
    skink.name = path
        .file_stem()
        .map_or(String::new(), |name| name.to_string_lossy().into_owned());
    let mut cpu = CPU::with_bus(skink);
    cpu.variant = Variant::Nmos6502;
    cpu.clock_hz = CLOCK_HZ;
    cpu.reset();
    Ok(cpu)
}

/// How big its picture's shown: each dot twice across, its lines four
/// times, for 80 columns on a 4 by 3 screen, near enough.
pub fn size() -> (u32, u32) {
    ((WIDTH * 2) as u32, (HEIGHT * 4) as u32)
}

/// Skink as `build` has it, run in a window, `fullscreen` or not.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(image: &str, fullscreen: bool) {
    let cpu = build(image).unwrap_or_else(|e| panic!("{e}"));
    let title = cpu.memory.title();
    let window = Window::new(&title, WIDTH, HEIGHT, size(), fullscreen);
    Host::new(cpu, Box::new(window), save_state::path(Path::new(image))).run();
}
