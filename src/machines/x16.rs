//! The Commander X16: a WDC 65C02 at 8 MHz, 40K of RAM and 512K more in
//! 8K banks, 32 banks of ROM, two 6522 VIAs, VERA's video, and an SMC on
//! I2C with its keyboard; as its Programmer's Reference has it, and the X16
//! emulator (x16emu, BSD-2) does.
//!
//! Its memory map:
//!
//! | `$0000-$9EFF` | RAM; `$00` the RAM bank, `$01` the ROM bank |
//! | `$9F00-$9F0F` | VIA 1: I2C, the SNES controllers, the IEC bus |
//! | `$9F10-$9F1F` | VIA 2: the user port, nothing on it            |
//! | `$9F20-$9F3F` | VERA                                           |
//! | `$9F40-$9F41` | the YM2151                                     |
//! | `$9F60-$9FFF` | the expansion cards', none                     |
//! | `$A000-$BFFF` | RAM, its bank of 64                            |
//! | `$C000-$FFFF` | ROM, its bank of 32; 32 and up a cartridge's   |
//!
//! VIA 1's port A: PA0 and PA1 I2C's SDA and SCL, open drain as the
//! KERNAL has them, a pin let go by setting it an input; PA2 and PA3 the
//! SNES controllers' latch and clock, PA7 to PA4 their data, controller 1
//! to 4. Its port B: PB3 to PB5 the IEC bus's ATN, CLK and DATA out, each
//! pulling its line low, and PB6 and PB7 CLK and DATA in, as the lines
//! are: nothing's on it, so as the X16 has them.
//!
//! The YM2151 at 2 MHz, as the cards from `$9FA0` are: each access takes
//! 3 cycles more. It takes its registers, but makes no sound yet, and
//! never says it's busy. What isn't anything reads as the bus was left,
//! the address's high byte.
//!
//! The 65C02's VPB, low as it reads a vector, has them read from ROM
//! bank 0, `$01` as it was: the KERNAL's handlers in RAM, saving it and
//! setting bank 0 themselves.
//!
//! Its keyboard's characters are typed as its keys (`keyboard`); its SNES
//! controllers 1 and 2 are the host's gamepads, their A and B the SNES's.
//! Its mouse is the host's pointer: how far it's moved over the picture
//! each frame, a dot a dot, and its buttons, sent as the mouse's packets.
//! The SMC asked to reset it, it resets; to power off, it's held in reset
//! till its Reset button. Its SD card's an image of one (`sd_card`), if
//! it's given, or roms/x16/sdcard.img, if that's there; its light's the
//! SMC's activity light, or the card being read or written. Its clock's
//! the host's time as it's turned on, kept by its battery since; its
//! clock's RAM is kept in roms/x16/nvram.bin, read as it's run and saved
//! as it's written.

#[cfg(test)]
mod tests {
    mod i2c;
    mod machine;
    mod sd_card;
    mod vera;
}

pub mod i2c;
pub mod keyboard;
pub mod sd_card;
pub mod vera;

use crate::chips::via6522::Via6522;
use crate::cpu::bus::Bus;
use crate::cpu::{CPU, Variant};
use crate::host::clock;
use crate::host::display::{Gamepad, Key, Pointer};
use crate::host::files;
use crate::host::machine::{Input, Machine, Picture};
#[cfg(not(target_arch = "wasm32"))]
use crate::host::run::Host;
#[cfg(not(target_arch = "wasm32"))]
use crate::host::save_state;
#[cfg(not(target_arch = "wasm32"))]
use crate::host::window::Window;
use crate::host::zip::crc32;
use i2c::{Asked, I2c};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use vera::{HEIGHT, Vera, WIDTH};

pub const CLOCK_HZ: u32 = 8_000_000;

/// Where its ROM is, unless it's said: roms/x16/rom.bin, or a release's
/// folder's in roms/x16/, the last by name.
pub const ROM_FOLDER: &str = "roms/x16";
/// The RTC's 64 bytes of RAM, kept by its battery, as they're kept here.
pub const NVRAM_FILE: &str = "roms/x16/nvram.bin";

const RAM_TOP: usize = 0x9F00;
const BANK: usize = 0x2000;
const RAM_BANKS: usize = 64;
const ROM_BANK: usize = 0x4000;
const ROM_BANKS: usize = 32;
/// Cycles more for an access at 2 MHz.
const SLOW: u32 = 3;
/// An NMI pulled for as long as this.
const NMI_CYCLES: u8 = 8;

/// VIA 1's port A pins.
const SDA: u8 = 0x01;
const SCL: u8 = 0x02;
const LATCH: u8 = 0x04;
const CLOCK: u8 = 0x08;
/// Its port B's: the IEC bus's CLK and DATA out, and in.
const CLK_OUT: u8 = 0x10;
const DATA_OUT: u8 = 0x20;
const CLK_IN: u8 = 0x40;
const DATA_IN: u8 = 0x80;

/// A SNES controller: its buttons' shift register, loaded as the latch is
/// high and shifted as the clock rises, each bit low for a button held;
/// what it's putting out.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Snes {
    shifting: u16,
    data: bool,
}

impl Snes {
    /// Its buttons, as its shift register's loaded: B, Y, Select, Start,
    /// up, down, left, right, A, X, L, R, each 0 if held; then 4 1s.
    fn buttons(pad: Gamepad) -> u16 {
        let held = [
            pad.b, false, pad.select, pad.start, pad.up, pad.down, pad.left, pad.right, pad.a,
        ];
        let held = held
            .iter()
            .enumerate()
            .fold(0u16, |bits, (n, &on)| bits | (on as u16) << n);
        !held & 0x0FFF | 0xF000
    }

    /// Its next bit out: the shift register's first, as the rest move on
    /// (0s after them).
    fn shift(&mut self) {
        self.data = self.shifting & 1 != 0;
        self.shifting >>= 1;
    }
}

/// In a save state, all but what's the host's, and its ROM.
#[derive(Serialize, Deserialize)]
pub struct X16 {
    /// Its RAM to `$9EFF` (`$00` and `$01` its bank registers), and its
    /// banks'.
    pub ram: Vec<u8>,
    pub banked: Vec<u8>,
    pub vias: [Via6522; 2],
    pub vera: Vera,
    pub i2c: I2c,
    pads: [Snes; 2],
    /// The SNES latch and clock as they were.
    latch: bool,
    clock: bool,
    /// The YM2151's register chosen, and its registers.
    ym_register: u8,
    ym: Vec<u8>,
    nmi: u8,
    /// VPB low, a vector being read: from ROM bank 0, whatever `$01` says.
    #[serde(skip)]
    vector_pull: bool,
    /// Reset, as the SMC's been asked, at the next cycle; powered off.
    resetting: bool,
    powered_off: bool,
    /// Cycles besides those ticked, for slow accesses.
    stolen: u32,
    #[serde(skip)]
    rom: Vec<u8>,
    /// The keys typed, waiting for room in the SMC.
    #[serde(skip)]
    typing: VecDeque<u8>,
    #[serde(skip)]
    gamepads: [Gamepad; 2],
    #[serde(skip)]
    mouse: Mouse,
    #[serde(skip)]
    reset_held: bool,
    /// Where its clock's RAM is kept, if it is.
    #[serde(skip)]
    nvram_file: Option<PathBuf>,
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    rom_crc: u32,
    #[serde(skip)]
    samples: Vec<f32>,
}

/// The host's pointer as the mouse has it: where it was, as far as its
/// moves have been sent, over the picture; its buttons as last sent.
#[derive(Clone, Copy, Debug, Default)]
struct Mouse {
    at: Option<(f32, f32)>,
    buttons: u8,
}

impl X16 {
    /// One turned on with `rom`, its banks from 0, up to 32 of 16K; past
    /// it, flash as it's erased.
    pub fn new(rom: &[u8]) -> Result<X16, String> {
        if rom.is_empty() || rom.len() > ROM_BANKS * ROM_BANK {
            return Err(format!(
                "a ROM of {} bytes: it's to be 512K at most",
                rom.len()
            ));
        }
        let mut flash = vec![0xFF; ROM_BANKS * ROM_BANK];
        flash[..rom.len()].copy_from_slice(rom);
        let mut x16 = X16 {
            ram: vec![0; RAM_TOP],
            banked: vec![0; RAM_BANKS * BANK],
            vias: [Via6522::new(), Via6522::new()],
            vera: Vera::new(),
            i2c: I2c::new(),
            pads: [Snes::default(); 2],
            latch: false,
            clock: false,
            ym_register: 0,
            ym: vec![0; 256],
            nmi: 0,
            vector_pull: false,
            resetting: false,
            powered_off: false,
            stolen: 0,
            rom: flash,
            typing: VecDeque::new(),
            gamepads: [Gamepad::default(); 2],
            mouse: Mouse::default(),
            reset_held: false,
            nvram_file: None,
            name: String::new(),
            rom_crc: crc32(rom),
            samples: Vec::new(),
        };
        x16.i2c.rtc.set(clock::now());
        x16.port_a_changed();
        Ok(x16)
    }

    /// What's on RESET: the bank registers, the VIAs, VERA, I2C's lines.
    fn reset_devices(&mut self) {
        (self.ram[0], self.ram[1]) = (0, 0);
        for via in &mut self.vias {
            via.reset();
        }
        self.vera.reset();
        self.port_a_changed();
        self.port_b_changed();
    }

    /// VIA 1's port A as it's set: I2C's lines as the CPU has them, and
    /// SDA as a device does; the SNES controllers latched or clocked, and
    /// their data.
    fn port_a_changed(&mut self) {
        let via = &self.vias[0];
        // A pin's high unless it's an output set low: pulled up, let go.
        let high = |pin: u8| via.ddra & pin == 0 || via.ora & pin != 0;
        let (scl, sda, latch, clock) = (high(SCL), high(SDA), high(LATCH), high(CLOCK));
        self.i2c.lines(scl, sda);
        if latch {
            for (pad, gamepad) in self.pads.iter_mut().zip(self.gamepads) {
                pad.shifting = Snes::buttons(gamepad);
                pad.shift();
            }
        } else if clock && !self.clock {
            for pad in &mut self.pads {
                pad.shift();
            }
        }
        (self.latch, self.clock) = (latch, clock);
        // Controllers 3 and 4 aren't there: their data's pulled up.
        let data = (self.pads[0].data as u8) << 7 | (self.pads[1].data as u8) << 6 | 0x30;
        let lines = self.i2c.sda() as u8 | (scl as u8) << 1 | (latch as u8) << 2;
        self.vias[0].pa_in = lines | (clock as u8) << 3 | data;
    }

    /// VIA 1's port B as it's set: the IEC bus's CLK and DATA low as it
    /// pulls them, nothing else on it.
    fn port_b_changed(&mut self) {
        let via = &self.vias[0];
        let pulling = |pin: u8| via.ddrb & pin != 0 && via.orb & pin != 0;
        let mut pins = 0xFF;
        if pulling(CLK_OUT) {
            pins &= !CLK_IN;
        }
        if pulling(DATA_OUT) {
            pins &= !DATA_IN;
        }
        self.vias[0].pb_in = pins;
    }

    /// An access at 2 MHz: its cycles more.
    fn slow(&mut self) {
        for _ in 0..SLOW {
            self.cycle();
        }
        self.stolen += SLOW;
    }

    /// A cycle of all that's clocked with the CPU.
    fn cycle(&mut self) {
        for via in &mut self.vias {
            via.tick();
        }
        self.vera.tick();
        self.i2c.tick();
        self.nmi = self.nmi.saturating_sub(1);
    }

    /// Its clock's RAM kept in the file at `path`: read from it, if it's
    /// there, and saved to it as it's written.
    pub fn keep_nvram(&mut self, path: &Path) {
        if let Ok(saved) = files::read(path) {
            let ram = &mut self.i2c.rtc.ram;
            let len = saved.len().min(ram.len());
            ram[..len].copy_from_slice(&saved[..len]);
        }
        self.nvram_file = Some(path.to_path_buf());
    }

    /// The keys typed moved on to the SMC, as it has room.
    fn type_on(&mut self) {
        while let Some(&code) = self.typing.front() {
            if !self.i2c.smc.key(code) {
                break;
            }
            self.typing.pop_front();
        }
    }

    /// The pointer's moves since the last frame, and its buttons, sent as
    /// the mouse's packets, as many as the SMC has room for; what isn't
    /// sent is sent with the next.
    fn move_mouse(&mut self, pointer: Pointer) {
        let buttons = pointer.buttons[0] as u8 | (pointer.buttons[1] as u8) << 1;
        let (Some(from), Some(to)) = (self.mouse.at, pointer.at) else {
            // Onto the picture, or off it: from there on.
            self.mouse.at = pointer.at;
            if buttons != self.mouse.buttons && self.i2c.smc.mouse(buttons, 0, 0) {
                self.mouse.buttons = buttons;
            }
            return;
        };
        // Up, for the mouse.
        let (mut x, mut y) = (
            (to.0 - from.0).round() as i32,
            (from.1 - to.1).round() as i32,
        );
        let mut sent = (0, 0);
        while x != 0 || y != 0 || buttons != self.mouse.buttons {
            let (dx, dy) = (x.clamp(-256, 255), y.clamp(-256, 255));
            if !self.i2c.smc.mouse(buttons, dx as i16, dy as i16) {
                break;
            }
            self.mouse.buttons = buttons;
            (x, y, sent) = (x - dx, y - dy, (sent.0 + dx, sent.1 + dy));
        }
        self.mouse.at = Some((from.0 + sent.0 as f32, from.1 - sent.1 as f32));
    }

    /// Row `row` of a text map at `base`, `width` cells to a row, its first
    /// `across` as text: their screen codes as upper case and graphics have
    /// them, the graphics as spaces.
    fn row_text(&self, base: usize, width: usize, row: usize, across: usize) -> String {
        (0..across)
            .map(|column| {
                let code = self.vera.peek(base + (row * width + column) * 2) & 0x7F;
                match code {
                    0x00 => '@',
                    0x01..=0x1A => (b'A' + code - 1) as char,
                    0x1B => '[',
                    0x1D => ']',
                    0x20..=0x3F => code as char,
                    _ => ' ',
                }
            })
            .collect::<String>()
            .trim_end()
            .to_string()
    }
}

impl Bus for X16 {
    fn read(&mut self, addr: u16) -> u8 {
        let open = (addr >> 8) as u8;
        match addr {
            0x0000..=0x9EFF => self.ram[addr as usize],
            0x9F00..=0x9F0F => self.vias[0].read(addr as u8),
            0x9F10..=0x9F1F => self.vias[1].read(addr as u8),
            0x9F20..=0x9F3F => self.vera.read(addr as u8),
            0x9F40..=0x9F5F => {
                self.slow();
                // The YM2151's status, never busy; its other address, nothing.
                if addr & 1 != 0 { 0 } else { open }
            }
            0x9F60..=0x9F9F => open,
            0x9FA0..=0x9FFF => {
                self.slow();
                open
            }
            0xA000..=0xBFFF => match self.ram[0] as usize {
                bank if bank < RAM_BANKS => self.banked[bank * BANK + (addr as usize - 0xA000)],
                _ => open,
            },
            0xC000..=0xFFFF if self.vector_pull => self.rom[addr as usize - 0xC000],
            0xC000..=0xFFFF => match self.ram[1] as usize {
                bank if bank < ROM_BANKS => self.rom[bank * ROM_BANK + (addr as usize - 0xC000)],
                _ => open,
            },
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x9EFF => self.ram[addr as usize] = value,
            0x9F00..=0x9F0F => {
                self.vias[0].write(addr as u8, value);
                match addr & 0x0F {
                    0x1 | 0x3 | 0xF => self.port_a_changed(),
                    0x0 | 0x2 => self.port_b_changed(),
                    _ => {}
                }
            }
            0x9F10..=0x9F1F => self.vias[1].write(addr as u8, value),
            0x9F20..=0x9F3F => self.vera.write(addr as u8, value),
            0x9F40..=0x9F5F => {
                self.slow();
                if addr & 1 == 0 {
                    self.ym_register = value;
                } else {
                    self.ym[self.ym_register as usize] = value;
                }
            }
            0x9F60..=0x9F9F => {}
            0x9FA0..=0x9FFF => self.slow(),
            0xA000..=0xBFFF => {
                if let bank @ 0..RAM_BANKS = self.ram[0] as usize {
                    self.banked[bank * BANK + (addr as usize - 0xA000)] = value;
                }
            }
            // ROM; a cartridge's, none.
            0xC000..=0xFFFF => {}
        }
    }

    /// A cycle: the chips'; and what the SMC's been asked, done.
    fn tick(&mut self) {
        self.cycle();
        if self.resetting {
            self.resetting = false;
            self.reset_devices();
        }
        match self.i2c.smc.asked.take() {
            Some(Asked::Reset) => self.resetting = true,
            Some(Asked::PowerOff) => {
                self.powered_off = true;
                self.vera.picture.fill(0);
            }
            Some(Asked::Nmi) => self.nmi = NMI_CYCLES,
            None => {}
        }
    }

    fn reset_held(&self) -> bool {
        self.reset_held || self.resetting || self.powered_off
    }

    fn vector_pull(&mut self, pulling: bool) {
        self.vector_pull = pulling;
    }

    fn irq(&self) -> bool {
        self.vera.irq() || self.vias[0].irq() || self.vias[1].irq()
    }

    fn nmi(&self) -> bool {
        self.nmi > 0
    }

    fn stolen(&mut self) -> u32 {
        std::mem::take(&mut self.stolen)
    }

    fn hand_back(&mut self) -> bool {
        self.vera.frame_ended()
    }
}

impl Machine for X16 {
    fn picture(&mut self) -> Picture<'_> {
        Picture {
            pixels: &self.vera.picture,
            width: WIDTH,
            height: HEIGHT,
        }
    }

    fn title(&self) -> String {
        format!("Commander X16   {}", self.name)
    }

    fn sound_on(&mut self) {}

    fn sound(&mut self) -> &mut Vec<f32> {
        &mut self.samples
    }

    fn debug(&self) -> Vec<String> {
        vec![
            format!(
                "X16 RAM BANK {:02X} ROM BANK {:02X}",
                self.ram[0], self.ram[1]
            ),
            format!(
                "KEYS {} TYPING {}",
                self.i2c.smc.keys.len(),
                self.typing.len()
            ),
        ]
    }

    fn key(&mut self, key: Key) {
        self.typing.extend(keyboard::codes(key));
        self.type_on();
    }

    fn keyboard(&self) -> bool {
        true
    }

    /// Text pasted: its letters as they're written; but with none in lower
    /// case, all of them unshifted, as letters, not the graphics they'd be
    /// shifted in the X16's first character set, upper case and graphics.
    fn type_text(&mut self, text: &str) {
        let mut text = text.replace("\r\n", "\n");
        if !text.bytes().any(|c| c.is_ascii_lowercase()) {
            text.make_ascii_lowercase();
        }
        for c in text.chars() {
            match c {
                '\n' | '\r' => self.key(Key::Char(13)),
                c if c.is_ascii() => self.key(Key::Char(c as u8)),
                _ => {}
            }
        }
    }

    fn screen_text(&self) -> Option<String> {
        let (base, width, across, down) = self.vera.text_map(1)?;
        let rows: Vec<String> = (0..down)
            .map(|row| self.row_text(base, width, row, across))
            .collect();
        let first = rows.iter().position(|row| !row.is_empty())?;
        let last = rows.iter().rposition(|row| !row.is_empty())?;
        Some(rows[first..=last].join("\n"))
    }

    fn input(&mut self, input: Input) {
        self.gamepads = [input.gamepad, input.gamepad_2];
        self.type_on();
        self.move_mouse(input.pointer);
    }

    fn light(&mut self) -> bool {
        let card = std::mem::take(&mut self.vera.sd_card().used);
        self.i2c.smc.activity_light || card
    }

    fn has_mouse(&self) -> bool {
        true
    }

    /// Its clock's RAM, saved as it's written.
    fn keep_saved(&mut self, _closing: bool) {
        let Some(path) = &self.nvram_file else { return };
        if std::mem::take(&mut self.i2c.rtc.ram_written) {
            files::save_later(path, self.i2c.rtc.ram.clone());
        }
    }

    fn leave_out_picture(&mut self, leave_out: bool) -> bool {
        self.vera.leave_out = leave_out;
        leave_out
    }

    /// Its Reset button, held; let go, it's reset, and on if it was off.
    fn reset_button(&mut self, held: bool) {
        if self.reset_held && !held {
            self.powered_off = false;
            self.reset_devices();
        }
        self.reset_held = held;
    }

    fn identity(&self) -> String {
        format!("Commander X16, ROM {:08X}", self.rom_crc)
    }

    fn keep(&mut self, running: &mut X16) {
        use std::mem::take;
        self.rom = take(&mut running.rom);
        self.vera.sd_card().image = running.vera.sd_card().image.take();
        self.typing = take(&mut running.typing);
        self.gamepads = running.gamepads;
        self.mouse = running.mouse;
        self.nvram_file = take(&mut running.nvram_file);
        self.vera.picture = take(&mut running.vera.picture);
        self.name = take(&mut running.name);
        self.rom_crc = running.rom_crc;
        self.samples = take(&mut running.samples);
    }
}

/// Its ROM's file, as it's looked for in ROM_FOLDER: rom.bin there, or the
/// last of its folders' by name that has one.
pub fn rom_file() -> Option<PathBuf> {
    let folder = Path::new(ROM_FOLDER);
    let rom = folder.join("rom.bin");
    if rom.exists() {
        return Some(rom);
    }
    let mut releases: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.path().join("rom.bin")))
        .filter(|rom| rom.exists())
        .collect();
    releases.sort();
    releases.pop()
}

/// Its SD card's image, as it's looked for: roms/x16/sdcard.img, if it's
/// there.
pub fn sd_card_file() -> Option<PathBuf> {
    let image = Path::new(ROM_FOLDER).join("sdcard.img");
    image.exists().then_some(image)
}

/// The X16 with the ROM at `rom`, turned on, its SD card the image at
/// `sd_card`, if there's one.
pub fn build(rom: &str, sd_card: Option<&str>) -> Result<CPU<X16>, String> {
    let path = Path::new(rom);
    let bytes = files::read(path).map_err(|e| format!("{rom}: {e}"))?;
    let mut x16 = X16::new(&bytes).map_err(|e| format!("{rom}: {e}"))?;
    if let Some(image) = sd_card {
        x16.vera.sd_card().image = Some(sd_card::Image::open(Path::new(image))?);
    }
    x16.name = path
        .parent()
        .and_then(Path::file_name)
        .map_or(String::new(), |name| name.to_string_lossy().into_owned());
    let mut cpu = CPU::with_bus(x16);
    cpu.variant = Variant::Wdc65C02;
    cpu.clock_hz = CLOCK_HZ;
    cpu.reset();
    Ok(cpu)
}

/// How big its picture's shown: half as big again, 960 by 720.
pub fn size() -> (u32, u32) {
    ((WIDTH * 3 / 2) as u32, (HEIGHT * 3 / 2) as u32)
}

/// The X16 as `build` has it, run in a window, `fullscreen` or not; its
/// state saved beside its SD card's image, or its ROM, with none.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(rom: &str, sd_card: Option<&str>, fullscreen: bool) {
    let mut cpu = build(rom, sd_card).unwrap_or_else(|e| panic!("{e}"));
    cpu.memory.keep_nvram(Path::new(NVRAM_FILE));
    let title = cpu.memory.title();
    let window = Window::new(&title, WIDTH, HEIGHT, size(), fullscreen);
    let state = save_state::path(Path::new(sd_card.unwrap_or(rom)));
    Host::new(cpu, Box::new(window), state).run();
}
