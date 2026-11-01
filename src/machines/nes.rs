//! The NES: its CPU, a Ricoh 2A03, a 6502 without decimal mode, at 1.79
//! MHz (the 21.477 MHz crystal's twelfth); its 2K of RAM; its PPU, a dot
//! three times a CPU cycle; its two controllers; and the cartridge, from
//! an iNES image, or a zip with one in it.
//!
//! The CPU's memory map: the RAM, $0000 to $07FF, and again to $1FFF; the
//! PPU's 8 registers, $2000 to $2007, and again to $3FFF; the APU's and
//! the I/O's, $4000 to $4017: OAM DMA ($4014), the controllers ($4016 and
//! $4017); and the cartridge's, $4020 up. A read of what nothing answers
//! is open bus: the last byte on the bus ($4015's read is the APU's own,
//! not on it). The PPU's vertical blank pulls the CPU's NMI; the APU's
//! frame counter and DMC, and the cartridge, its IRQ.
//!
//! OAM DMA, a write of a page's number to $4014: the 256 bytes of that
//! page copied to OAMDATA, the CPU halted on its next read meanwhile, a
//! cycle to halt it, a byte read on each "get" cycle and written on the
//! "put" after, a cycle more first if the first is a put: 513 or 514.
//! The DMC's DMA, a byte of its sample: a cycle halting the CPU, a dummy
//! cycle, a cycle more to a get if need be, and the get. During OAM DMA,
//! its cycles go on beside OAM DMA's, but for its get, which OAM DMA waits
//! for, a cycle more to line up again: 2 more cycles; 1 or 3 at OAM DMA's
//! end. On a halted cycle neither does anything with, the CPU's read is
//! made again.
//!
//! Its sound, the APU's, sampled at the host's rate, through the filters
//! the NES's own output has: high-passes at 90 Hz and 440 Hz, a low-pass
//! at 14 kHz.
//!
//! A controller, its 8 buttons latched while $4016's bit 0 is written 1,
//! read a bit at a time (bit 0 of $4016 or $4017) once it's 0: A, B,
//! Select, Start, Up, Down, Left, Right; then 1s. The first is the
//! window's gamepad; the second, none.
//!
//! Its picture is shown as a TV shows it, its edges hidden behind the
//! screen's bezel, 8 dots or lines each by default: where games leave
//! what's not to be seen, sprites put out of the way and tiles being
//! scrolled in.
//!
//! F2 saves its state, F4 loads it back, beside its image, its name with
//! .state after; F12 is its Reset button, held.

pub mod apu;
pub mod cartridge;
pub mod palette;
pub mod ppu;

#[cfg(test)]
mod tests {
    mod cartridge;
    mod test_roms;
}

use crate::cpu::{Variant, CPU};
use crate::cpu::bus::Bus;
use crate::host::display::{Display, Gamepad, SAMPLE_RATE};
use crate::host::window::Window;
use crate::machines::save_state;
use apu::Apu;
use cartridge::Cartridge;
use ppu::Ppu;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The NTSC NES's: the crystal's 21,477,272 Hz, a twelfth.
pub const CLOCK_HZ: u32 = 1_789_773;

/// How loud it is, of the host's full volume.
const VOLUME: f32 = 0.6;

/// The output's filters' corners: its two high-passes', its low-pass's.
const FILTERS: [f32; 3] = [90.0, 440.0, 14_000.0];

/// A save state to be saved, or loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wanted {
    Save,
    Load,
}

/// The DMC's DMA, a step at a time: its halt, its dummy cycle, then its
/// get (on a get cycle, a cycle to line up first if need be).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum DmcStep {
    Halt,
    Dummy,
    Get,
}

/// OAM DMA under way: the page; the byte read, to be written; and how
/// many have been.
#[derive(Serialize, Deserialize)]
struct Dma {
    page: u8,
    value: Option<u8>,
    done: u16,
}

/// In a save state, all but its ROMs and the host's side, which the
/// machine it's loaded into has.
#[derive(Serialize, Deserialize)]
pub struct Nes {
    pub ram: Vec<u8>,
    pub ppu: Ppu,
    pub apu: Apu,
    pub cartridge: Cartridge,
    /// The controllers' shift registers, and whether they're latching.
    shifts: [u8; 2],
    strobe: bool,
    /// The last byte on the CPU's bus.
    open_bus: u8,
    /// OAM DMA; the DMC's; whether the CPU's halted for them, and the
    /// address of the read it's halted on.
    dma: Option<Dma>,
    dmc_dma: Option<DmcStep>,
    halted: bool,
    halted_on: u16,
    /// The NMI and IRQ lines at the end of the last cycle, for the CPU.
    nmi_line: bool,
    irq_line: bool,
    /// CPU cycles since power-on; and frames.
    pub cycles: u64,
    pub frames: u64,
    pub reset_held: bool,
    /// A save state wanted, by F2 or F4, the CPU's run loop handed back
    /// for it.
    #[serde(skip)]
    pub state_wanted: Option<Wanted>,
    /// The window the picture is shown in, and the gamepad comes from; the
    /// gamepad of the tests', without one.
    #[serde(skip)]
    pub display: Option<Box<dyn Display>>,
    #[serde(skip)]
    pub gamepad: Gamepad,
    /// The PPU's colours; the dots or lines hidden at each edge, as a TV's
    /// overscan hides them; and the frame made of them, what's seen of it.
    #[serde(skip)]
    colours: Vec<u32>,
    #[serde(skip)]
    pub overscan: usize,
    #[serde(skip)]
    picture: Vec<u32>,
    /// The sound: the output summed over the sample being made, and its
    /// cycles; how far into it, in parts of CLOCK_HZ; the filters' last
    /// input and output each; the samples made this frame.
    #[serde(skip)]
    sample_sum: f32,
    #[serde(skip)]
    sample_cycles: u32,
    #[serde(skip)]
    sample_phase: u32,
    #[serde(skip)]
    filters: [(f32, f32); 3],
    #[serde(skip)]
    sound: Vec<f32>,
}

impl Nes {
    pub fn new(cartridge: Cartridge) -> Self {
        Nes {
            ram: vec![0; 0x800],
            ppu: Ppu::new(),
            apu: Apu::new(),
            cartridge,
            shifts: [0; 2],
            strobe: false,
            open_bus: 0,
            dma: None,
            dmc_dma: None,
            halted: false,
            halted_on: 0,
            nmi_line: false,
            irq_line: false,
            cycles: 0,
            frames: 0,
            reset_held: false,
            state_wanted: None,
            display: None,
            gamepad: Gamepad::default(),
            colours: palette::palette(),
            overscan: 0,
            picture: Vec::new(),
            sample_sum: 0.0,
            sample_cycles: 0,
            sample_phase: 0,
            filters: [(0.0, 0.0); 3],
            sound: Vec::new(),
        }
    }

    /// The first controller's buttons, a bit each: A, B, Select, Start, Up,
    /// Down, Left, Right. A D-pad's opposites can't both be pressed.
    fn buttons(&self) -> u8 {
        let pad = self.display.as_ref().map_or(self.gamepad, |display| display.gamepad());
        let (up, down) = (pad.up && !pad.down, pad.down && !pad.up);
        let (left, right) = (pad.left && !pad.right, pad.right && !pad.left);
        [pad.a, pad.b, pad.select, pad.start, up, down, left, right]
            .iter()
            .enumerate()
            .fold(0, |buttons, (bit, &held)| buttons | (held as u8) << bit)
    }

    /// Controller `n`'s next bit; while latching, A's, over again.
    fn controller(&mut self, n: usize) -> u8 {
        if self.strobe {
            self.shifts = [self.buttons(), 0];
        }
        let bit = self.shifts[n] & 1;
        if !self.strobe {
            self.shifts[n] = self.shifts[n] >> 1 | 0x80;
        }
        bit
    }

    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram[addr as usize & 0x7FF],
            0x2000..=0x3FFF => self.ppu.read_register(addr, &mut self.cartridge),
            0x4015 => self.apu.status(self.open_bus),
            0x4016 | 0x4017 => self.controller(addr as usize - 0x4016) | (self.open_bus & 0xE0),
            0x4000..=0x401F => self.open_bus,
            _ => self.cartridge.cpu_read(addr).unwrap_or(self.open_bus),
        }
    }

    /// A cycle of DMA, the CPU halted, the odd cycles gets: the DMC's step,
    /// its byte read on its get; OAM DMA's, a byte read on a get and
    /// written on the put after, unless the DMC's get has the cycle. If
    /// neither does anything, the CPU's read made again.
    fn dma_cycle(&mut self) {
        let get = self.cycles % 2 == 1;
        let mut busy = false;
        match self.dmc_dma {
            Some(DmcStep::Halt) => self.dmc_dma = Some(DmcStep::Dummy),
            Some(DmcStep::Dummy) => self.dmc_dma = Some(DmcStep::Get),
            Some(DmcStep::Get) if get => {
                if let Some(addr) = self.apu.dmc_wants() {
                    let byte = self.cpu_read(addr);
                    self.open_bus = byte;
                    self.apu.dmc_fill(byte);
                }
                self.dmc_dma = None;
                busy = true;
            }
            _ => {}
        }
        if let Some(dma) = self.dma.as_ref().filter(|_| !busy) {
            match dma.value {
                None if get => {
                    let addr = (dma.page as u16) << 8 | dma.done;
                    let value = self.cpu_read(addr);
                    self.open_bus = value;
                    self.dma.as_mut().unwrap().value = Some(value);
                    busy = true;
                }
                Some(value) if !get => {
                    self.ppu.dma_write(value);
                    let dma = self.dma.as_mut().unwrap();
                    dma.value = None;
                    dma.done += 1;
                    if dma.done == 256 {
                        self.dma = None;
                    }
                    busy = true;
                }
                _ => {} // a cycle to line up with the gets and puts
            }
        }
        if !busy {
            let value = self.cpu_read(self.halted_on);
            self.open_bus = value;
        }
        self.halted = self.dma.is_some() || self.dmc_dma.is_some();
    }

    /// A frame done: shown; the function keys taken, and the Reset button,
    /// the PPU reset while it's held.
    fn frame(&mut self) {
        self.frames += 1;
        if let Some(display) = &mut self.display {
            let (width, height) = seen(self.overscan);
            self.picture.clear();
            for line in self.ppu.frame.chunks(ppu::WIDTH).skip(self.overscan).take(height) {
                let dots = &line[self.overscan..self.overscan + width];
                self.picture.extend(dots.iter().map(|&colour| self.colours[colour as usize]));
            }
            display.show(&self.picture, width, height);
            while let Some(key) = display.function_key() {
                match key {
                    2 => self.state_wanted = Some(Wanted::Save),
                    4 => self.state_wanted = Some(Wanted::Load),
                    _ => {}
                }
            }
            self.reset_held = display.reset_held();
            if display.closed() {
                std::process::exit(0);
            }
        }
        if let Some(display) = &mut self.display {
            display.play(&self.sound);
            self.sound.clear();
        }
        if self.reset_held {
            self.ppu.reset();
            (self.dma, self.dmc_dma, self.halted) = (None, None, false);
        }
    }

    /// A cycle of sound: the output summed; a sample made when one is due,
    /// the sum's average through the filters, in both channels.
    fn sound_cycle(&mut self) {
        self.sample_sum += self.apu.output();
        self.sample_cycles += 1;
        self.sample_phase += SAMPLE_RATE;
        if self.sample_phase < CLOCK_HZ {
            return;
        }
        self.sample_phase -= CLOCK_HZ;
        let mut sample = self.sample_sum / self.sample_cycles as f32;
        (self.sample_sum, self.sample_cycles) = (0.0, 0);
        let dt = 1.0 / SAMPLE_RATE as f32;
        for (n, (last_in, last_out)) in self.filters.iter_mut().enumerate() {
            let rc = 1.0 / (2.0 * std::f32::consts::PI * FILTERS[n]);
            let out = if n < 2 {
                rc / (rc + dt) * (*last_out + sample - *last_in)
            } else {
                *last_out + dt / (rc + dt) * (sample - *last_out)
            };
            (*last_in, *last_out) = (sample, out);
            sample = out;
        }
        self.sound.extend([sample * VOLUME; 2]);
    }

    /// The machine, as a state's header names it: the NES, its game (a
    /// checksum of its ROMs).
    fn machine(&self) -> String {
        let roms = [&self.cartridge.prg_rom[..], &self.cartridge.chr_rom[..]].concat();
        format!("NES, ROMs {:08X}", crate::host::zip::crc32(&roms))
    }

    /// What a save state leaves out, taken from `running`, the machine
    /// it's loaded into: its ROMs and the host's side.
    fn keep(&mut self, running: &mut Nes) {
        self.cartridge.take_roms(&mut running.cartridge);
        self.display = running.display.take();
        self.gamepad = running.gamepad;
        self.colours = std::mem::take(&mut running.colours);
        self.overscan = running.overscan;
        self.picture = std::mem::take(&mut running.picture);
        self.ppu.frame = std::mem::take(&mut running.ppu.frame);
    }
}

impl Bus for Nes {
    fn read(&mut self, addr: u16) -> u8 {
        let value = self.cpu_read(addr);
        if addr != 0x4015 {
            self.open_bus = value;
        }
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.open_bus = value;
        match addr {
            0x0000..=0x1FFF => self.ram[addr as usize & 0x7FF] = value,
            0x2000..=0x3FFF => self.ppu.write_register(addr, value, &mut self.cartridge),
            0x4014 => self.dma = Some(Dma { page: value, value: None, done: 0 }),
            0x4016 => {
                self.strobe = value & 1 != 0;
                if self.strobe {
                    self.shifts = [self.buttons(), 0];
                }
            }
            0x4000..=0x4017 => self.apu.write(addr, value, self.cycles % 2 == 1),
            0x4018..=0x401F => {}
            _ => self.cartridge.cpu_write(addr, value),
        }
    }

    /// A cycle's three dots: the first the last cycle's, after its access,
    /// at whose end the CPU samples NMI and IRQ; then this one's two,
    /// before its access.
    fn tick(&mut self) {
        self.cycles += 1;
        self.ppu.tick(&mut self.cartridge);
        self.nmi_line = self.ppu.nmi();
        self.irq_line = self.cartridge.irq() || self.apu.irq();
        if self.reset_held {
            self.apu.reset();
        }
        self.apu.cycle();
        self.ppu.tick(&mut self.cartridge);
        self.ppu.tick(&mut self.cartridge);
        if self.halted {
            self.dma_cycle();
        }
        if self.dmc_dma.is_none() && self.apu.dmc_wants().is_some() {
            self.dmc_dma = Some(DmcStep::Halt);
        }
        if self.display.is_some() {
            self.sound_cycle();
        }
        if self.ppu.frame_ready {
            self.ppu.frame_ready = false;
            self.frame();
        }
    }

    fn reset_held(&self) -> bool {
        self.reset_held
    }

    fn irq(&self) -> bool {
        self.irq_line
    }

    fn nmi(&self) -> bool {
        self.nmi_line
    }

    /// Halted on a read of `addr`: its cycle the DMC's halt, if it's
    /// waiting for one, the CPU not halted already.
    fn halt(&mut self, addr: u16) -> bool {
        let wanted = self.dma.is_some() || self.dmc_dma.is_some();
        if wanted && !self.halted {
            self.halted_on = addr;
            if self.dmc_dma == Some(DmcStep::Halt) {
                self.dmc_dma = Some(DmcStep::Dummy);
            }
        }
        self.halted = wanted;
        wanted
    }

    fn hand_back(&self) -> bool {
        self.state_wanted.is_some()
    }
}

/// The iNES image at `path`, or in the zip there, the first in it.
pub fn image(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let zipped = path.extension().is_some_and(|kind| kind.eq_ignore_ascii_case("zip"));
    if !zipped {
        return Ok(bytes);
    }
    let (_, image) = crate::host::zip::file(&bytes, |name| name.to_lowercase().ends_with(".nes"))?;
    Ok(image)
}

/// The state wanted, if one is, saved to `path` or loaded from it.
fn wanted(cpu: &mut CPU<Nes>, path: &Path) {
    match cpu.memory.state_wanted.take() {
        None => {}
        Some(Wanted::Save) => {
            let saving = path.with_extension("state.saving");
            let saved = save_state::save(cpu, &cpu.memory.machine()).and_then(|state| {
                std::fs::write(&saving, state).and_then(|_| std::fs::rename(&saving, path)).map_err(|e| e.to_string())
            });
            if let Err(e) = saved {
                eprintln!("{}: not saved: {e}", path.display());
            }
        }
        Some(Wanted::Load) => {
            let loaded = std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|state| save_state::load::<Nes>(&state, &cpu.memory.machine()));
            match loaded {
                Ok(mut loaded) => {
                    loaded.memory.keep(&mut cpu.memory);
                    *cpu = loaded;
                }
                Err(e) => eprintln!("{}: not loaded: {e}", path.display()),
            }
        }
    }
}

/// What's seen of the picture, `overscan` hidden at each edge: its width
/// and height.
fn seen(overscan: usize) -> (usize, usize) {
    (ppu::WIDTH - 2 * overscan, ppu::HEIGHT - 2 * overscan)
}

/// The NES run, with the game at `rom` (an iNES image, or a zip of one),
/// in a window, `fullscreen` or not, `overscan` dots or lines hidden at
/// each edge of its picture.
pub fn run(rom: &str, fullscreen: bool, overscan: usize) {
    let path = Path::new(rom);
    let image = image(path).unwrap_or_else(|e| panic!("{rom}: {e}"));
    let cartridge = Cartridge::new(&image).unwrap_or_else(|e| panic!("{rom}: {e}"));
    let mut nes = Nes::new(cartridge);
    let name = path.file_stem().map_or(String::new(), |name| name.to_string_lossy().into_owned());
    // Its dots and lines seen, each dot 8:7 as a TV shows it, 3 times over:
    // 823 by 672, of 240 by 224.
    nes.overscan = overscan;
    let (width, height) = seen(overscan);
    let size = ((width as f32 * 8.0 / 7.0 * 3.0).round() as u32, height as u32 * 3);
    nes.display = Some(Box::new(Window::new(&format!("NES   {name}"), width, height, size, fullscreen)));
    let state: PathBuf = path.with_file_name(format!("{}.state", path.file_name().unwrap().to_string_lossy()));

    let mut cpu = CPU::with_bus(nes);
    cpu.variant = Variant::Ricoh2A03;
    cpu.clock_hz = CLOCK_HZ;
    cpu.reset();
    loop {
        cpu.execute_forever();
        wanted(&mut cpu, &state);
    }
}
