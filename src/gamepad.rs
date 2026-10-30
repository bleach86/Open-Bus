//! Gamepads, read straight from evdev, into the input chip. Any input device with gamepad buttons
//! counts, with or without analog sticks; gilrs wants at least two axes, which
//! a button-only pad like the RG351MP's does not have.

use crate::backlight::Blanker;
use crate::input::{self as io, Input};
use evdev::{AbsoluteAxisCode, Device, EventSummary, KeyCode};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// How often to look for newly connected gamepads.
const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

const BTN_SOUTH: u16 = 0x130;

/// Linux button codes and their `PAD_*` bits. Pads put their printed X and
/// Y on `BTN_X` (0x133) and `BTN_Y` (0x134), which the kernel also calls
/// `BTN_NORTH` and `BTN_WEST`.
const BUTTONS: [(u16, u16); 16] = [
    (BTN_SOUTH, io::PAD_A),
    (0x131, io::PAD_B),      // BTN_EAST
    (0x133, io::PAD_X),      // BTN_X
    (0x134, io::PAD_Y),      // BTN_Y
    (0x136, io::PAD_L1),     // BTN_TL
    (0x137, io::PAD_R1),     // BTN_TR
    (0x138, io::PAD_L2),     // BTN_TL2
    (0x139, io::PAD_R2),     // BTN_TR2
    (0x13A, io::PAD_SELECT), // BTN_SELECT
    (0x13B, io::PAD_START),  // BTN_START
    (0x13D, io::PAD_L3),     // BTN_THUMBL
    (0x13E, io::PAD_R3),     // BTN_THUMBR
    (0x220, io::PAD_UP),     // BTN_DPAD_UP
    (0x221, io::PAD_DOWN),   // BTN_DPAD_DOWN
    (0x222, io::PAD_LEFT),   // BTN_DPAD_LEFT
    (0x223, io::PAD_RIGHT),  // BTN_DPAD_RIGHT
];

/// The gamepads, each read by its own thread that sleeps until the pad
/// sends something, so a press reaches the input chip at once and an idle
/// pad costs nothing. Select + Start together is the reset button; a press
/// that only wakes a blank screen is ignored until released.
pub struct Gamepads {
    input: Arc<Input>,
    blanker: Option<Arc<Mutex<Blanker>>>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// Buttons held on each connected pad.
    held: HashMap<PathBuf, u16>,
    /// Buttons that woke the screen, until they are let go.
    ignored: u16,
    reset_held: bool,
}

impl Gamepads {
    /// Start reading every gamepad there is now or is plugged in later.
    pub fn start(input: Arc<Input>, blanker: Option<Arc<Mutex<Blanker>>>) {
        let pads = Arc::new(Gamepads { input, blanker, state: Mutex::default() });
        thread::spawn(move || loop {
            for (path, device) in evdev::enumerate() {
                let is_gamepad = device
                    .supported_keys()
                    .is_some_and(|keys| keys.contains(KeyCode::new(BTN_SOUTH)));
                if !is_gamepad || pads.state.lock().unwrap().held.contains_key(&path) {
                    continue;
                }
                if device.set_nonblocking(false).is_err() {
                    continue;
                }
                pads.state.lock().unwrap().held.insert(path.clone(), 0);
                let pads = Arc::clone(&pads);
                thread::spawn(move || pads.read(path, device));
            }
            thread::sleep(RESCAN_INTERVAL);
        });
    }

    /// Read one pad until it goes away.
    fn read(&self, path: PathBuf, mut device: Device) {
        loop {
            let events: Vec<_> = match device.fetch_events() {
                Ok(events) => events.map(|event| event.destructure()).collect(),
                Err(_) => break,
            };
            let mut state = self.state.lock().unwrap();
            let mut buttons = state.held.get(&path).copied().unwrap_or(0);
            let mut woke = 0;
            for event in events {
                buttons = apply(buttons, event, &mut |bit| {
                    if self.blanker.as_ref().is_some_and(|b| b.lock().unwrap().input()) {
                        woke |= bit;
                    }
                });
            }
            state.held.insert(path.clone(), buttons);
            state.ignored |= woke;
            self.publish(&mut state);
        }
        // Gone: forget what it held rather than leave it stuck.
        let mut state = self.state.lock().unwrap();
        state.held.remove(&path);
        self.publish(&mut state);
    }

    /// The buttons held across every pad, to the input chip.
    fn publish(&self, state: &mut State) {
        let all = state.held.values().fold(0, |all, &held| all | held);
        state.ignored &= all;
        let buttons = all & !state.ignored;
        self.input.pad.store(buttons, Ordering::Relaxed);
        let reset = io::PAD_SELECT | io::PAD_START;
        let reset_held = buttons & reset == reset;
        if reset_held && !state.reset_held {
            self.input.reset.store(true, Ordering::Release);
        }
        state.reset_held = reset_held;
    }
}

fn apply(buttons: u16, event: EventSummary, pressed: &mut impl FnMut(u16)) -> u16 {
    match event {
        EventSummary::Key(_, code, value) => {
            let Some(&(_, bit)) = BUTTONS.iter().find(|(c, _)| *c == code.code()) else {
                return buttons;
            };
            match value {
                1 => {
                    pressed(bit);
                    buttons | bit
                }
                0 => buttons & !bit,
                _ => buttons, // autorepeat
            }
        }
        // Most PC pads report the D-pad as a hat axis instead of buttons.
        EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_HAT0X, value) => {
            hat(buttons, value, io::PAD_LEFT, io::PAD_RIGHT, pressed)
        }
        EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_HAT0Y, value) => {
            hat(buttons, value, io::PAD_UP, io::PAD_DOWN, pressed)
        }
        _ => buttons,
    }
}

fn hat(buttons: u16, value: i32, negative: u16, positive: u16, pressed: &mut impl FnMut(u16)) -> u16 {
    let held = match value.signum() {
        -1 => negative,
        1 => positive,
        _ => 0,
    };
    if held != 0 && buttons & held == 0 {
        pressed(held);
    }
    buttons & !(negative | positive) | held
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hat_presses_once_and_releases() {
        let mut presses = Vec::new();
        let mut pressed = |bit| presses.push(bit);
        let held = hat(0, -1, io::PAD_LEFT, io::PAD_RIGHT, &mut pressed);
        assert_eq!(held, io::PAD_LEFT);
        let held = hat(held, -1, io::PAD_LEFT, io::PAD_RIGHT, &mut pressed);
        assert_eq!(held, io::PAD_LEFT, "still held, no second press");
        let held = hat(held | io::PAD_A, 0, io::PAD_LEFT, io::PAD_RIGHT, &mut pressed);
        assert_eq!(held, io::PAD_A, "centred releases only the hat");
        assert_eq!(presses, [io::PAD_LEFT]);
    }
}
