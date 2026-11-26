//! Fingers on a touch screen, as a machine's pointer: for a machine with a
//! mouse, the screen's a trackpad (a finger dragged moves the pointer by
//! as far as it goes, a tap clicks, two fingers tapped together click the
//! second button, and a tap and then a finger held drags with the first
//! button held); for a machine that's aimed at, a light gun's, where a
//! finger is is where it's aimed, and it's fired as it's held.

use crate::host::display::Pointer;

/// A tap: lifted this soon after the first finger landed, in seconds...
const TAP: f64 = 0.25;
/// ...not having moved further than this, in the picture's pixels.
const STILL: f32 = 6.0;
/// A finger landing this soon after a tap: a drag, the button held.
const AGAIN: f64 = 0.3;
/// How long a tap's click, or a shot, is held: for frames enough that the
/// machine sees it.
const CLICK: f64 = 0.08;
/// How far the pointer goes for how far a finger does.
const SPEED: f32 = 1.25;

/// How fingers are the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    /// A trackpad's, the machine's mouse.
    Trackpad,
    /// A light gun's, aimed.
    Aimed,
}

struct Finger {
    id: i64,
    /// How far it's gone since it landed, in the picture's pixels.
    travelled: f32,
}

/// The fingers down, and the pointer they make.
#[derive(Default)]
pub struct Fingers {
    down: Vec<Finger>,
    /// When the first of them landed, and the most down together since.
    landed: Option<f64>,
    most: usize,
    /// When the last tap was lifted, and whether a drag's holding the
    /// first button.
    tapped: Option<f64>,
    dragging: bool,
    /// Until when each button's held, by a tap or a shot.
    until: [Option<f64>; 2],
    pointer: Pointer,
}

impl Fingers {
    pub fn new() -> Fingers {
        Fingers::default()
    }

    /// The pointer, as the fingers have it `now`: in seconds, as
    /// `clock::seconds` has them, as every time given them is.
    pub fn pointer(&self, now: f64) -> Pointer {
        let held = |n: usize| self.until[n].is_some_and(|until| now < until);
        Pointer {
            at: self.pointer.at,
            buttons: [held(0) || self.dragging || self.pointer.buttons[0], held(1)],
        }
    }

    /// The pointer put at `at`, the fingers' gone on from there: as a
    /// mouse captured had it.
    pub fn start_at(&mut self, at: Option<(f32, f32)>) {
        self.pointer.at = at;
    }

    /// Finger `id` landed `now`, at `at` on the picture, in its pixels, of
    /// `picture`'s size.
    pub fn down(&mut self, way: Way, id: i64, at: (f32, f32), picture: (f32, f32), now: f64) {
        if self.down.is_empty() {
            self.landed = Some(now);
            self.most = 0;
            if way == Way::Trackpad {
                self.dragging = self.tapped.is_some_and(|tapped| now - tapped < AGAIN);
            }
        }
        self.down.retain(|finger| finger.id != id);
        self.down.push(Finger { id, travelled: 0.0 });
        self.most = self.most.max(self.down.len());
        match way {
            Way::Trackpad => {
                if self.pointer.at.is_none() {
                    self.pointer.at = Some((picture.0 / 2.0, picture.1 / 2.0));
                }
            }
            Way::Aimed => {
                self.pointer.at = Some(at);
                self.pointer.buttons[0] = true;
                self.until[0] = Some(now + CLICK);
            }
        }
    }

    /// Finger `id` moved: to `at` on the picture, by `by`, in its pixels.
    pub fn moved(&mut self, way: Way, id: i64, at: (f32, f32), by: (f32, f32)) {
        let first = self.down.first().is_some_and(|finger| finger.id == id);
        let Some(finger) = self.down.iter_mut().find(|finger| finger.id == id) else {
            return;
        };
        finger.travelled += by.0.hypot(by.1);
        match way {
            // The first finger moves it; one beside it, as two are tapped,
            // doesn't.
            Way::Trackpad if first && self.down.len() == 1 => {
                let (x, y) = self.pointer.at.unwrap_or_default();
                self.pointer.at = Some((x + by.0 * SPEED, y + by.1 * SPEED));
            }
            Way::Trackpad => {}
            Way::Aimed if first => self.pointer.at = Some(at),
            Way::Aimed => {}
        }
    }

    /// Finger `id` lifted `now`.
    pub fn up(&mut self, way: Way, id: i64, now: f64) {
        let Some(at) = self.down.iter().position(|finger| finger.id == id) else {
            return;
        };
        let finger = self.down.remove(at);
        let still = finger.travelled <= STILL;
        if !self.down.is_empty() {
            if !still {
                self.most = usize::MAX;
            }
            return;
        }
        let quick = self.landed.is_some_and(|landed| now - landed < TAP);
        match way {
            Way::Trackpad if self.dragging => {
                self.dragging = false;
                self.tapped = None;
            }
            Way::Trackpad if quick && still && self.most == 1 => {
                self.until[0] = Some(now + CLICK);
                self.tapped = Some(now);
            }
            Way::Trackpad if quick && still && self.most == 2 => {
                self.until[1] = Some(now + CLICK);
                self.tapped = None;
            }
            Way::Trackpad => self.tapped = None,
            Way::Aimed => self.pointer.buttons[0] = false,
        }
    }

    /// Every finger lifted, as the screen's been left.
    pub fn lift_all(&mut self) {
        self.down.clear();
        self.dragging = false;
        self.pointer.buttons = [false; 2];
    }
}
