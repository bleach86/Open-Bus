//! The host: a machine run, a frame at a time, whichever machine and
//! whichever display. A frame's cycles run, then between frames all that
//! isn't the machine's: its picture shown, with a notice over it if there
//! is one (or its text screen in a terminal), its sound played; the keys,
//! the controls and the Reset button given to it; F2 saving its state and
//! F4 loading it back; F6 changing the controls; F3 the debug panel, how fast it's going, its CPU and
//! what the machine has to say of itself; what it keeps saved; and the
//! window closed, off.
//!
//! `Host::frame` is a frame, for whatever calls it a frame at a time;
//! `Host::run` calls it, keeping the machine to its clock.

use crate::host::controls::{ACTIONS, Action, Controls, is_stick, name, said};
use crate::cpu::CPU;
use crate::host::clock;
use crate::host::display::Display;
use crate::host::files;
use crate::host::machine::{Input, Machine};
use crate::host::notice;
use crate::host::save_state;
use crate::host::stats::Stats;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// The controls being changed: whose; how far through; as they were, to
/// go back to; and whether that's all that's running.
struct Changing {
    player: usize,
    at: usize,
    before: Controls,
    alone: bool,
}

impl Changing {
    /// What's asked for: the first player has the way back too.
    fn actions(&self) -> &'static [Action] {
        if self.player == 0 { &ACTIONS } else { &ACTIONS[..ACTIONS.len() - 1] }
    }
}

/// The function keys while they're changing, and what cancels it; about how
/// many frames a second they're shown at.
const CONTROLS_KEY: u8 = 6;
const NEXT_PLAYER_KEY: u8 = 7;
const DEFAULTS_KEY: u8 = 8;
const CANCEL: &str = "key:Escape";
const FRAMES_A_SECOND: u32 = 60;

/// How far behind its clock a machine is before it leaves out a picture, if
/// it can: half a frame.
const LATE: Duration = Duration::from_millis(8);

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
    /// The debug panel's showing; and how it's going, measured whether it
    /// is or not.
    pub debug: bool,
    stats: Stats,
    /// The controls being changed, the machine paused, if they are.
    changing: Option<Changing>,
    /// Behind its clock, and whether the last frame's picture was left out.
    late: bool,
    left_out: bool,
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
            debug: false,
            stats: Stats::new(),
            changing: None,
            late: false,
            left_out: false,
        }
        .hinted()
    }

    /// With what the display has to say at the start, if anything.
    fn hinted(mut self) -> Self {
        if let Some(hint) = self.display.hint() {
            self.say(hint);
        }
        self
    }

    /// Only the controls, for the window to close when they're done, as
    /// `cpu_emu controls` is.
    pub fn only_controls(mut self) -> Self {
        self.notice = None;
        self.change_controls(true);
        self
    }

    /// F6: the machine paused, and the controls changed, a press each.
    fn change_controls(&mut self, alone: bool) {
        let Some(controls) = self.display.controls() else { return };
        let before = controls.clone();
        self.display.capture(true);
        self.changing = Some(Changing { player: 0, at: 0, before, alone });
    }

    /// The controls done with: kept, or put back as they were. Whether the
    /// window's to close.
    fn stop_changing(&mut self, keep: bool) -> bool {
        let Some(changing) = self.changing.take() else { return false };
        self.display.capture(false);
        if let Some(controls) = self.display.controls() {
            if keep {
                match controls.save() {
                    Ok(()) => self.say("CONTROLS SAVED"),
                    Err(e) => self.say(format!("NOT SAVED: {e}")),
                }
            } else {
                *controls = changing.before;
            }
        }
        changing.alone
    }

    /// A frame of changing them: the machine stands still, and the time it
    /// would have taken is let go, for it not to rush to catch up after.
    fn changing_frame(&mut self) -> bool {
        self.cycles += (self.cpu.clock_hz / FRAMES_A_SECOND) as u64;
        self.show();
        let mut done = None;
        while let Some(key) = self.display.function_key() {
            let player = self.changing.as_ref().map_or(0, |changing| changing.player);
            match key {
                CONTROLS_KEY => done = Some(true),
                NEXT_PLAYER_KEY => {
                    if let Some(changing) = &mut self.changing {
                        (changing.player, changing.at) = (1 - player, 0);
                    }
                }
                DEFAULTS_KEY => {
                    if let Some(controls) = self.display.controls() {
                        controls.reset(player);
                    }
                }
                _ => {}
            }
        }
        while let Some(source) = self.display.pressed() {
            if source == CANCEL {
                done = Some(false);
                break;
            }
            let Some(changing) = &mut self.changing else { break };
            let (player, action) = (changing.player, changing.actions()[changing.at]);
            changing.at += 1;
            let finished = changing.at == changing.actions().len();
            if let Some(controls) = self.display.controls() {
                controls.bind(player, action, &source);
            }
            if finished {
                done = Some(true);
                break;
            }
        }
        if self.display.closed() {
            self.stop_changing(false);
            return false;
        }
        match done {
            Some(keep) => !self.stop_changing(keep),
            None => true,
        }
    }

    /// The controls' panel: each action and what's bound to it.
    fn changing_lines(&mut self) -> Option<Vec<String>> {
        let (player, at) = self.changing.as_ref().map(|changing| (changing.player, changing.at))?;
        let actions = self.changing.as_ref()?.actions();
        let controls = self.display.controls()?;
        let mut lines = vec![format!("PLAYER {}: PRESS A BUTTON FOR", player + 1)];
        for (place, &action) in actions.iter().enumerate() {
            let bound: Vec<String> = controls.of(player, action).iter().filter(|source| !is_stick(source)).map(|source| said(source)).collect();
            let marker = if place == at { ">" } else { " " };
            lines.push(format!("{marker}{:8}{}", name(action), bound.join(", ")));
        }
        lines.push(String::new());
        lines.push("F6 DONE  ESC CANCEL".to_string());
        lines.push("F7 OTHER PLAYER  F8 DEFAULTS".to_string());
        Some(lines)
    }

    /// A frame: false once the window's closed, the machine put away.
    pub fn frame(&mut self) -> bool {
        if self.changing.is_some() {
            return self.changing_frame();
        }
        let began = clock::seconds();
        let before = self.cpu.clock;
        // Never two in a row, or there'd be no telling it's still going.
        let leave_out = self.late && !self.left_out && self.notice.is_none() && !self.debug;
        self.left_out = self.cpu.memory.leave_out_picture(leave_out);
        self.cpu.run_frame();
        let cycles = self.cpu.clock.wrapping_sub(before) as u64;
        self.cycles += cycles;

        if self.left_out {
            self.display.no_picture();
        } else {
            self.show();
        }
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
            gamepad_2: self.display.gamepad_2(),
            key_down: self.display.key_down(),
        });
        while let Some(key) = self.display.function_key() {
            match key {
                CONTROLS_KEY => self.change_controls(false),
                2 => self.save_state(),
                3 => self.debug = !self.debug,
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
        let now = clock::seconds();
        self.stats.frame(now, cycles, now - began);
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
            self.late = elapsed > due + LATE;
        }
    }

    /// The debug panel's lines: how it's going, the CPU's, the machine's.
    fn debug_lines(&self) -> Vec<String> {
        let cpu = &self.cpu;
        let mut lines = self.stats.lines(cpu.clock_hz);
        lines.push(format!(
            "PC {:04X} A{:02X} X{:02X} Y{:02X} S{:02X} P{:02X}",
            cpu.pc, cpu.a, cpu.x, cpu.y, cpu.sp, cpu.status.to_byte()
        ));
        lines.extend(cpu.memory.debug());
        lines
    }

    /// The frame shown: as text, in a terminal; or its picture, with the
    /// notice and the debug panel over it, if they're on.
    fn show(&mut self) {
        if self.display.text_only() {
            if let Some(text) = self.cpu.memory.text_screen() {
                self.display.text(&text);
            }
            return;
        }
        let changing = self.changing_lines();
        let lines = changing.or_else(|| self.debug.then(|| self.debug_lines()));
        let picture = self.cpu.memory.picture();
        if self.notice.is_none() && lines.is_none() {
            self.display.show(picture.pixels, picture.width, picture.height);
            return;
        }
        let (width, height) = (picture.width, picture.height);
        self.noticed.clear();
        self.noticed.extend_from_slice(picture.pixels);
        if let Some(lines) = &lines {
            notice::panel(&mut self.noticed, width, height, lines);
        }
        if let Some((text, _)) = &self.notice {
            notice::draw(&mut self.noticed, width, height, text);
        }
        self.display.show(&self.noticed, width, height);
        if let Some((_, frames)) = &mut self.notice {
            *frames -= 1;
            if *frames == 0 {
                self.notice = None;
            }
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
