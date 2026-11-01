//! The NES: its CPU, a Ricoh 2A03, a 6502 without decimal mode, at 1.79
//! MHz (the 21.477 MHz crystal's twelfth); its 2K of RAM; its PPU, a dot
//! three times a CPU cycle; its two controllers; and the cartridge, from
//! an iNES image, or a zip with one in it.
//!
//! The CPU's memory map: the RAM, $0000 to $07FF, and again to $1FFF; the
//! PPU's 8 registers, $2000 to $2007, and again to $3FFF; the APU's and
//! the I/O's, $4000 to $4017: OAM DMA ($4014), the controllers ($4016 and
//! $4017); and the cartridge's, $4020 up. A read of what nothing answers
//! is open bus: the last byte on the bus. The PPU's vertical blank pulls
//! the CPU's NMI.
//!
//! OAM DMA, a write of a page's number to $4014: the 256 bytes of that
//! page copied to OAMDATA, the CPU halted on its next read meanwhile, a
//! cycle to halt it, a byte read on each "get" cycle and written on the
//! "put" after, a cycle more first if the first is a put: 513 or 514.
//!
//! A controller, its 8 buttons latched while $4016's bit 0 is written 1,
//! read a bit at a time (bit 0 of $4016 or $4017) once it's 0: A, B,
//! Select, Start, Up, Down, Left, Right; then 1s. The first is the
//! window's gamepad; the second, none.
//!
//! F2 saves its state, F4 loads it back, beside its image, its name with
//! .state after; F12 is its Reset button, held.

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
use crate::host::display::{Display, Gamepad};
use crate::host::window::Window;
use crate::machines::save_state;
use cartridge::Cartridge;
use ppu::Ppu;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The NTSC NES's: the crystal's 21,477,272 Hz, a twelfth.
pub const CLOCK_HZ: u32 = 1_789_773;

/// A save state to be saved, or loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wanted {
    Save,
    Load,
}

/// OAM DMA under way: the page; whether it has the CPU halted yet; the
/// byte read, to be written; and how many have been.
#[derive(Serialize, Deserialize)]
struct Dma {
    page: u8,
    halted: bool,
    value: Option<u8>,
    done: u16,
}

/// In a save state, all but its ROMs and the host's side, which the
/// machine it's loaded into has.
#[derive(Serialize, Deserialize)]
pub struct Nes {
    pub ram: Vec<u8>,
    pub ppu: Ppu,
    pub cartridge: Cartridge,
    /// The controllers' shift registers, and whether they're latching.
    shifts: [u8; 2],
    strobe: bool,
    /// The last byte on the CPU's bus.
    open_bus: u8,
    dma: Option<Dma>,
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
    /// The PPU's colours, and the frame made of them.
    #[serde(skip)]
    colours: Vec<u32>,
    #[serde(skip)]
    picture: Vec<u32>,
}

impl Nes {
    pub fn new(cartridge: Cartridge) -> Self {
        Nes {
            ram: vec![0; 0x800],
            ppu: Ppu::new(),
            cartridge,
            shifts: [0; 2],
            strobe: false,
            open_bus: 0,
            dma: None,
            nmi_line: false,
            irq_line: false,
            cycles: 0,
            frames: 0,
            reset_held: false,
            state_wanted: None,
            display: None,
            gamepad: Gamepad::default(),
            colours: palette::palette(),
            picture: vec![0; ppu::WIDTH * ppu::HEIGHT],
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
            0x4016 | 0x4017 => self.controller(addr as usize - 0x4016) | (self.open_bus & 0xE0),
            0x4000..=0x401F => self.open_bus,
            _ => self.cartridge.cpu_read(addr).unwrap_or(self.open_bus),
        }
    }

    /// A cycle of OAM DMA, the CPU halted: a byte read on a get cycle, the
    /// even ones; written on the put after.
    fn dma_cycle(&mut self) {
        let Some(dma) = &self.dma else { return };
        let get = self.cycles % 2 == 0;
        match dma.value {
            None if get => {
                let addr = (dma.page as u16) << 8 | dma.done;
                let value = self.cpu_read(addr);
                self.open_bus = value;
                self.dma.as_mut().unwrap().value = Some(value);
            }
            Some(value) if !get => {
                self.ppu.dma_write(value);
                let dma = self.dma.as_mut().unwrap();
                dma.value = None;
                dma.done += 1;
                if dma.done == 256 {
                    self.dma = None;
                }
            }
            _ => {} // a cycle to line up with the gets and puts
        }
    }

    /// A frame done: shown; the function keys taken, and the Reset button,
    /// the PPU reset while it's held.
    fn frame(&mut self) {
        self.frames += 1;
        if let Some(display) = &mut self.display {
            for (dot, &colour) in self.picture.iter_mut().zip(&self.ppu.frame) {
                *dot = self.colours[colour as usize];
            }
            display.show(&self.picture, ppu::WIDTH, ppu::HEIGHT);
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
        if self.reset_held {
            self.ppu.reset();
        }
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
        self.picture = std::mem::take(&mut running.picture);
        self.ppu.frame = std::mem::take(&mut running.ppu.frame);
    }
}

impl Bus for Nes {
    fn read(&mut self, addr: u16) -> u8 {
        let value = self.cpu_read(addr);
        self.open_bus = value;
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.open_bus = value;
        match addr {
            0x0000..=0x1FFF => self.ram[addr as usize & 0x7FF] = value,
            0x2000..=0x3FFF => self.ppu.write_register(addr, value, &mut self.cartridge),
            0x4014 => self.dma = Some(Dma { page: value, halted: false, value: None, done: 0 }),
            0x4016 => {
                self.strobe = value & 1 != 0;
                if self.strobe {
                    self.shifts = [self.buttons(), 0];
                }
            }
            0x4000..=0x401F => {}
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
        self.irq_line = self.cartridge.irq();
        self.ppu.tick(&mut self.cartridge);
        self.ppu.tick(&mut self.cartridge);
        if self.dma.as_ref().is_some_and(|dma| dma.halted) {
            self.dma_cycle();
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

    fn halt(&mut self) -> bool {
        match &mut self.dma {
            Some(dma) => {
                dma.halted = true;
                true
            }
            None => false,
        }
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

/// The NES run, with the game at `rom` (an iNES image, or a zip of one),
/// in a window, `fullscreen` or not.
pub fn run(rom: &str, fullscreen: bool) {
    let path = Path::new(rom);
    let image = image(path).unwrap_or_else(|e| panic!("{rom}: {e}"));
    let cartridge = Cartridge::new(&image).unwrap_or_else(|e| panic!("{rom}: {e}"));
    let mut nes = Nes::new(cartridge);
    let name = path.file_stem().map_or(String::new(), |name| name.to_string_lossy().into_owned());
    // Its 256 dots by 240 lines, each dot 8:7 as a TV shows it: 878 by 720.
    nes.display = Some(Box::new(Window::new(&format!("NES   {name}"), ppu::WIDTH, ppu::HEIGHT, (878, 720), fullscreen)));
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
