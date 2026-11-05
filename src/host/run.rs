//! The host: a machine run, a frame at a time, whichever machine and
//! whichever display. A frame's cycles run, then between frames all that
//! isn't the machine's: its picture shown, with a notice over it if there
//! is one (or its text screen in a terminal), its sound played; the keys,
//! the controls and the Reset button given to it; F2 saving its state and
//! F4 loading it back; what it keeps saved; and the window closed, off.
//!
//! `Host::frame` is a frame, for whatever calls it a frame at a time;
//! `Host::run` calls it, keeping the machine to its clock.

use crate::cpu::CPU;
use crate::host::display::Display;
use crate::host::files;
use crate::host::machine::{Input, Machine};
use crate::host::notice;
use crate::host::save_state;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub struct Host<M: Machine> {
    pub cpu: CPU<M>,
    display: Box<dyn Display>,
    /// Where its save state is kept.
    state: PathBuf,
    /// A notice over the picture, and the frames more it's shown; the
    /// picture with it on.
    pub notice: Option<(String, u32)>,
    noticed: Vec<u32>,
    /// The window's title, as last set; the cycles run.
    title: String,
    cycles: u64,
}

impl<M: Machine> Host<M> {
    pub fn new(mut cpu: CPU<M>, display: Box<dyn Display>, state: PathBuf) -> Self {
        if !display.text_only() {
            cpu.memory.sound_on();
        }
        Host {
            cpu,
            display,
            state,
            notice: None,
            noticed: Vec::new(),
            title: String::new(),
            cycles: 0,
        }
    }

    /// A frame: false once the window's closed, the machine put away.
    pub fn frame(&mut self) -> bool {
        let before = self.cpu.clock;
        self.cpu.run_frame();
        self.cycles += self.cpu.clock.wrapping_sub(before) as u64;

        self.show();
        let machine = &mut self.cpu.memory;
        self.display.play(machine.sound());
        machine.sound().clear();
        let title = machine.title();
        if title != self.title {
            self.display.title(&title);
            self.title = title;
        }

        while let Some(key) = self.display.key() {
            self.cpu.memory.key(key);
        }
        self.cpu.memory.input(Input {
            joystick: self.display.joystick(),
            gamepad: self.display.gamepad(),
            key_down: self.display.key_down(),
        });
        while let Some(key) = self.display.function_key() {
            match key {
                2 => self.save_state(),
                4 => self.load_state(),
                _ => {
                    if let Some(text) = self.cpu.memory.function_key(key) {
                        self.say(text);
                    }
                }
            }
        }
        self.cpu.memory.reset_button(self.display.reset_held());
        self.cpu.memory.keep_saved(false);
        if self.display.closed() {
            self.cpu.memory.keep_saved(true);
            return false;
        }
        true
    }

    /// Frames, each when it's due at the CPU's clock, until the window's
    /// closed. The time due is from the start, so a late frame is made up
    /// by the next.
    pub fn run(mut self) {
        let start = Instant::now();
        while self.frame() {
            let due_ns = self.cycles as u128 * 1_000_000_000 / self.cpu.clock_hz as u128;
            let due = Duration::from_nanos(due_ns as u64);
            let elapsed = start.elapsed();
            if due > elapsed {
                sleep(due - elapsed);
            }
        }
    }

    /// The frame shown: as text, in a terminal; or its picture, with the
    /// notice over it, if there is one.
    fn show(&mut self) {
        if self.display.text_only() {
            if let Some(text) = self.cpu.memory.text_screen() {
                self.display.text(&text);
            }
            return;
        }
        let picture = self.cpu.memory.picture();
        let Some((text, frames)) = &mut self.notice else {
            self.display.show(picture.pixels, picture.width, picture.height);
            return;
        };
        self.noticed.clear();
        self.noticed.extend_from_slice(picture.pixels);
        notice::draw(&mut self.noticed, picture.width, picture.height, text);
        self.display.show(&self.noticed, picture.width, picture.height);
        *frames -= 1;
        if *frames == 0 {
            self.notice = None;
        }
    }

    /// A notice over the picture.
    pub fn say(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), notice::FRAMES));
    }

    /// F2: the machine's state saved to its file.
    fn save_state(&mut self) {
        let path = &self.state;
        let saved = save_state::state(&self.cpu)
            .and_then(|state| files::save(path, &state).map_err(|e| e.to_string()));
        match saved {
            Ok(()) => self.say("State saved"),
            Err(e) => {
                eprintln!("{}: not saved: {e}", path.display());
                self.say("State not saved");
            }
        }
    }

    /// F4: the machine put back as its file has it, if it can be.
    fn load_state(&mut self) {
        if !files::exists(&self.state) {
            return self.say("No state saved");
        }
        let loaded = files::read(&self.state)
            .map_err(|e| e.to_string())
            .and_then(|state| save_state::restore(&mut self.cpu, &state));
        match loaded {
            Ok(()) => self.say("State loaded"),
            Err(e) => {
                eprintln!("{}: not loaded: {e}", self.state.display());
                self.say("State not loaded");
            }
        }
    }
}
