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
//! made again. A start or stop of the DMC's by a write of $4015 takes 3 or
//! 4 cycles to arrive, and a sample's end as long: a DMA caught by one gets
//! no further than its halt, or if the CPU's writing, doesn't happen. DMA
//! reads are on the external bus alone, and answered by the APU's registers
//! only if the CPU was reading them when it was halted; see `dma_read`.
//!
//! Its sound, the APU's, through the filters the NES's own output has: a
//! low-pass at 14 kHz, on the signal as it is, a cycle at a time, with
//! two more at 20 kHz so that nothing above half the host's rate folds
//! back in to be heard when it's sampled (a triangle's ultrasonic note,
//! as games silence it); then sampled at the host's rate, and high-passed
//! at 90 Hz. Not at 440 Hz too, as the console's output is said to be: it
//! takes the bass out of the triangle, and sounds bad.
//!
//! A controller, its 8 buttons latched while $4016's bit 0 is written 1,
//! as a put cycle begins, so that a 1 written on a put and gone by the get
//! after is never seen; read a bit at a time (bit 0 of $4016 or $4017) once
//! it's 0: A, B,
//! Select, Start, Up, Down, Left, Right; then 1s. The first is the
//! window's first gamepad and its keys; the second, its second gamepad.
//!
//! Its picture is shown as a TV shows it, its edges hidden behind the
//! screen's bezel, 8 dots or lines each by default: where games leave
//! what's not to be seen, sprites put out of the way and tiles being
//! scrolled in.
//!
//! A cartridge's battery-backed RAM, a game's saves, is kept beside its
//! image, its name with .srm for its extension (as RetroArch keeps it, the
//! RAM as it is): read at power-on, saved a second after the game writes
//! it, and as the window closes.
//!
//! F2 saves its state, F4 loads it back, beside its image, its name with
//! .state after; F12 is its Reset button, held.
//!
//! A Japanese game's console is a Famicom: its PPU's reset wired high, it
//! starts at power-on a frame before the CPU does, so its warm-up's over
//! by the time the game starts (Magic John's and Sansuu 2 Nen's write it
//! before a NES's would be); and its Reset button the CPU's alone. A game
//! is Japanese if the database says it's the Famicom's, or its name says
//! Japan's alone; a Disk System's always is.

pub mod apu;
pub mod cartridge;
mod database;
pub mod fds;
pub mod mmc5;
pub mod palette;
pub mod ppu;
pub mod region;
pub mod vrc;
pub mod zapper;

#[cfg(test)]
mod tests {
    mod cartridge;
    mod controllers;
    mod database;
    mod fds;
    mod mmc5;
    mod sound;
    mod test_roms;
    mod timing;
    mod vrc;
    mod zapper;
}

use crate::cpu::bus::Bus;
use crate::cpu::{CPU, Variant};
use crate::host::display::{Gamepad, SAMPLE_RATE};
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
use apu::Apu;
use cartridge::Cartridge;
use ppu::Ppu;
use region::Region;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zapper::Zapper;

/// The NTSC NES's: the crystal's 21,477,272 Hz, a twelfth.
pub const CLOCK_HZ: u32 = 1_789_773;

/// How loud it is, of the host's full volume.
const VOLUME: f32 = 0.85;
/// A header, and the least a cartridge can hold: 16K and 8K.
#[cfg(not(target_arch = "wasm32"))]
const HEADER_AND_BLANK: usize = 16 + 0x4000 + 0x2000;
/// The Disk System's BIOS: its name, and its size.
const FDS_BIOS: &str = "disksys.rom";
const FDS_BIOS_SIZE: usize = 0x2000;
/// The function key that changes a Disk System's disk.
const CHANGE_DISK_KEY: u8 = 9;

/// The output's filters' corners: its low-passes', a cycle at a time; its
/// high-pass's, a sample at a time.
const LOW_PASSES: [f32; 3] = [14_000.0, 20_000.0, 20_000.0];
const HIGH_PASS: f32 = 90.0;

/// Its sound, when it's made: the low-passes, a cycle at a time; the
/// sampling; the high-pass.
struct Sound {
    low_passes: [LowPass; 3],
    sampler: Sampler<1>,
    high_pass: HighPass,
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

/// In a save state, all but its ROMs and what's the host's (the picture
/// and sound, the controls), which the machine it's loaded into has.
#[derive(Serialize, Deserialize)]
pub struct Nes {
    /// Its 2K of RAM: all $FF at power-on unless it's said otherwise (see
    /// `PowerOnRam`), as Mesen has it as an option. Minna no Taabou,
    /// starting its NMI before its RAM, needs some $FF in it to start.
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
    /// CPU cycles since power-on; frames, and whether one has ended, for
    /// the host.
    pub cycles: u64,
    pub frames: u64,
    #[serde(skip)]
    frame_ended: bool,
    reset_held: bool,
    /// What's on the chip's own data bus, which every read and write of the
    /// CPU's puts there, and DMA's don't; and $4016's bit 0, as last written.
    #[serde(skip)]
    internal_bus: u8,
    #[serde(skip)]
    strobe_written: bool,
    /// Its region, from its game: a state's loaded into one of the same.
    #[serde(skip)]
    pub region: Region,
    /// Whether it's a Famicom, from its game too.
    #[serde(skip)]
    pub famicom: bool,
    /// The game's name; the gamepads, as the host last gave them.
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    pub gamepad: Gamepad,
    #[serde(skip)]
    pub gamepad_2: Gamepad,
    /// The Zapper in the second port, if its game's played with one.
    #[serde(skip)]
    pub zapper: Option<Zapper>,
    /// The PPU's colours; the dots or lines hidden at each edge, as a TV's
    /// overscan hides them; and the frame made of them, what's seen of it.
    #[serde(skip)]
    colours: Vec<u32>,
    #[serde(skip)]
    pub overscan: usize,
    /// Where the cartridge's battery-backed RAM is kept, if it has any.
    #[serde(skip)]
    pub battery_file: Option<PathBuf>,
    #[serde(skip)]
    picture: Vec<u32>,
    /// Its sound, if it's made, and the samples made and not yet taken.
    #[serde(skip)]
    sound: Option<Sound>,
    #[serde(skip)]
    samples: Vec<f32>,
}

impl Nes {
    pub fn new(cartridge: Cartridge) -> Self {
        Nes::of(cartridge, Region::Ntsc)
    }

    /// A Famicom, not a NES: its PPU started a frame before its CPU, so
    /// warmed up already, and its Reset button not the PPU's.
    pub fn as_famicom(&mut self) {
        self.famicom = true;
        self.ppu.warmed_up();
    }

    /// Performance mode, which has to be before its sound's on.
    pub fn performance(&mut self) {
        self.ppu.fast = true;
        self.apu.fast = true;
    }

    /// A NES of `region`.
    pub fn of(cartridge: Cartridge, region: Region) -> Self {
        Nes {
            famicom: false,
            ram: vec![0xFF; 0x800],
            ppu: Ppu::new(region),
            apu: Apu::new(region),
            region,
            zapper: cartridge.zapper.then(Zapper::default),
            cartridge,
            shifts: [0; 2],
            strobe: false,
            strobe_written: false,
            internal_bus: 0,
            open_bus: 0,
            dma: None,
            dmc_dma: None,
            halted: false,
            halted_on: 0,
            nmi_line: false,
            irq_line: false,
            cycles: 0,
            frames: 0,
            frame_ended: false,
            reset_held: false,
            name: String::new(),
            gamepad: Gamepad::default(),
            gamepad_2: Gamepad::default(),
            colours: palette::palette(region),
            overscan: 0,
            battery_file: None,
            picture: Vec::new(),
            sound: None,
            samples: Vec::new(),
        }
    }

    /// A controller's buttons, a bit each: A, B, Select, Start, Up, Down,
    /// Left, Right. A D-pad's opposites can't both be pressed.
    fn buttons(pad: Gamepad) -> u8 {
        let (up, down) = (pad.up && !pad.down, pad.down && !pad.up);
        let (left, right) = (pad.left && !pad.right, pad.right && !pad.left);
        [pad.a, pad.b, pad.select, pad.start, up, down, left, right]
            .iter()
            .enumerate()
            .fold(0, |buttons, (bit, &held)| buttons | (held as u8) << bit)
    }

    /// The controllers latched.
    fn latch(&mut self) {
        self.shifts = [Nes::buttons(self.gamepad), Nes::buttons(self.gamepad_2)];
    }

    /// Controller `n`'s next bit; while latching, A's, over again.
    fn controller(&mut self, n: usize) -> u8 {
        if self.strobe {
            self.latch();
        }
        let bit = self.shifts[n] & 1;
        if !self.strobe {
            self.shifts[n] = self.shifts[n] >> 1 | 0x80;
        }
        bit
    }

    /// $4017 with a Zapper there: its bits; a Famicom's, the second
    /// controller's too, which is still wired in.
    fn zapper_read(&mut self) -> u8 {
        let pad = if self.famicom { self.controller(1) } else { 0 };
        let zapper = self.zapper.unwrap_or_default();
        let ppu = &self.ppu;
        zapper.read(ppu.line, ppu.dot, &ppu.frame, &self.colours) | pad | (self.open_bus & 0xE0)
    }

    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram[addr as usize & 0x7FF],
            0x2000..=0x3FFF => self.ppu.read_register(addr, &mut self.cartridge),
            0x4015 => self.apu.status(self.internal_bus, self.cycles % 2 == 1),
            0x4017 if self.zapper.is_some() => self.zapper_read(),
            0x4016 | 0x4017 => self.controller(addr as usize - 0x4016) | (self.open_bus & 0xE0),
            0x4000..=0x401F => self.open_bus,
            0x4020..=0x7FFF => self.cartridge.register_read(addr, self.open_bus),
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
                let byte = self.dma_read(self.apu.dmc_address());
                self.apu.dmc_fill(byte);
                self.dmc_dma = None;
                busy = true;
            }
            _ => {}
        }
        if let Some(dma) = self.dma.as_ref().filter(|_| !busy) {
            match dma.value {
                None if get => {
                    let addr = (dma.page as u16) << 8 | dma.done;
                    let value = self.dma_read(addr);
                    self.dma.as_mut().unwrap().value = Some(value);
                    busy = true;
                }
                Some(value) if !get => {
                    self.open_bus = value;
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
        // The PAL CPU stops for the DMC only on an instruction's first
        // cycle, so a read of a register isn't made again.
        let register = (0x2000..0x4020).contains(&self.halted_on);
        if !busy && !(self.region == Region::Pal && register && self.dma.is_none()) {
            self.read(self.halted_on);
        }
        self.halted = self.dma.is_some() || self.dmc_dma.is_some();
    }

    /// A byte read by DMA, on the external bus alone. Only its address's low
    /// 5 bits are the DMA's: the rest stay the CPU's, so the APU's registers
    /// answer it, in their mirrors everywhere, if the CPU was reading them,
    /// and not otherwise, even at their own address. $4015's is read inside
    /// the chip, the bus left as it is but for its open bit. A controller's
    /// bits 0 to 4 win on the bus, for the CPU to see, against anything
    /// driving it; that, not they, is what the DMA gets.
    fn dma_read(&mut self, addr: u16) -> u8 {
        let driving = match addr {
            0x4000..=0x401F => None,
            0x4020..=0xFFFF => self.cartridge.cpu_read(addr),
            _ => Some(self.cpu_read(addr)),
        };
        self.open_bus = driving.unwrap_or(self.open_bus);
        if self.halted_on & 0xFFE0 != 0x4000 {
            return self.open_bus;
        }
        match 0x4000 | (addr & 0x1F) {
            0x4015 => self.apu.status(self.open_bus, self.cycles % 2 == 1),
            register @ (0x4016 | 0x4017) => {
                let bits = self.controller(register as usize - 0x4016);
                self.open_bus = self.open_bus & 0xE0 | bits;
                driving.unwrap_or(self.open_bus)
            }
            _ => self.open_bus,
        }
    }

    /// The cartridge's battery-backed RAM saved to its file, if it's been
    /// written.
    fn save_battery(&mut self) {
        let Some(path) = &self.battery_file else {
            return;
        };
        if !self.cartridge.written {
            return;
        }
        self.cartridge.written = false;
        files::save_later(path, self.cartridge.prg_ram.clone());
    }

    /// A cycle of sound: the output through the low-passes; a sample made
    /// when one is due, their average through the high-pass, in both
    /// channels.
    /// How many cycles between looking at the sound's level: each, or, in
    /// performance mode, each 4th, still far quicker than it can be heard.
    fn sound_every(&self) -> u64 {
        if self.ppu.fast { 4 } else { 1 }
    }

    fn sound_cycle(&mut self) {
        if self.cycles & (self.sound_every() - 1) != 0 {
            return;
        }
        let Some(sound) = &mut self.sound else { return };
        let output = sound.low_passes.iter_mut().fold(
            self.apu.output() + self.cartridge.output(),
            |output, low_pass| low_pass.filter(output),
        );
        if let Some([sample]) = sound.sampler.cycle([output]) {
            let sample = sound.high_pass.filter(sample);
            self.samples.extend([sample * VOLUME; 2]);
        }
    }
}

impl Bus for Nes {
    fn read(&mut self, addr: u16) -> u8 {
        let value = self.cpu_read(addr);
        if addr != 0x4015 {
            self.open_bus = value;
        }
        self.internal_bus = value;
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.open_bus = value;
        self.internal_bus = value;
        match addr {
            0x0000..=0x1FFF => self.ram[addr as usize & 0x7FF] = value,
            0x2000..=0x3FFF => {
                self.ppu.write_register(addr, value, &mut self.cartridge);
                self.cartridge.ppu_register_written(addr, value);
            }
            0x4014 => {
                self.cartridge.ppu_register_written(addr, value);
                self.dma = Some(Dma {
                    page: value,
                    value: None,
                    done: 0,
                })
            }
            0x4016 => self.strobe_written = value & 1 != 0,
            0x4000..=0x4017 => self.apu.write(addr, value, self.cycles % 2 == 1),
            0x4018..=0x401F => {}
            _ => self.cartridge.cpu_write(addr, value, self.cycles),
        }
    }

    /// A cycle's dots, three (PAL's now and then four): the first the last
    /// cycle's, after its access, at whose end the CPU samples NMI and IRQ;
    /// then this one's, before its access.
    fn tick(&mut self) {
        self.cycles += 1;
        let dots = self.region.dots(self.cycles);
        let passed = self.ppu.passed(dots as u16);
        if !passed {
            self.ppu.tick(&mut self.cartridge);
        }
        self.nmi_line = self.ppu.nmi();
        if self.ppu.fetching_unseen() {
            self.cartridge.fetching();
        }
        self.cartridge.cycle();
        self.irq_line = self.cartridge.irq() || self.apu.irq();
        if self.reset_held {
            self.apu.reset();
        }
        self.apu.cycle();
        if !passed {
            for _ in 1..dots {
                self.ppu.tick(&mut self.cartridge);
            }
        }
        // A write of $4016 only reaches the controllers as a put begins.
        if self.cycles % 2 == 0 {
            self.strobe = self.strobe_written;
            if self.strobe {
                self.latch();
            }
        }
        if self.dmc_dma == Some(DmcStep::Halt) && !self.apu.dmc_wants() {
            self.dmc_dma = None; // stopped before it could halt the CPU
        }
        if self.halted {
            self.dma_cycle();
        }
        if self.dmc_dma.is_none() && self.apu.dmc_wants() {
            self.dmc_dma = Some(DmcStep::Halt);
        }
        self.sound_cycle();
        if self.ppu.frame_ready {
            self.ppu.frame_ready = false;
            self.frames += 1;
            self.frame_ended = true;
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
    /// waiting for one, the CPU not halted already; and all of its DMA,
    /// if the DMC stops on the next cycle.
    fn halt(&mut self, addr: u16) -> bool {
        let wanted = self.dma.is_some() || self.dmc_dma.is_some();
        if wanted && !self.halted {
            self.halted_on = addr;
            if self.dmc_dma == Some(DmcStep::Halt) {
                let aborted = self.apu.dmc_stopping();
                self.dmc_dma = (!aborted).then_some(DmcStep::Dummy);
            }
        }
        self.halted = self.dma.is_some() || self.dmc_dma.is_some();
        wanted
    }

    fn hand_back(&mut self) -> bool {
        std::mem::take(&mut self.frame_ended)
    }
}

impl Machine for Nes {
    /// Its frame in colour, what's seen of it.
    fn picture(&mut self) -> Picture<'_> {
        let (width, height) = seen(self.overscan);
        self.picture.clear();
        let lines = self.ppu.frame.chunks(ppu::WIDTH).skip(self.overscan);
        for line in lines.take(height) {
            let dots = &line[self.overscan..self.overscan + width];
            self.picture
                .extend(dots.iter().map(|&colour| self.colours[colour as usize]));
        }
        Picture {
            pixels: &self.picture,
            width,
            height,
        }
    }

    fn debug(&self) -> Vec<String> {
        let region = match self.region {
            Region::Ntsc => "NTSC",
            Region::Pal => "PAL",
        };
        let console = if self.famicom { "FAMICOM" } else { "NES" };
        let mut lines = vec![format!("{console} {region} FRAME {}", self.frames)];
        lines.extend(self.cartridge.debug());
        lines.push(format!("APU {}", self.apu.sounding()));
        lines
    }

    fn title(&self) -> String {
        let console = if self.famicom { "Famicom" } else { "NES" };
        match self.region {
            Region::Ntsc => format!("{console}   {}", self.name),
            Region::Pal => format!("{console}   {}   PAL, 50 Hz", self.name),
        }
    }

    fn menu(&self) -> Vec<(&'static str, u8)> {
        if matches!(self.cartridge.mapper, cartridge::Mapper::Fds(_)) {
            vec![("CHANGE DISK SIDE", CHANGE_DISK_KEY)]
        } else {
            Vec::new()
        }
    }

    /// F9 changes a Disk System's disk.
    fn function_key(&mut self, key: u8) -> Option<String> {
        if key == CHANGE_DISK_KEY {
            self.cartridge.change_disk()
        } else {
            None
        }
    }

    fn sound_on(&mut self) {
        let clock_hz = self.region.clock_hz() / self.sound_every() as u32;
        self.sound = Some(Sound {
            low_passes: LOW_PASSES.map(|corner| LowPass::new(corner, clock_hz)),
            sampler: Sampler::new(clock_hz),
            high_pass: HighPass::new(HIGH_PASS, SAMPLE_RATE),
        });
    }

    fn sound(&mut self) -> &mut Vec<f32> {
        &mut self.samples
    }

    fn leave_out_picture(&mut self, leave_out: bool) -> bool {
        // Not a Zapper's: what's drawn is what it sees.
        self.ppu.leaving_out = leave_out && self.ppu.fast && self.zapper.is_none();
        self.ppu.leaving_out
    }

    fn input(&mut self, input: Input) {
        self.gamepad = input.gamepad;
        self.gamepad_2 = input.gamepad_2;
        if let Some(zapper) = &mut self.zapper {
            zapper.aim(input.pointer, self.overscan);
        }
    }

    fn aimed_at(&self) -> bool {
        self.zapper.is_some()
    }

    /// Held, the PPU's reset, but for a Famicom's, and the CPU and APU
    /// held so.
    fn reset_button(&mut self, held: bool) {
        self.reset_held = held;
        if held {
            if !self.famicom {
                self.ppu.reset();
            }
            self.cartridge.reset();
            (self.dma, self.dmc_dma, self.halted) = (None, None, false);
        }
    }

    /// Its battery's RAM, a second after it's written.
    fn keep_saved(&mut self, closing: bool) {
        if closing || self.frames % 60 == 0 {
            self.save_battery();
        }
    }

    /// The NES, and a checksum of its game's ROMs: what a state of it
    /// leaves out.
    fn identity(&self) -> String {
        let roms = [&self.cartridge.prg_rom[..], &self.cartridge.chr_rom[..]].concat();
        let region = if self.region == Region::Pal {
            ", PAL"
        } else {
            ""
        };
        let disk = self
            .cartridge
            .disk_crc()
            .map_or(String::new(), |crc| format!(", disk {crc:08X}"));
        format!("NES, ROMs {:08X}{disk}{region}", crc32(&roms))
    }

    fn keep(&mut self, running: &mut Nes) {
        use std::mem::take;
        self.cartridge.take_roms(&mut running.cartridge);
        self.region = running.region;
        (self.ppu.region, self.apu.region) = (running.region, running.region);
        self.famicom = running.famicom;
        self.name = take(&mut running.name);
        self.gamepad = running.gamepad;
        self.gamepad_2 = running.gamepad_2;
        self.zapper = running.zapper;
        self.colours = take(&mut running.colours);
        self.overscan = running.overscan;
        self.battery_file = running.battery_file.take();
        self.picture = take(&mut running.picture);
        self.ppu.frame = take(&mut running.ppu.frame);
        self.ppu.fast = running.ppu.fast;
        self.apu.fast = running.apu.fast;
        self.sound = running.sound.take();
        self.samples = take(&mut running.samples);
    }
}

/// The iNES image at `path`, or in the zip there, the first in it.
pub fn image(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = files::read(path).map_err(|e| e.to_string())?;
    let zipped = path
        .extension()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("zip"));
    if !zipped {
        return Ok(bytes);
    }
    let is_game = |name: &str| {
        let name = name.to_lowercase();
        name.ends_with(".nes") || name.ends_with(".fds")
    };
    let (_, image) = crate::host::zip::file(&bytes, is_game)?;
    Ok(image)
}

/// The cartridge in `image`, or the Disk System with it in, its BIOS found
/// beside the game at `path`, or in roms/nes/, or as the web page keeps it.
fn cartridge(image: &[u8], path: &Path) -> Result<Cartridge, String> {
    let Some(sides) = fds::sides_of(image) else {
        return Cartridge::new(image);
    };
    let places = [
        path.with_file_name(FDS_BIOS),
        Path::new("roms/nes").join(FDS_BIOS),
        Path::new(FDS_BIOS).to_path_buf(),
    ];
    let bios = places.iter().find_map(|place| {
        files::read(place)
            .ok()
            .filter(|bios| bios.len() == FDS_BIOS_SIZE)
    });
    let bios = bios.ok_or(format!(
        "a Disk System game needs its BIOS, {FDS_BIOS}, added beside it"
    ))?;
    Ok(Cartridge::disk(image, sides, &bios))
}

/// What's known of the game at `path` without running it, for a library to
/// keep: a line of its CRC-32 (its ROMs', after the header, as the game
/// database has it), its mapper, whether it's a Famicom's, and whether it's
/// a Zapper's, then why it won't load, if it won't, and its path last, a tab
/// between each. A Disk System's game, its BIOS not needed for this.
pub fn facts(path: &Path) -> String {
    let fact = |image: &[u8]| -> Result<[String; 4], String> {
        if fds::sides_of(image).is_some() {
            let crc = crc32(image);
            return Ok([
                format!("{crc:08X}"),
                "MAPPER 20 DISK SYSTEM".into(),
                "1".into(),
                "0".into(),
            ]);
        }
        let cartridge = Cartridge::new(image)?;
        let trainer = if image[6] & 0x04 != 0 { 512 } else { 0 };
        let name = path
            .file_name()
            .map_or(String::new(), |name| name.to_string_lossy().into_owned());
        let famicom = cartridge.famicom || region::japans_alone(&name);
        Ok([
            format!("{:08X}", crc32(&image[16 + trainer..])),
            cartridge.mapper_name(),
            (famicom as u8).to_string(),
            (cartridge.zapper as u8).to_string(),
        ])
    };
    let [crc, mapper, famicom, zapper, problem] = match image(path).and_then(|image| fact(&image)) {
        Ok([crc, mapper, famicom, zapper]) => [crc, mapper, famicom, zapper, String::new()],
        Err(problem) => ["".into(), "".into(), "0".into(), "0".into(), problem],
    };
    let problem = problem.replace(['\t', '\n'], " ");
    format!(
        "{crc}\t{mapper}\t{famicom}\t{zapper}\t{problem}\t{}",
        path.display()
    )
}

/// `cpu_emu facts`: `facts` of each file named on standard input, a line
/// each, as soon as it's known.
#[cfg(not(target_arch = "wasm32"))]
pub fn report_facts() {
    use std::io::{BufRead, Write};
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        if writeln!(out, "{}", facts(Path::new(&line)))
            .and_then(|_| out.flush())
            .is_err()
        {
            return;
        }
    }
}

/// What's seen of the picture, `overscan` hidden at each edge: its width
/// and height.
fn seen(overscan: usize) -> (usize, usize) {
    (ppu::WIDTH - 2 * overscan, ppu::HEIGHT - 2 * overscan)
}

/// The NES, on whichever host, with the game at `rom` (an iNES image, or a
/// zip of one), its battery's RAM read if it has any, reset; `overscan` dots
/// or lines hidden at each edge of its picture; its `region`, or the
/// game's, as its header or its name says.
pub fn build(rom: &str, overscan: usize, region: Option<Region>) -> Result<CPU<Nes>, String> {
    let path = Path::new(rom);
    let image = image(path).map_err(|e| format!("{rom}: {e}"))?;
    let cartridge = cartridge(&image, path).map_err(|e| format!("{rom}: {e}"))?;
    let named = path
        .file_name()
        .and_then(|name| Region::from_name(&name.to_string_lossy()));
    let region = region.or(cartridge.region).or(named).unwrap_or_default();
    let japans = path
        .file_name()
        .is_some_and(|name| region::japans_alone(&name.to_string_lossy()));
    let famicom = cartridge.famicom || japans;
    let mut nes = Nes::of(cartridge, region);
    if famicom {
        nes.as_famicom();
    }
    if nes.cartridge.battery {
        let file = path.with_extension("srm");
        if let Ok(saved) = files::read(&file) {
            let ram = &mut nes.cartridge.prg_ram;
            let len = saved.len().min(ram.len());
            ram[..len].copy_from_slice(&saved[..len]);
        }
        nes.battery_file = Some(file);
    }
    nes.name = path
        .file_stem()
        .map_or(String::new(), |name| name.to_string_lossy().into_owned());
    nes.overscan = overscan;
    let mut cpu = CPU::with_bus(nes);
    cpu.variant = Variant::Ricoh2A03;
    cpu.clock_hz = region.clock_hz();
    cpu.reset();
    Ok(cpu)
}

/// Only the controls being changed, with no game: its window closing when
/// that's done. A cartridge of nothing, for the window to be a NES's.
#[cfg(not(target_arch = "wasm32"))]
pub fn controls() {
    let mut image = b"NES\x1A\x01\x01".to_vec();
    image.resize(HEADER_AND_BLANK, 0);
    let cartridge = Cartridge::new(&image).unwrap_or_else(|e| panic!("{e}"));
    let mut cpu = CPU::with_bus(Nes::of(cartridge, Region::Ntsc));
    cpu.variant = Variant::Ricoh2A03;
    cpu.memory.overscan = 0;
    let (width, height) = seen(0);
    let window = Window::new(
        "Open Bus: controls",
        width,
        height,
        size(0, Region::Ntsc),
        false,
    );
    Host::new(cpu, Box::new(window), PathBuf::new())
        .only_controls()
        .run();
}

/// How big its picture's shown, `overscan` hidden: its dots and lines seen,
/// each dot as `region`'s TV shows it, 8:7 on NTSC's, 3 times over: 823 by
/// 672, of 240 by 224.
pub fn size(overscan: usize, region: Region) -> (u32, u32) {
    let (width, height) = seen(overscan);
    (
        (width as f32 * region.dot_shape() * 3.0).round() as u32,
        height as u32 * 3,
    )
}

/// The NES as `build` has it, run in a window, `fullscreen` or not, in
/// performance mode if it's `fast`.
#[cfg(not(target_arch = "wasm32"))]
/// `ram` what its RAM holds at power-on.
pub fn run(
    rom: &str,
    fullscreen: bool,
    overscan: usize,
    region: Option<Region>,
    fast: bool,
    ram: crate::host::machine::PowerOnRam,
) {
    let mut cpu = build(rom, overscan, region).unwrap_or_else(|e| panic!("{e}"));
    ram.fill(&mut cpu.memory.ram);
    if fast {
        cpu.memory.performance();
    }
    let region = cpu.memory.region;
    let (width, height) = seen(overscan);
    let title = cpu.memory.title();
    let window = Window::new(&title, width, height, size(overscan, region), fullscreen);
    Host::new(cpu, Box::new(window), save_state::path(Path::new(rom))).run();
}
