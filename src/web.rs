//! The emulator as a web page has it: its files put in, a machine started,
//! a frame at a time, its picture and sound taken after each, and what's
//! pressed told to it. All the rest is `web/`, the page's own.

use crate::cpu::Variant;
use crate::host::clock;
use crate::host::display::{Gamepad, Joystick, Key};
use crate::host::files;
use crate::host::page::{Page, Shared};
use crate::host::run::Host;
use crate::host::save_state;
use crate::machines::apple2::video::Monitor;
use crate::machines::apple2::{self, Apple2, Model};
use crate::machines::nes::{self, Nes};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

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
    names.map(|name| name.to_string_lossy().into_owned()).collect()
}

/// The files with parts written over since the page took them.
#[wasm_bindgen]
pub fn changed_files() -> Vec<String> {
    let names = files::changed().into_iter();
    names.map(|name| name.to_string_lossy().into_owned()).collect()
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

enum Running {
    Apple(Host<Apple2>),
    Nes(Host<Nes>),
}

#[wasm_bindgen]
pub struct Emulator {
    running: Running,
    page: Rc<RefCell<Shared>>,
    size: (u32, u32),
}

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
    /// The NES, with the game `rom`.
    pub fn nes(rom: &str, overscan: usize) -> Result<Emulator, String> {
        let cpu = nes::build(rom, overscan, None)?;
        let region = cpu.memory.region;
        let page = Rc::new(RefCell::new(Shared::default()));
        let state = save_state::path(Path::new(rom));
        let host = Host::new(cpu, Box::new(Page(page.clone())), state);
        Ok(Emulator {
            running: Running::Nes(host),
            page,
            size: nes::size(overscan, region),
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
            running: Running::Apple(host),
            page,
            size: model.size(),
        })
    }

    /// A frame, `local_seconds` after 1970 began, where the page is.
    pub fn frame(&mut self, local_seconds: f64) {
        clock::set_local(local_seconds as i64);
        match &mut self.running {
            Running::Apple(host) => host.frame(),
            Running::Nes(host) => host.frame(),
        };
    }

    /// Frames a second.
    pub fn hz(&self) -> f64 {
        match &self.running {
            Running::Apple(_) => apple2::CLOCK_HZ as f64 / apple2::CYCLES_PER_FRAME as f64,
            Running::Nes(host) => host.cpu.memory.region.frames_per_second(),
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

    pub fn function_key(&self, key: u8) {
        self.page.borrow_mut().function_keys.push_back(key);
    }

    /// The controls as they are: the gamepad's a bit each, up, down, left,
    /// right, A, B, Select, Start.
    pub fn controls(&self, gamepad: u8, x: f32, y: f32, buttons: u8, key_down: bool, reset: bool) {
        let held = |bit: u8| gamepad >> bit & 1 != 0;
        let page = &mut self.page.borrow_mut();
        page.gamepad = Gamepad {
            up: held(0),
            down: held(1),
            left: held(2),
            right: held(3),
            a: held(4),
            b: held(5),
            select: held(6),
            start: held(7),
        };
        page.joystick = Joystick {
            x,
            y,
            buttons: [buttons & 1 != 0, buttons & 2 != 0, buttons & 4 != 0],
        };
        page.key_down = key_down;
        page.reset = reset;
    }
}
