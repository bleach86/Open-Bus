//! The host: a machine run, a frame at a time, whichever machine and
//! whichever display. A frame's cycles run, then between frames all that
//! isn't the machine's: its picture shown, with a notice over it if there
//! is one (or its text screen in a terminal), its sound played; the keys,
//! the controls and the Reset button given to it; F2 saving its state and
//! F4 loading it back; F6 changing the controls; F3 the debug panel, how fast it's going, its CPU and
//! what the machine has to say of itself; what it keeps saved; and the
//! window closed, off. And a menu over the game, for all that without a
//! keyboard: F1, or Select and Start tapped together.
//!
//! `Host::frame` is a frame, for whatever calls it a frame at a time;
//! `Host::run` calls it, keeping the machine to its clock.

use crate::host::controls::{ACTIONS, Action, Controls, is_stick, name, said};
use crate::cpu::CPU;
use crate::host::clock;
use crate::host::display::{Display, Gamepad, Key};
use crate::host::files;
use crate::host::machine::{Input, Machine};
use crate::host::notice;
use crate::host::save_state;
use crate::host::stats::Stats;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// The controls being changed: whose; which entry's chosen, and whether it's
/// waiting for a press to bind to it; as they were, to go back to; and
/// whether that's all that's running.
struct Changing {
    player: usize,
    at: usize,
    waiting: bool,
    before: Controls,
    alone: bool,
}

/// What's on the controls' list: an action, to bind; or what to do.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Entry {
    Bind(Action),
    OtherPlayer,
    Defaults,
    Done,
    Cancel,
}

impl Changing {
    /// What's on its list: the first player has the way back too.
    fn entries(&self) -> Vec<Entry> {
        let actions = if self.player == 0 { &ACTIONS[..] } else { &ACTIONS[..ACTIONS.len() - 1] };
        let mut entries: Vec<Entry> = actions.iter().map(|&action| Entry::Bind(action)).collect();
        entries.extend([Entry::OtherPlayer, Entry::Defaults, Entry::Done, Entry::Cancel]);
        entries
    }
}

/// The menu over a game, paused: what's on it, which is chosen, and the
/// gamepad as it was, for what's newly pressed.
struct Menu {
    items: Vec<Item>,
    at: usize,
    before: Gamepad,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Item {
    Resume,
    Save,
    Load,
    /// What the machine has of its own, by its function key.
    Machine(&'static str, u8),
    Reset,
    Controls,
    Quit,
}

impl Item {
    fn name(self) -> &'static str {
        match self {
            Item::Resume => "RESUME",
            Item::Save => "SAVE STATE",
            Item::Load => "LOAD STATE",
            Item::Machine(name, _) => name,
            Item::Reset => "RESET",
            Item::Controls => "CONTROLS",
            Item::Quit => "QUIT",
        }
    }
}

/// F1, the menu, on a keyboard. On a gamepad, Select and Start together, let
/// go before TAP_FRAMES: held longer, they're the way back to a launcher, as
/// the window has it. While they're together the machine sees neither.
const MENU_KEY: u8 = 1;
const TAP_FRAMES: u32 = 45;
/// How long the menu's Reset holds the Reset button.
const RESET_FRAMES: u32 = 6;

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

/// Quick resume, if it's asked for: a state kept as a machine's put away, and
/// gone on from the next time it's run, a file of its own beside the state F2
/// saves, which it leaves alone.
pub static RESUME: AtomicBool = AtomicBool::new(false);

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
    /// The menu, if it's up; how many frames Select and Start have been held
    /// together, and whether they're kept from the machine until they're
    /// let go; the Reset button's frames still to be held, as the menu has
    /// it.
    menu: Option<Menu>,
    both_held: u32,
    holding_back: bool,
    resetting: u32,
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
            menu: None,
            both_held: 0,
            holding_back: false,
            resetting: 0,
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
        self.changing = Some(Changing { player: 0, at: 0, waiting: false, before, alone });
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
    /// would have taken is let go, for it not to rush to catch up after. Up
    /// and down the list by a gamepad or the keys, A or Return to choose,
    /// B or Escape to cancel; an action chosen, the next press bound to it.
    fn changing_frame(&mut self) -> bool {
        self.cycles += (self.cpu.clock_hz / FRAMES_A_SECOND) as u64;
        self.show();
        let mut done = None;
        while let Some(key) = self.display.function_key() {
            match key {
                CONTROLS_KEY => done = Some(true),
                NEXT_PLAYER_KEY => self.choose_entry(Entry::OtherPlayer),
                DEFAULTS_KEY => self.choose_entry(Entry::Defaults),
                _ => {}
            }
        }
        while let Some(source) = self.display.pressed() {
            let Some(changing) = &mut self.changing else { break };
            let entries = changing.entries();
            if changing.waiting {
                changing.waiting = false;
                if source == CANCEL {
                    continue;
                }
                let (player, Entry::Bind(action)) = (changing.player, entries[changing.at]) else { continue };
                if matches!(entries[changing.at + 1], Entry::Bind(_)) {
                    changing.at += 1;
                }
                if let Some(controls) = self.display.controls() {
                    controls.bind(player, action, &source);
                }
                continue;
            }
            match source.as_str() {
                "key:Up" | "pad:dpup" | "pad:lefty-" => changing.at = (changing.at + entries.len() - 1) % entries.len(),
                "key:Down" | "pad:dpdown" | "pad:lefty+" => changing.at = (changing.at + 1) % entries.len(),
                "key:Return" | "pad:a" => match entries[changing.at] {
                    Entry::Done => done = Some(true),
                    Entry::Cancel => done = Some(false),
                    entry => self.choose_entry(entry),
                },
                CANCEL | "pad:b" => done = Some(false),
                _ => {}
            }
            if done.is_some() {
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

    /// An entry of the controls' list chosen: an action to wait for a press
    /// for, the other player's list, or this one's defaults.
    fn choose_entry(&mut self, entry: Entry) {
        let Some(changing) = &mut self.changing else { return };
        let player = changing.player;
        match entry {
            Entry::Bind(_) => changing.waiting = true,
            Entry::OtherPlayer => (changing.player, changing.at, changing.waiting) = (1 - player, 0, false),
            Entry::Defaults => {
                if let Some(controls) = self.display.controls() {
                    controls.reset(player);
                }
            }
            Entry::Done | Entry::Cancel => {}
        }
    }

    /// The controls' panel: each action and what's bound to it, and what
    /// else there is to do.
    fn changing_lines(&mut self) -> Option<Vec<String>> {
        let changing = self.changing.as_ref()?;
        let (player, at, waiting, entries) = (changing.player, changing.at, changing.waiting, changing.entries());
        let controls = self.display.controls()?;
        let mut lines = vec![format!("PLAYER {} CONTROLS", player + 1)];
        for (place, &entry) in entries.iter().enumerate() {
            let marker = if place == at { ">" } else { " " };
            lines.push(match entry {
                Entry::Bind(action) => {
                    let bound: Vec<String> = controls.of(player, action).iter().filter(|source| !is_stick(source)).map(|source| said(source)).collect();
                    format!("{marker}{:8}{}", name(action), bound.join(", "))
                }
                Entry::OtherPlayer => format!("{marker}PLAYER {}", 2 - player),
                Entry::Defaults => format!("{marker}DEFAULTS"),
                Entry::Done => format!("{marker}DONE"),
                Entry::Cancel => format!("{marker}CANCEL"),
            });
        }
        lines.push(String::new());
        lines.push(match (waiting, entries[at]) {
            (true, Entry::Bind(action)) => format!("PRESS FOR {}, ESC TO STOP", name(action)),
            _ => "A: CHOOSE  B: CANCEL".to_string(),
        });
        Some(lines)
    }

    /// The menu over the game, paused: what this machine and this display
    /// have to offer.
    fn open_menu(&mut self) {
        let mut items = vec![Item::Resume, Item::Save, Item::Load];
        items.extend(self.cpu.memory.menu().into_iter().map(|(name, key)| Item::Machine(name, key)));
        items.push(Item::Reset);
        if self.display.controls().is_some() {
            items.push(Item::Controls);
        }
        if self.display.can_close() {
            items.push(Item::Quit);
        }
        self.notice = None;
        self.menu = Some(Menu { items, at: 0, before: self.display.gamepad() });
    }

    /// Whether the menu's asked for by the gamepad: Select and Start tapped
    /// together. Neither's given to the machine once they're together, until
    /// both are let go; the first pressed can reach it before the other is.
    fn menu_asked(&mut self, gamepad: &mut Gamepad) -> bool {
        let both = gamepad.select && gamepad.start;
        let tapped = !both && self.both_held > 0 && self.both_held < TAP_FRAMES;
        self.both_held = if both { self.both_held + 1 } else { 0 };
        self.holding_back = both || (self.holding_back && (gamepad.select || gamepad.start));
        if self.holding_back {
            (gamepad.select, gamepad.start) = (false, false);
        }
        tapped
    }

    /// A frame of the menu: the machine stands still, as for the controls.
    /// Up and down it by the gamepad or the keys, A or Return to choose, B,
    /// Escape or F1 to go back to the game.
    fn menu_frame(&mut self) -> bool {
        self.cycles += (self.cpu.clock_hz / FRAMES_A_SECOND) as u64;
        self.show();
        let Some(menu) = &mut self.menu else { return true };
        let gamepad = self.display.gamepad();
        let newly = |now: bool, was: bool| now && !was;
        let (mut up, mut down) = (newly(gamepad.up, menu.before.up), newly(gamepad.down, menu.before.down));
        let (mut choose, mut back) = (newly(gamepad.a, menu.before.a), newly(gamepad.b, menu.before.b));
        menu.before = gamepad;
        while let Some(key) = self.display.key() {
            match key {
                Key::Up => up = true,
                Key::Down => down = true,
                Key::Char(13) => choose = true,
                Key::Char(27) => back = true,
                _ => {}
            }
        }
        while let Some(key) = self.display.function_key() {
            back |= key == MENU_KEY;
        }
        let count = menu.items.len();
        if up {
            menu.at = (menu.at + count - 1) % count;
        }
        if down {
            menu.at = (menu.at + 1) % count;
        }
        let item = menu.items[menu.at];
        if back {
            self.menu = None;
        } else if choose {
            self.menu = None;
            match item {
                Item::Resume => {}
                Item::Save => self.save_state(),
                Item::Load => self.load_state(),
                Item::Machine(_, key) => {
                    if let Some(text) = self.cpu.memory.function_key(key) {
                        self.say(text);
                    }
                }
                Item::Reset => self.resetting = RESET_FRAMES,
                Item::Controls => self.change_controls(false),
                Item::Quit => self.display.close(),
            }
        }
        if self.display.closed() {
            self.cpu.memory.keep_saved(true);
            return false;
        }
        true
    }

    /// The menu's lines, with the one chosen marked.
    fn menu_lines(&self) -> Option<Vec<String>> {
        let menu = self.menu.as_ref()?;
        let mut lines = vec!["PAUSED".to_string()];
        lines.extend(menu.items.iter().enumerate().map(|(place, item)| format!("{}{}", if place == menu.at { ">" } else { " " }, item.name())));
        lines.push(String::new());
        lines.push("A: CHOOSE  B: BACK".to_string());
        Some(lines)
    }

    /// A frame: false once the window's closed, the machine put away.
    pub fn frame(&mut self) -> bool {
        if self.changing.is_some() {
            return self.changing_frame();
        }
        if self.menu.is_some() {
            return self.menu_frame();
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
        let mut gamepad = self.display.gamepad();
        if self.menu_asked(&mut gamepad) && !self.display.text_only() {
            self.open_menu();
        }
        self.cpu.memory.input(Input {
            joystick: self.display.joystick(),
            gamepad,
            gamepad_2: self.display.gamepad_2(),
            key_down: self.display.key_down(),
        });
        while let Some(key) = self.display.function_key() {
            match key {
                MENU_KEY if !self.display.text_only() => self.open_menu(),
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
        self.cpu.memory.reset_button(self.display.reset_held() || self.resetting > 0);
        self.resetting = self.resetting.saturating_sub(1);
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
        let resuming = RESUME.load(Ordering::Relaxed);
        if resuming {
            self.resume();
        }
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
        if resuming {
            self.keep_for_resuming();
        }
    }

    /// Where the state to resume from is: beside the one F2 saves.
    fn resume_file(&self) -> PathBuf {
        self.state.with_extension("resume")
    }

    /// Gone on from where it was put away, if it was, and said so. A state
    /// that won't load, as one from before a change to what's saved won't,
    /// is told of, and it starts afresh.
    pub fn resume(&mut self) {
        let file = self.resume_file();
        if !files::exists(&file) {
            return;
        }
        let loaded = files::read(&file)
            .map_err(|e| e.to_string())
            .and_then(|state| save_state::restore(&mut self.cpu, &state));
        match loaded {
            Ok(()) => self.say("Resumed"),
            Err(e) => eprintln!("{}: not resumed: {e}", file.display()),
        }
    }

    /// Its state kept, as it's put away, to resume from.
    pub fn keep_for_resuming(&mut self) {
        let file = self.resume_file();
        let kept = save_state::state(&self.cpu).and_then(|state| files::save(&file, &state).map_err(|e| e.to_string()));
        if let Err(e) = kept {
            eprintln!("{}: not kept to resume: {e}", file.display());
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
        let lines = changing.or_else(|| self.menu_lines()).or_else(|| self.debug.then(|| self.debug_lines()));
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
