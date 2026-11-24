//! The host: a machine run, a frame at a time, whichever machine and
//! whichever display. A frame's cycles run, then between frames all that
//! isn't the machine's: its picture shown, with a notice over it if there
//! is one (or its text screen in a terminal), its sound played; the keys,
//! the controls and the Reset button given to it; F2 saving its state and
//! F4 loading it back; rewind, held, going back to what was, a console's
//! last REWIND_SECONDS kept as it runs; F6 changing the controls; F3 the debug panel, how fast it's going, its CPU and
//! what the machine has to say of itself; what it keeps saved; and the
//! window closed, off. And a menu over the game, for all that without a
//! keyboard: F1, or Select and Start tapped together.
//!
//! `Host::frame` is a frame, for whatever calls it a frame at a time;
//! `Host::run` calls it, keeping the machine to its clock.

use crate::cpu::CPU;
use crate::host::clock;
use crate::host::controls::{ACTIONS, Action, BUTTONS, Controls, REMOTE_OK, is_stick, name};
use crate::host::display::{COPY_KEY, Display, Gamepad, Key, PASTE_KEY, Scancode};
use crate::host::files;
use crate::host::machine::{Input, Machine};
use crate::host::notice;
use crate::host::save_state;
use crate::host::saves;
use crate::host::stats::Stats;
use std::collections::VecDeque;
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
    /// What's on its list: the first player has the way back and
    /// fast-forward too.
    fn entries(&self) -> Vec<Entry> {
        let actions = if self.player == 0 {
            &ACTIONS[..]
        } else {
            &ACTIONS[..BUTTONS]
        };
        let mut entries: Vec<Entry> = actions.iter().map(|&action| Entry::Bind(action)).collect();
        entries.extend([
            Entry::OtherPlayer,
            Entry::Defaults,
            Entry::Done,
            Entry::Cancel,
        ]);
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
    Paste,
    Copy,
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
            Item::Paste => "PASTE",
            Item::Copy => "COPY SCREEN",
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
/// The function keys the host has for itself, as above: none of them a
/// machine's that takes the keyboard's keys.
const HOSTS_KEYS: [u8; 5] = [MENU_KEY, 2, 3, 4, CONTROLS_KEY];
const NEXT_PLAYER_KEY: u8 = 7;
const DEFAULTS_KEY: u8 = 8;
const CANCEL: &str = "key:Escape";
const FRAMES_A_SECOND: u32 = 60;

/// How far behind its clock a machine is before it leaves out a picture, if
/// it can: half a frame.
const LATE: Duration = Duration::from_millis(8);
/// How far behind it is when it's been held up, not just slow: paused, as
/// Android pauses it under a dialog, or its window held still. It goes on
/// from there, the time gone let go, not rushing to make it up.
const HELD_UP: Duration = Duration::from_millis(250);

/// How near a display's refresh rate a machine's own frame rate is for its
/// frames to be kept in step with the refresh, a frame to each, not by its
/// clock: a frame a second or so either way. An NES's 60.1 a second, on a
/// TV's 59.94, would otherwise have a frame dropped every 6 seconds, and
/// stutter around it, which shows as the picture scrolls. Its speed's that
/// much slower or quicker, and its sound, which nothing can tell.
const IN_STEP: f64 = 1.5;
/// How many more frames than the refresh's, in step, for the display to be
/// found not to be waiting for it, as a minimised window's may not: kept by
/// the clock again then, for a while.
const NOT_WAITING: f64 = 1.25;
const NOT_IN_STEP_FOR: Duration = Duration::from_secs(5);

/// How many times its own speed it goes, fast-forwarded, as Mesen's 300%:
/// a picture shown of each so many, and no sound, which would only stutter.
const FAST: u64 = 3;
const FAST_NOTICE: &str = "Fast forward";

/// Rewinding, unless it's turned off, as `--no-rewind` does: a moment of a
/// machine that can be, kept every REWIND_EVERY frames, the last
/// REWIND_SECONDS of them. Held, a moment's gone back to each frame, and a
/// frame run from it to see it: back twice as fast as it went, and quiet.
pub static REWIND: AtomicBool = AtomicBool::new(true);
/// A drive's light at the corner of the picture while it's busy, unless
/// it's turned off, as `--no-drive-light` does; lit LIGHT_FRAMES at least,
/// a block read in a moment seen.
pub static DRIVE_LIGHT: AtomicBool = AtomicBool::new(true);
/// Frames kept in step with the display's refresh where they can be, as
/// IN_STEP has it, unless it's turned off, as `--no-vsync` does.
pub static VSYNC: AtomicBool = AtomicBool::new(true);
const LIGHT_FRAMES: u32 = 6;
const REWIND_EVERY: u32 = 2;
const REWIND_SECONDS: u32 = 30;
const REWIND_NOTICE: &str = "Rewind";

/// Quick resume, if it's asked for: a state kept as a machine's put away, and
/// gone on from the next time it's run, a file of its own beside the state F2
/// saves, which it leaves alone.
pub static RESUME: AtomicBool = AtomicBool::new(false);

pub struct Host<M: Machine> {
    /// Whether the machine has a mouse, as the display was last told.
    has_mouse: bool,
    pub cpu: CPU<M>,
    display: Box<dyn Display>,
    /// Where its save state is kept.
    state: PathBuf,
    /// A notice over the picture, and the frames more it's shown; the
    /// picture with it on.
    pub notice: Option<(String, u32)>,
    noticed: Vec<u32>,
    /// The frames more a drive's light is lit.
    light: u32,
    /// The window's title, as last set; the cycles run, as its clock's kept
    /// to them, each fast-forwarded a FAST'th of one.
    title: String,
    pub(crate) cycles: u64,
    /// The debug panel's showing; and how it's going, measured whether it
    /// is or not.
    pub debug: bool,
    stats: Stats,
    /// The controls being changed, the machine paused, if they are.
    changing: Option<Changing>,
    /// Behind its clock, and whether the last frame's picture was left out;
    /// the frames fast-forwarded so far, while it's held.
    late: bool,
    /// The display's refresh rate, while frames are kept in step with it.
    in_step: Option<f64>,
    left_out: bool,
    fast: u64,
    /// The frames run, for turbo's pressing and letting go.
    frames: u32,
    /// The moments kept to rewind to, the last last, if it's kept; and the
    /// frames since the last.
    pub(crate) moments: Option<VecDeque<Vec<u8>>>,
    since_moment: u32,
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
    pub fn new(mut cpu: CPU<M>, mut display: Box<dyn Display>, state: PathBuf) -> Self {
        if !display.text_only() {
            cpu.memory.sound_on();
        }
        display.aiming(cpu.memory.aimed_at());
        let has_mouse = cpu.memory.has_mouse();
        display.mouse(has_mouse);
        display.raw_keys(cpu.memory.raw_keys());
        let moments = (REWIND.load(Ordering::Relaxed) && cpu.memory.rewinds()).then(VecDeque::new);
        Host {
            has_mouse,
            cpu,
            display,
            state,
            notice: None,
            noticed: Vec::new(),
            light: 0,
            title: String::new(),
            cycles: 0,
            debug: false,
            stats: Stats::new(),
            changing: None,
            late: false,
            in_step: None,
            left_out: false,
            fast: 0,
            frames: 0,
            moments,
            since_moment: 0,
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
        let Some(controls) = self.display.controls() else {
            return;
        };
        let before = controls.clone();
        self.display.capture(true);
        self.changing = Some(Changing {
            player: 0,
            at: 0,
            waiting: false,
            before,
            alone,
        });
    }

    /// The controls done with: kept, or put back as they were. Whether the
    /// window's to close.
    fn stop_changing(&mut self, keep: bool) -> bool {
        let Some(changing) = self.changing.take() else {
            return false;
        };
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
            let Some(changing) = &mut self.changing else {
                break;
            };
            let entries = changing.entries();
            if changing.waiting {
                changing.waiting = false;
                // A TV remote's OK, which isn't bound: nothing bound, as
                // Escape, the remote having none.
                if source == CANCEL || source == REMOTE_OK {
                    continue;
                }
                let (player, Entry::Bind(action)) = (changing.player, entries[changing.at]) else {
                    continue;
                };
                if matches!(entries[changing.at + 1], Entry::Bind(_)) {
                    changing.at += 1;
                }
                if let Some(controls) = self.display.controls() {
                    controls.bind(player, action, &source);
                }
                continue;
            }
            // Choosing and going back as the menus' are bound, as they are
            // now; and by the keyboard, and a TV remote's OK, always.
            let mut bound = |action| {
                self.display
                    .controls()
                    .is_some_and(|controls| controls.of(0, action).contains(&source.as_str()))
            };
            let (accept, back) = (
                source == "key:Return" || source == REMOTE_OK || bound(Action::MenuAccept),
                source == CANCEL || bound(Action::MenuBack),
            );
            let Some(changing) = &mut self.changing else {
                break;
            };
            match source.as_str() {
                "key:Up" | "pad:dpup" | "pad:lefty-" => {
                    changing.at = (changing.at + entries.len() - 1) % entries.len()
                }
                "key:Down" | "pad:dpdown" | "pad:lefty+" => {
                    changing.at = (changing.at + 1) % entries.len()
                }
                _ if accept => match entries[changing.at] {
                    Entry::Done => done = Some(true),
                    Entry::Cancel => done = Some(false),
                    entry => self.choose_entry(entry),
                },
                _ if back => done = Some(false),
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
        let Some(changing) = &mut self.changing else {
            return;
        };
        let player = changing.player;
        match entry {
            Entry::Bind(_) => changing.waiting = true,
            Entry::OtherPlayer => {
                (changing.player, changing.at, changing.waiting) = (1 - player, 0, false)
            }
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
        let (player, at, waiting, entries) = (
            changing.player,
            changing.at,
            changing.waiting,
            changing.entries(),
        );
        let controls = self.display.controls()?.clone();
        let mut lines = vec![format!("PLAYER {} CONTROLS", player + 1)];
        for (place, &entry) in entries.iter().enumerate() {
            let marker = if place == at { ">" } else { " " };
            lines.push(match entry {
                Entry::Bind(action) => {
                    let bound: Vec<String> = controls
                        .of(player, action)
                        .iter()
                        .filter(|source| !is_stick(source))
                        .map(|source| self.display.said(source))
                        .collect();
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
            _ => self.menu_hint().replace("BACK", "CANCEL"),
        });
        Some(lines)
    }

    /// The menu over the game, paused: what this machine and this display
    /// have to offer.
    fn open_menu(&mut self) {
        let mut items = vec![Item::Resume, Item::Save, Item::Load];
        items.extend(
            self.cpu
                .memory
                .menu()
                .into_iter()
                .map(|(name, key)| Item::Machine(name, key)),
        );
        if self.cpu.memory.keyboard() && self.display.clipboard() {
            items.extend([Item::Paste, Item::Copy]);
        }
        items.push(Item::Reset);
        if self.display.controls().is_some() {
            items.push(Item::Controls);
        }
        if self.display.can_close() {
            items.push(Item::Quit);
        }
        self.notice = None;
        self.menu = Some(Menu {
            items,
            at: 0,
            before: self.display.gamepad(),
        });
        // The keyboard the host's while it's up, not the machine's, whose
        // keys aren't the menu's (a TV remote's OK did nothing in the X16's
        // menu).
        self.display.raw_keys(false);
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
    /// A frame held still from outside, as Android's own menus over the
    /// game hold it: the machine stands still, its picture kept, and the
    /// time it would have taken let go, for it not to rush after.
    fn still_frame(&mut self) -> bool {
        self.cycles += (self.cpu.clock_hz / FRAMES_A_SECOND) as u64;
        self.show();
        if self.display.closed() {
            self.cpu.memory.keep_saved(true);
            files::all_saved();
            return false;
        }
        true
    }

    fn menu_frame(&mut self) -> bool {
        self.cycles += (self.cpu.clock_hz / FRAMES_A_SECOND) as u64;
        self.show();
        let Some(menu) = &mut self.menu else {
            return true;
        };
        let gamepad = self.display.gamepad();
        let newly = |now: bool, was: bool| now && !was;
        let (mut up, mut down) = (
            newly(gamepad.up, menu.before.up),
            newly(gamepad.down, menu.before.down),
        );
        let (mut choose, mut back) = (
            newly(gamepad.accept, menu.before.accept),
            newly(gamepad.back, menu.before.back),
        );
        menu.before = gamepad;
        // Keys let go meanwhile, let go for the machine too.
        while let Some((key, down)) = self.display.key_change() {
            if !down {
                self.cpu.memory.key_change(key, false);
            }
        }
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
        if back || choose {
            self.menu = None;
            self.display.raw_keys(self.cpu.memory.raw_keys());
        }
        if choose && !back {
            match item {
                Item::Resume => {}
                Item::Save => self.save_state(),
                Item::Load => self.load_state(),
                Item::Machine(_, key) => {
                    if let Some(text) = self.cpu.memory.function_key(key) {
                        self.say(text);
                    }
                }
                Item::Paste => self.paste(),
                Item::Copy => self.copy(),
                Item::Reset => self.resetting = RESET_FRAMES,
                Item::Controls => self.change_controls(false),
                Item::Quit => self.display.close(),
            }
        }
        if self.display.closed() {
            self.cpu.memory.keep_saved(true);
            files::all_saved();
            return false;
        }
        true
    }

    /// The menu's lines, with the one chosen marked.
    pub(crate) fn menu_lines(&mut self) -> Option<Vec<String>> {
        let menu = self.menu.as_ref()?;
        let mut lines = vec!["PAUSED".to_string()];
        lines.extend(menu.items.iter().enumerate().map(|(place, item)| {
            format!(
                "{}{}",
                if place == menu.at { ">" } else { " " },
                item.name()
            )
        }));
        lines.push(String::new());
        lines.push(self.menu_hint());
        Some(lines)
    }

    /// The display's refresh rate, if its frames are to be kept in step with
    /// it: if it's near enough the machine's own frame rate, as measured,
    /// and the display's not lately been found not to be waiting for it.
    fn in_step_with(&self, not_waiting: Option<Instant>) -> Option<f64> {
        if !VSYNC.load(Ordering::Relaxed)
            || not_waiting.is_some_and(|at| at.elapsed() < NOT_IN_STEP_FOR)
        {
            return None;
        }
        let refresh = self.display.refresh_rate()?;
        let cycles = self.stats.cycles_per_frame;
        let rate = self.cpu.clock_hz as f64 / cycles as f64;
        (cycles > 0 && (rate - refresh).abs() < IN_STEP).then_some(refresh)
    }

    /// What chooses and goes back in a menu, as they're bound, for the foot
    /// of one: the gamepad's, as the controls name them.
    fn menu_hint(&mut self) -> String {
        let mut named = |action, or: &str| {
            let source = self.display.controls().and_then(|controls| {
                controls
                    .of(0, action)
                    .into_iter()
                    .find(|source| source.starts_with("pad:"))
                    .map(str::to_string)
            });
            source.map_or(or.to_string(), |source| self.display.said(&source))
        };
        format!(
            "{}: CHOOSE  {}: BACK",
            named(Action::MenuAccept, "A"),
            named(Action::MenuBack, "B")
        )
    }

    /// A frame: false once the window's closed, the machine put away.
    pub fn frame(&mut self) -> bool {
        if self.changing.is_some() {
            return self.changing_frame();
        }
        if self.menu.is_some() {
            return self.menu_frame();
        }
        if self.display.held_still() {
            return self.still_frame();
        }
        let began = clock::seconds();
        let rewinding = self.rewind();
        let before = self.cpu.clock;
        let fast = !rewinding && self.display.fast_forward();
        self.fast = if fast { self.fast + 1 } else { 0 };
        // Their notices for as long as they're held, and no longer.
        let held = if rewinding {
            Some(REWIND_NOTICE)
        } else {
            fast.then_some(FAST_NOTICE)
        };
        match held {
            Some(notice) => self.say(notice),
            None if self
                .notice
                .as_ref()
                .is_some_and(|(text, _)| [FAST_NOTICE, REWIND_NOTICE].contains(&text.as_str())) =>
            {
                self.notice = None;
            }
            None => {}
        }
        // Fast-forwarded, a picture of each FAST; late, one left out, but
        // never two in a row, or there'd be no telling it's still going.
        let unseen = fast && !self.fast.is_multiple_of(FAST);
        let leave_out =
            unseen || self.late && !self.left_out && self.notice.is_none() && !self.debug;
        self.left_out = self.cpu.memory.leave_out_picture(leave_out);
        self.cpu.run_frame();
        self.watch_light();
        // A mouse only while the machine has one on: the display told.
        let has_mouse = self.cpu.memory.has_mouse();
        if has_mouse != self.has_mouse {
            self.has_mouse = has_mouse;
            self.display.mouse(has_mouse);
        }
        let cycles = self.cpu.clock.wrapping_sub(before) as u64;
        self.cycles += if fast { cycles / FAST } else { cycles };
        if !rewinding {
            self.keep_moment();
        }

        if self.left_out || unseen {
            self.display.no_picture();
        } else {
            self.show();
        }
        let machine = &mut self.cpu.memory;
        if !fast && !rewinding {
            self.display.play(machine.sound());
        }
        machine.sound().clear();
        let title = machine.title();
        if title != self.title {
            self.display.title(&title);
            self.title = title;
        }

        // A machine that takes the keyboard's keys has them, not what they
        // type: but the host's own.
        let raw = self.cpu.memory.raw_keys();
        while let Some(key) = self.display.key() {
            if !raw {
                self.cpu.memory.key(key);
            }
        }
        while let Some((key, down)) = self.display.key_change() {
            let hosts = HOSTS_KEYS.map(|n| Scancode::function(n as u16));
            if !hosts.contains(&key) {
                self.cpu.memory.key_change(key, down);
            }
        }
        self.frames = self.frames.wrapping_add(1);
        let mut gamepad = self.display.gamepad().turbo(self.frames);
        if self.menu_asked(&mut gamepad) && !self.display.text_only() {
            self.open_menu();
        }
        self.cpu.memory.input(Input {
            joystick: self.display.joystick(),
            gamepad,
            gamepad_2: self.display.gamepad_2().turbo(self.frames),
            pointer: self.display.pointer(),
            key_down: self.display.key_down(),
        });
        while let Some(key) = self.display.function_key() {
            match key {
                MENU_KEY if !self.display.text_only() => self.open_menu(),
                CONTROLS_KEY => self.change_controls(false),
                2 => self.save_state(),
                3 => self.debug = !self.debug,
                4 => self.load_state(),
                PASTE_KEY => self.paste(),
                COPY_KEY => self.copy(),
                _ => {
                    if let Some(text) = self.cpu.memory.function_key(key) {
                        self.say(text);
                    }
                }
            }
        }
        self.cpu
            .memory
            .reset_button(self.display.reset_held() || self.resetting > 0);
        self.resetting = self.resetting.saturating_sub(1);
        self.cpu.memory.keep_saved(false);
        let now = clock::seconds();
        let waited = self.display.waited();
        self.stats.frame(now, cycles, now - began - waited);
        if self.display.closed() {
            self.cpu.memory.keep_saved(true);
            files::all_saved();
            return false;
        }
        true
    }

    /// Rewind, if it's held and there's a moment to go back to: the machine
    /// as it was at the last kept, the oldest staying as the last there is.
    /// Whether it's rewinding.
    fn rewind(&mut self) -> bool {
        if !self.display.rewinding() {
            return false;
        }
        let Some(moments) = &mut self.moments else {
            return false;
        };
        let moment = if moments.len() > 1 {
            moments.pop_back()
        } else {
            moments.back().cloned()
        };
        let Some(moment) = moment else {
            return false;
        };
        self.since_moment = 0;
        save_state::back_to(&mut self.cpu, &moment).is_ok()
    }

    /// A moment of the machine kept, if it's rewound and it's been
    /// REWIND_EVERY frames: the oldest let go, past REWIND_SECONDS.
    fn keep_moment(&mut self) {
        let Some(moments) = &mut self.moments else {
            return;
        };
        self.since_moment += 1;
        if self.since_moment < REWIND_EVERY {
            return;
        }
        self.since_moment = 0;
        if let Ok(moment) = save_state::moment(&self.cpu) {
            moments.push_back(moment);
        }
        let most = (REWIND_SECONDS * FRAMES_A_SECOND / REWIND_EVERY) as usize;
        while moments.len() > most {
            moments.pop_front();
        }
    }

    /// Frames, each when it's due at the CPU's clock, until the window's
    /// closed. The time due is from the start, so a late frame is made up
    /// by the next; but not the time it was held up for.
    pub fn run(mut self) {
        let resuming = RESUME.load(Ordering::Relaxed);
        if resuming {
            self.resume();
        }
        // Kept by its clock from here, or in step with the display's
        // refresh, as it's found it can be: frames shown since, and when
        // the display was found not to be waiting.
        let (mut start, mut base, mut shown) = (Instant::now(), self.cycles, 0);
        let mut not_waiting: Option<Instant> = None;
        while self.frame() {
            let refresh = self.in_step_with(not_waiting);
            if refresh.is_some() != self.in_step.is_some() {
                self.display.keep_in_step(refresh.is_some());
                (start, base, shown) = (Instant::now(), self.cycles, 0);
                self.in_step = refresh;
                self.late = false;
                continue;
            }
            let elapsed = start.elapsed();
            if let Some(refresh) = refresh {
                shown += 1;
                if self.fast > 0 {
                    (start, shown) = (Instant::now(), 0);
                } else if elapsed > Duration::from_secs(1) {
                    if shown as f64 / elapsed.as_secs_f64() > refresh * NOT_WAITING {
                        not_waiting = Some(Instant::now());
                    }
                    (start, shown) = (Instant::now(), 0);
                }
                continue;
            }
            let due_ns = (self.cycles - base) as u128 * 1_000_000_000 / self.cpu.clock_hz as u128;
            let due = Duration::from_nanos(due_ns as u64);
            if elapsed > due + HELD_UP {
                (start, base) = (Instant::now(), self.cycles);
                self.late = false;
                continue;
            }
            if due > elapsed {
                sleep(due - elapsed);
            }
            self.late = elapsed > due + LATE;
        }
        if resuming {
            self.keep_for_resuming();
        }
    }

    /// The state to resume from, as it would be beside the game: by the one
    /// F2 saves, kept where saves are.
    fn resume_file(&self) -> PathBuf {
        self.state.with_extension("resume")
    }

    /// Gone on from where it was put away, if it was, and said so. A state
    /// that won't load, as one from before a change to what's saved won't,
    /// is told of, and it starts afresh.
    pub fn resume(&mut self) {
        let file = self.resume_file();
        if !saves::exists(&file) {
            return;
        }
        let loaded = saves::read(&file)
            .map_err(|e| e.to_string())
            .and_then(|state| save_state::restore(&mut self.cpu, &state));
        match loaded {
            Ok(()) => self.say("Resumed"),
            Err(e) => eprintln!("{}: not resumed: {e}", file.display()),
        }
    }

    /// Its state kept, as it's put away, to resume from.
    pub fn keep_for_resuming(&mut self) {
        let file = saves::place(&self.resume_file());
        let kept = save_state::state(&self.cpu)
            .and_then(|state| files::save(&file, &state).map_err(|e| e.to_string()));
        if let Err(e) = kept {
            eprintln!("{}: not kept to resume: {e}", file.display());
        }
    }

    /// The debug panel's lines: how it's going, the CPU's, the machine's.
    fn debug_lines(&self) -> Vec<String> {
        let cpu = &self.cpu;
        let mut lines = self.stats.lines(cpu.clock_hz);
        lines.push(match self.in_step {
            Some(refresh) => format!("VSYNC ON {refresh:.0} HZ"),
            None => "VSYNC OFF".to_string(),
        });
        lines.push(format!(
            "PC {:04X} A{:02X} X{:02X} Y{:02X} S{:02X} P{:02X}",
            cpu.pc,
            cpu.a,
            cpu.x,
            cpu.y,
            cpu.sp,
            cpu.status.to_byte()
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
        let lines = changing
            .or_else(|| self.menu_lines())
            .or_else(|| self.debug.then(|| self.debug_lines()));
        let picture = self.cpu.memory.picture();
        if self.notice.is_none() && lines.is_none() && self.light == 0 {
            self.display
                .show(picture.pixels, picture.width, picture.height);
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
        if self.light > 0 {
            notice::light(&mut self.noticed, width, height);
        }
        self.display.show(&self.noticed, width, height);
        if let Some((_, frames)) = &mut self.notice {
            *frames -= 1;
            if *frames == 0 {
                self.notice = None;
            }
        }
    }

    /// The drive's light, as the frame just run left it: lit again if
    /// it's busy, or going out once it's been lit its frames.
    fn watch_light(&mut self) {
        let busy = self.cpu.memory.light();
        if !DRIVE_LIGHT.load(Ordering::Relaxed) {
            return;
        }
        self.light = if busy {
            LIGHT_FRAMES
        } else {
            self.light.saturating_sub(1)
        };
    }

    /// A notice over the picture.
    pub fn say(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), notice::FRAMES));
    }

    /// What's on the clipboard typed on the machine's keyboard, if it has
    /// one: a key at a time, as it takes them.
    fn paste(&mut self) {
        if !self.cpu.memory.keyboard() {
            return;
        }
        match self.display.clipboard_text() {
            Some(text) if !text.is_empty() => {
                self.cpu.memory.type_text(&text);
                self.say("Pasting");
            }
            _ => self.say("Nothing to paste"),
        }
    }

    /// The text on the machine's screen copied, if it has a keyboard and so
    /// text to copy.
    fn copy(&mut self) {
        if !self.cpu.memory.keyboard() {
            return;
        }
        match self.cpu.memory.screen_text() {
            Some(text) if !text.is_empty() => {
                self.display.copy_text(&text);
                self.say("Screen copied");
            }
            _ => self.say("No text on the screen to copy"),
        }
    }

    /// F2: the machine's state saved to its file.
    fn save_state(&mut self) {
        let path = &saves::place(&self.state);
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
        if !saves::exists(&self.state) {
            return self.say("No state saved");
        }
        let loaded = saves::read(&self.state)
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
