//! The emulator as a web page has it: its files put in, a machine started,
//! a frame at a time, its picture and sound taken after each, and what's
//! pressed told to it. All the rest is `web/`, the page's own.

use crate::cpu::Variant;
use crate::host::clock;
use crate::host::display::{COPY_KEY, Gamepad, Joystick, Key, PASTE_KEY, Pointer, Scancode};
use crate::host::files;
use crate::host::machine::{Machine, PowerOnRam};
use crate::host::page::{Page, Shared};
use crate::host::run::{self, Host};
use crate::host::save_state;
use crate::machines::apple2::rom_set::Sorted;
use crate::machines::apple2::video::Monitor;
use crate::machines::apple2::{self, Apple2, Model};
use crate::machines::atari2600::{self, Atari2600};
use crate::machines::atari7800::{self, Atari7800};
use crate::machines::facts;
use crate::machines::nes::{self, Nes};
use crate::machines::region::Region;
use crate::machines::x16::{self, X16};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use wasm_bindgen::prelude::*;

/// What's known of the game `name`, its file's `bytes`, a NES's, a 2600's or a 7800's,
/// without running it, as `cpu_emu facts` says it, a line.
#[wasm_bindgen]
pub fn game_facts(name: &str, bytes: Vec<u8>) -> String {
    facts::facts_of(Path::new(name), bytes)
}

/// Which Open Bus this is, as Settings shows it.
#[wasm_bindgen]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

/// Whether the machines started from now on can be rewound, as Settings
/// has it: the page's `--no-rewind`.
#[wasm_bindgen]
pub fn rewind(on: bool) {
    run::REWIND.store(on, Ordering::Relaxed);
}

/// Whether a disk drive's light shows while it's busy, as Settings has it:
/// the page's `--no-drive-light`.
#[wasm_bindgen]
pub fn drive_light(on: bool) {
    run::DRIVE_LIGHT.store(on, Ordering::Relaxed);
}

/// A file put in by the page.
#[wasm_bindgen]
pub fn put_file(name: &str, bytes: Vec<u8>) {
    files::put(Path::new(name), bytes);
}

#[wasm_bindgen]
pub fn read_file(name: &str) -> Option<Vec<u8>> {
    files::read(Path::new(name)).ok()
}

/// The files written since this was last asked, for the page to keep.
#[wasm_bindgen]
pub fn written_files() -> Vec<String> {
    let names = files::written().into_iter();
    names
        .map(|name| name.to_string_lossy().into_owned())
        .collect()
}

/// The files with parts written over since the page took them.
#[wasm_bindgen]
pub fn changed_files() -> Vec<String> {
    let names = files::changed().into_iter();
    names
        .map(|name| name.to_string_lossy().into_owned())
        .collect()
}

/// Where in `name` its parts written over start, for the page to keep.
#[wasm_bindgen]
pub fn take_changed_parts(name: &str) -> Vec<u32> {
    files::take_changed_parts(Path::new(name))
}

#[wasm_bindgen]
pub fn read_part(name: &str, offset: u32) -> Option<Vec<u8>> {
    files::read_part(Path::new(name), offset)
}

/// The Apple's ROMs found in what the page's given.
#[wasm_bindgen]
pub struct RomSorter(Sorted);

impl Default for RomSorter {
    fn default() -> Self {
        RomSorter::new()
    }
}

#[wasm_bindgen]
impl RomSorter {
    #[wasm_bindgen(constructor)]
    pub fn new() -> RomSorter {
        RomSorter(Sorted::new())
    }

    /// The file `name` looked through: whether there were ROMs in it.
    pub fn add(&mut self, name: &str, bytes: &[u8]) -> bool {
        self.0.add(name, bytes)
    }

    pub fn names(&self) -> Vec<String> {
        self.0.names()
    }

    pub fn bytes(&self, name: &str) -> Option<Vec<u8>> {
        self.0.bytes(name)
    }

    /// How many of the II+'s 6 chips there are.
    pub fn plus_chips(&self) -> usize {
        self.0.plus_chips()
    }
}

/// Each machine's host, boxed: they're far apart in size, and only one's
/// ever running.
enum Running {
    Apple(Box<Host<Apple2>>),
    Nes(Box<Host<Nes>>),
    Atari2600(Box<Host<Atari2600>>),
    Atari7800(Box<Host<Atari7800>>),
    X16(Box<Host<X16>>),
}

/// Each running machine's host, `host`, as it's asked of: `$then`.
macro_rules! each {
    ($running:expr, $host:ident => $then:expr) => {
        match $running {
            Running::Apple($host) => $then,
            Running::Nes($host) => $then,
            Running::Atari2600($host) => $then,
            Running::Atari7800($host) => $then,
            Running::X16($host) => $then,
        }
    };
}

#[wasm_bindgen]
pub struct Emulator {
    running: Running,
    page: Rc<RefCell<Shared>>,
    size: (u32, u32),
}

/// The X16's RTC's RAM, as the page keeps it.
const X16_NVRAM: &str = "x16-nvram.bin";

fn model(name: &str) -> Result<Model, String> {
    match name {
        "apple2" => Ok(Model::IIPlus),
        "apple2e" => Ok(Model::IIe),
        "apple2ee" => Ok(Model::IIeEnhanced),
        _ => Err(format!("{name}: not an Apple")),
    }
}

#[wasm_bindgen]
impl Emulator {
    /// The NES, with the game `rom`, and what's chosen of it, each if it
    /// is, else "": its `region`, ntsc or pal; its `console`, nes or
    /// famicom; its second `port`, controller or zapper; its `ram` at
    /// power-on, ones, zeros or random.
    pub fn nes(
        rom: &str,
        overscan: usize,
        region: &str,
        console: &str,
        port: &str,
        ram: &str,
    ) -> Result<Emulator, String> {
        let either = |said: &str, no: &str, yes: &str| match said {
            "" => Ok(None),
            _ if said == no => Ok(Some(false)),
            _ if said == yes => Ok(Some(true)),
            _ => Err(format!("{said}: not {no} or {yes}")),
        };
        let chosen = nes::Chosen {
            region: match region {
                "" => None,
                _ => Some(Region::parse(region).ok_or(format!("{region}: not a region"))?),
            },
            famicom: either(console, "nes", "famicom")?,
            zapper: either(port, "controller", "zapper")?,
        };
        let mut cpu = nes::build(rom, overscan, chosen)?;
        if !ram.is_empty() {
            PowerOnRam::parse(ram)
                .ok_or(format!("{ram}: not ones, zeros or random"))?
                .fill(&mut cpu.memory.ram);
        }
        let region = cpu.memory.region;
        let page = Rc::new(RefCell::new(Shared::default()));
        let state = save_state::path(Path::new(rom));
        let host = Host::new(cpu, Box::new(Page(page.clone())), state);
        Ok(Emulator {
            running: Running::Nes(Box::new(host)),
            page,
            size: nes::size(overscan, region),
        })
    }

    /// The Atari 2600, with the game `rom`, its `region`, ntsc or pal, if
    /// it's chosen, else "".
    pub fn atari2600(rom: &str, region: &str) -> Result<Emulator, String> {
        let region = match region {
            "" => None,
            _ => Some(Region::parse(region).ok_or(format!("{region}: not a region"))?),
        };
        let cpu = atari2600::build(rom, region)?;
        let layout = cpu.memory.layout;
        let page = Rc::new(RefCell::new(Shared::default()));
        let state = save_state::path(Path::new(rom));
        let host = Host::new(cpu, Box::new(Page(page.clone())), state);
        Ok(Emulator {
            running: Running::Atari2600(Box::new(host)),
            page,
            size: atari2600::size(layout),
        })
    }

    /// The Atari 7800, with the game `rom`, its `region`, ntsc or pal, if
    /// it's chosen, else "".
    pub fn atari7800(rom: &str, region: &str) -> Result<Emulator, String> {
        let region = match region {
            "" => None,
            _ => Some(Region::parse(region).ok_or(format!("{region}: not a region"))?),
        };
        let cpu = atari7800::build(rom, region)?;
        let region = cpu.memory.region;
        let page = Rc::new(RefCell::new(Shared::default()));
        let state = save_state::path(Path::new(rom));
        let host = Host::new(cpu, Box::new(Page(page.clone())), state);
        Ok(Emulator {
            running: Running::Atari7800(Box::new(host)),
            page,
            size: atari7800::size(region),
        })
    }

    /// An Apple, `name` apple2, apple2e or apple2ee (the enhanced), its
    /// ROMs `roms` and the `disks` in it.
    pub fn apple(
        name: &str,
        roms: &str,
        disks: Vec<String>,
        mockingboards: usize,
    ) -> Result<Emulator, String> {
        let model = model(name)?;
        let disks: Vec<&str> = disks.iter().map(String::as_str).collect();
        let none: Option<Variant> = None;
        let built = apple2::build(model, roms, &disks, mockingboards, none, Monitor::Colour)?;
        let page = Rc::new(RefCell::new(Shared::default()));
        let mut host = Host::new(built.cpu, Box::new(Page(page.clone())), built.state);
        if let Some(text) = built.notice {
            host.say(text);
        }
        Ok(Emulator {
            running: Running::Apple(Box::new(host)),
            page,
            size: model.size(),
        })
    }

    /// The Commander X16, its ROM `rom`, with `game` (a zip, or a program)
    /// on a card of its own, `card`, made if it isn't there yet; or with
    /// none, "", to BASIC, `card` made blank. Its clock set to the time
    /// where the page is, `local_seconds` after 1970 began; the RTC's RAM
    /// kept in X16_NVRAM.
    pub fn x16(rom: &str, game: &str, card: &str, local_seconds: f64) -> Result<Emulator, String> {
        clock::set_local(local_seconds as i64);
        let card_path = Path::new(card);
        if game.is_empty() {
            x16::blank_card(card_path)?;
        } else {
            x16::make_card(Path::new(game), card_path)?;
        }
        let mut cpu = x16::build(rom, Some(card))?;
        cpu.memory
            .keep_nvram(Path::new(X16_NVRAM), Path::new(X16_NVRAM));
        if !game.is_empty() {
            cpu.memory.name = x16::game_name(Path::new(game));
        }
        let page = Rc::new(RefCell::new(Shared::default()));
        let host = Host::new(
            cpu,
            Box::new(Page(page.clone())),
            save_state::path(card_path),
        );
        let (width, height) = x16::size();
        Ok(Emulator {
            running: Running::X16(Box::new(host)),
            page,
            size: (width, height),
        })
    }

    /// A frame, `local_seconds` after 1970 began, where the page is.
    pub fn frame(&mut self, local_seconds: f64) {
        clock::set_local(local_seconds as i64);
        each!(&mut self.running, host => host.frame());
    }

    /// Frames a second.
    pub fn hz(&self) -> f64 {
        match &self.running {
            Running::Apple(_) => apple2::CLOCK_HZ as f64 / apple2::CYCLES_PER_FRAME as f64,
            Running::Nes(host) => host.cpu.memory.region.frames_per_second(),
            Running::Atari2600(host) => atari2600::frames_per_second(&host.cpu.memory),
            Running::Atari7800(host) => atari7800::frames_per_second(&host.cpu.memory),
            Running::X16(_) => x16::CLOCK_HZ as f64 / x16::FRAME_CYCLES as f64,
        }
    }

    pub fn title(&self) -> String {
        self.page.borrow().title.clone()
    }

    pub fn width(&self) -> usize {
        self.page.borrow().width
    }

    pub fn height(&self) -> usize {
        self.page.borrow().height
    }

    pub fn shown_width(&self) -> u32 {
        self.size.0
    }

    pub fn shown_height(&self) -> u32 {
        self.size.1
    }

    /// The last picture, four bytes a dot.
    pub fn pixels(&self) -> Vec<u8> {
        self.page.borrow().pixels.clone()
    }

    /// The sound since this was last asked, each a left then a right.
    pub fn samples(&self) -> Vec<f32> {
        std::mem::take(&mut self.page.borrow_mut().samples)
    }

    /// A character typed.
    pub fn key_char(&self, code: u8) {
        self.page.borrow_mut().keys.push_back(Key::Char(code));
    }

    /// An arrow pressed: 0 left, 1 right, 2 up, 3 down.
    pub fn key_arrow(&self, arrow: u8) {
        let key = [Key::Left, Key::Right, Key::Up, Key::Down][arrow as usize & 3];
        self.page.borrow_mut().keys.push_back(key);
    }

    /// Whether it's a Disk System, with a disk to change.
    pub fn disk_system(&self) -> bool {
        matches!(&self.running, Running::Nes(host) if host.cpu.memory.cartridge.disk_crc().is_some())
    }

    /// How many keypads a 2600's game has, for their keys to be told of.
    pub fn keypads(&self) -> usize {
        match &self.running {
            Running::Atari2600(host) => host.cpu.memory.keypads(),
            _ => 0,
        }
    }

    /// Whether it's aimed at, as a light gun is, the pointer its.
    pub fn aimed_at(&self) -> bool {
        each!(&self.running, host => host.cpu.memory.aimed_at())
    }

    /// Whether it has a mouse: the pointer its as it's locked, its moves
    /// told however far they go.
    pub fn has_mouse(&self) -> bool {
        each!(&self.running, host => host.cpu.memory.has_mouse())
    }

    /// Whether it takes the keyboard's keys themselves (`key_change`), not
    /// the characters they type.
    pub fn raw_keys(&self) -> bool {
        each!(&self.running, host => host.cpu.memory.raw_keys())
    }

    /// The key at `usage`, its USB HID usage, pressed (`down`, again as it
    /// repeats) or let go.
    pub fn key_change(&self, usage: u16, down: bool) {
        self.page
            .borrow_mut()
            .key_changes
            .push_back((Scancode(usage), down));
    }

    /// The pointer as it is: at `x`, `y` on the picture, in its pixels, if
    /// it's `on` it; its buttons a bit each, the first and the second.
    pub fn pointer(&self, x: f32, y: f32, on: bool, buttons: u8) {
        self.page.borrow_mut().pointer = Pointer {
            at: on.then_some((x, y)),
            buttons: [buttons & 1 != 0, buttons & 2 != 0],
        };
    }

    pub fn function_key(&self, key: u8) {
        self.page.borrow_mut().function_keys.push_back(key);
    }

    /// `text` typed on the machine's keyboard, as it's pasted.
    pub fn paste(&self, text: String) {
        let page = &mut self.page.borrow_mut();
        page.pasted = Some(text);
        page.function_keys.push_back(PASTE_KEY);
    }

    /// The screen's text copied, by the next frame: for `copied`.
    pub fn copy_screen(&self) {
        self.page.borrow_mut().function_keys.push_back(COPY_KEY);
    }

    /// `text` said over the picture, as the host's notices are.
    pub fn say(&mut self, text: &str) {
        each!(&mut self.running, host => host.say(text))
    }

    /// The screen's text, as it was copied, once: for the page to put on
    /// the clipboard.
    pub fn copied(&self) -> Option<String> {
        self.page.borrow_mut().copied.take()
    }

    /// The controls as they are: each gamepad's a bit each, up, down, left,
    /// right, A, B, Select, Start, turbo A and turbo B; and whether
    /// fast-forward's held, and rewind. A number each, as the page calls it
    /// every frame, not an object to be made and taken apart.
    #[allow(clippy::too_many_arguments)]
    pub fn controls(
        &self,
        gamepad: u16,
        gamepad_2: u16,
        x: f32,
        y: f32,
        buttons: u8,
        key_down: bool,
        reset: bool,
        fast: bool,
        rewind: bool,
    ) {
        let pad = |bits: u16| {
            let held = |bit: u16| bits >> bit & 1 != 0;
            Gamepad {
                up: held(0),
                down: held(1),
                left: held(2),
                right: held(3),
                a: held(4),
                b: held(5),
                select: held(6),
                start: held(7),
                // The page's touch and pads give a NES pad's: none of a
                // SNES pad's own.
                x: false,
                y: false,
                l: false,
                r: false,
                turbo_a: held(8),
                turbo_b: held(9),
                accept: held(10),
                back: held(11),
            }
        };
        let page = &mut self.page.borrow_mut();
        page.gamepad = pad(gamepad);
        page.gamepad_2 = pad(gamepad_2);
        page.joystick = Joystick {
            x,
            y,
            buttons: [buttons & 1 != 0, buttons & 2 != 0, buttons & 4 != 0],
        };
        page.key_down = key_down;
        page.reset = reset;
        page.fast = fast;
        page.rewind = rewind;
    }
}
