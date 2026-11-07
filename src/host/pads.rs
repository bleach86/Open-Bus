//! Gamepads, as SDL has them: the players', what each holds in the bindings'
//! words, and, for the desktop app's library, those told to it as they
//! change, since a webview's own idea of them is its gamepad library's, which
//! a D-pad can get past.

use crate::host::controls::{Action, Controls, word};
use sdl2::GameControllerSubsystem;
use sdl2::controller::{Axis, GameController};
use sdl2::event::Event;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// A stick's travel taken as centred: a resting gamepad's isn't quite 0.
const DEAD_ZONE: f32 = 0.15;
/// How far a stick or trigger's pushed to be held.
const PUSHED: f32 = 0.5;
/// How often the library's are looked at, and how long they've to say what's
/// held already before the first's told.
const LOOK_EVERY: Duration = Duration::from_millis(4);
const SETTLE: Duration = Duration::from_millis(150);

/// What the library moves by, as it's bound, and its shoulder buttons.
const LIBRARY: [Action; 7] = [Action::Up, Action::Down, Action::Left, Action::Right, Action::A, Action::B, Action::Start];
const SHOULDERS: [(&str, &str); 2] = [("leftshoulder", "previous"), ("rightshoulder", "next")];

/// To be said before SDL starts, by anything using it for gamepads: that
/// buttons go by where they are; and SDL's own driver for Switch
/// controllers, closing, leaves one in a mode Linux's driver doesn't read,
/// dead to everything else until it's plugged in again: with that driver
/// there, it's left to it, and SDL reads what it gives.
pub fn prepare() {
    // Buttons by where they are, not what they're labelled: a Nintendo
    // pad's A, on the right, is an Xbox pad's B. The defaults are by where.
    sdl2::hint::set("SDL_GAMECONTROLLER_USE_BUTTON_LABELS", "0");
    if Path::new("/sys/module/hid_nintendo").exists() {
        sdl2::hint::set("SDL_JOYSTICK_HIDAPI_SWITCH", "0");
        sdl2::hint::set("SDL_JOYSTICK_HIDAPI_JOY_CONS", "0");
    }
}

/// A player's gamepad: its left stick, -1 to 1 each way, for a joystick; and
/// what's held of it, by the bindings' words for its buttons, and its sticks'
/// and triggers' for being pushed well over.
pub struct Pad {
    controller: GameController,
    pub stick: (f32, f32),
    pub held: HashSet<String>,
}

impl Pad {
    fn new(controller: GameController) -> Pad {
        Pad {
            controller,
            stick: (0.0, 0.0),
            held: HashSet::new(),
        }
    }
}

/// The players' gamepads.
pub struct Pads {
    controllers: GameControllerSubsystem,
    places: [Option<Pad>; 2],
}

impl Pads {
    /// Those there already, as SDL says at the start.
    pub fn new(controllers: GameControllerSubsystem) -> Pads {
        let mut opened = (0..controllers.num_joysticks().unwrap_or(0))
            .filter(|&n| controllers.is_game_controller(n))
            .filter_map(|n| controllers.open(n).ok())
            .map(Pad::new);
        let places = [opened.next(), opened.next()];
        drop(opened);
        Pads { controllers, places }
    }

    /// The first there is.
    pub fn first(&self) -> Option<&Pad> {
        self.places.iter().flatten().next()
    }

    /// Whether `button`'s held by `player`.
    pub fn holds(&self, player: usize, button: &str) -> bool {
        self.places[player].as_ref().is_some_and(|pad| pad.held.contains(button))
    }

    /// The gamepad of instance `which`, if it's a player's.
    fn of(&mut self, which: u32) -> Option<&mut Pad> {
        self.places.iter_mut().flatten().find(|pad| pad.controller.instance_id() == which)
    }

    /// A gamepad plugged in, or there already, as SDL says at the start: the
    /// first place free's, unless it has one.
    fn plugged_in(&mut self, device: u32) {
        let Some(place) = self.places.iter().position(Option::is_none) else {
            return;
        };
        let Ok(controller) = self.controllers.open(device) else {
            return;
        };
        if self.of(controller.instance_id()).is_none() {
            self.places[place] = Some(Pad::new(controller));
        }
    }

    /// `event`, if it's a gamepad's, taken in; what it newly pressed, as a
    /// binding, if it pressed anything.
    pub fn take(&mut self, event: &Event) -> Option<String> {
        match *event {
            Event::ControllerDeviceAdded { which, .. } => self.plugged_in(which),
            Event::ControllerDeviceRemoved { which, .. } => {
                let place = self.places.iter_mut().find(|pad| {
                    pad.as_ref().is_some_and(|pad| pad.controller.instance_id() == which)
                });
                if let Some(place) = place {
                    *place = None;
                }
            }
            Event::ControllerAxisMotion { which, axis, value, .. } => {
                let pad = self.of(which)?;
                let travel = (value as f32 / 32767.0).clamp(-1.0, 1.0);
                let settled = if travel.abs() < DEAD_ZONE { 0.0 } else { travel };
                match axis {
                    Axis::LeftX => pad.stick.0 = settled,
                    Axis::LeftY => pad.stick.1 = settled,
                    _ => {}
                }
                let name = axis.string();
                let trigger = matches!(axis, Axis::TriggerLeft | Axis::TriggerRight);
                let ways = if trigger {
                    vec![(name.clone(), travel)]
                } else {
                    vec![(format!("{name}-"), -travel), (format!("{name}+"), travel)]
                };
                let mut pressed = None;
                for (way, pushed) in ways {
                    if pushed > PUSHED {
                        if pad.held.insert(way.clone()) {
                            pressed = Some(format!("pad:{way}"));
                        }
                    } else {
                        pad.held.remove(&way);
                    }
                }
                return pressed;
            }
            Event::ControllerButtonDown { which, button, .. } => {
                let pad = self.of(which)?;
                pad.held.insert(button.string());
                return Some(format!("pad:{}", button.string()));
            }
            Event::ControllerButtonUp { which, button, .. } => {
                self.of(which)?.held.remove(&button.string());
            }
            _ => {}
        }
        None
    }

    /// What's held for moving around the library, on either gamepad, as a
    /// line of words.
    fn library(&self, controls: &Controls) -> String {
        let mut words: Vec<&str> = Vec::new();
        for player in 0..2 {
            let held = |source: &str| source.strip_prefix("pad:").is_some_and(|button| self.holds(player, button));
            words.extend(LIBRARY.iter().filter(|&&action| controls.holds(player, action, held)).map(|&action| word(action)));
            words.extend(SHOULDERS.iter().filter(|(button, _)| self.holds(player, button)).map(|(_, name)| *name));
        }
        words.sort_unstable();
        words.dedup();
        words.join(" ")
    }
}

/// `cpu_emu pads`: no window, and what's held told on standard output, a
/// line for each change and one to start, until standard input closes. With
/// no window to have the focus, SDL has to be told to listen anyway; a game
/// isn't to be left competing with it, so this goes when it's told, and on
/// its own if the app that started it does.
pub fn report() {
    prepare();
    sdl2::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
    let sdl = sdl2::init().expect("SDL");
    let mut pads = Pads::new(sdl.game_controller().expect("SDL's game controllers"));
    let mut events = sdl.event_pump().expect("SDL's events");
    let controls = Controls::load();
    let told_to_go = Arc::new(AtomicBool::new(false));
    let told = told_to_go.clone();
    std::thread::spawn(move || {
        let _ = std::io::stdin().read_to_end(&mut Vec::new());
        told.store(true, Ordering::Relaxed);
    });
    let started = Instant::now();
    let mut said = None;
    while !told_to_go.load(Ordering::Relaxed) {
        for event in events.poll_iter() {
            pads.take(&event);
        }
        if started.elapsed() >= SETTLE {
            let held = pads.library(&controls);
            if said.as_ref() != Some(&held) {
                let mut out = std::io::stdout();
                if writeln!(out, "{held}").and_then(|_| out.flush()).is_err() {
                    return;
                }
                said = Some(held);
            }
        }
        std::thread::sleep(LOOK_EVERY);
    }
}
