//! A web page as a display: what the host shows and plays kept for the page
//! to take after each frame, and what the page is told of the keys and the
//! controls kept for the host. No more of a browser than that: the page
//! has the canvas and the sound, and calls in through `web.rs`.

use crate::host::clock;
use crate::host::display::{Display, Gamepad, Joystick, Key, Pointer, Scancode};
use crate::host::fingers::Fingers;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// What the page and the host share.
#[derive(Default)]
pub struct Shared {
    /// The last picture, four bytes a dot, as a canvas has them; its size.
    pub pixels: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// The sound played and not yet taken; the title.
    pub samples: Vec<f32>,
    pub title: String,
    /// What's pressed, and held; for a machine that takes the keys
    /// themselves, each pressed and let go.
    pub keys: VecDeque<Key>,
    pub key_changes: VecDeque<(Scancode, bool)>,
    pub function_keys: VecDeque<u8>,
    pub gamepad: Gamepad,
    pub gamepad_2: Gamepad,
    pub joystick: Joystick,
    pub pointer: Pointer,
    /// Fingers on the picture, a trackpad for a machine's mouse; and
    /// whether they're the pointer, not the mouse, as they were last used.
    pub fingers: Fingers,
    pub touching: bool,
    pub key_down: bool,
    pub reset: bool,
    pub fast: bool,
    pub rewind: bool,
    /// What the page was given to paste, with PASTE_KEY; and the screen's
    /// text copied, for it to put on the clipboard.
    pub pasted: Option<String>,
    pub copied: Option<String>,
}

/// The display a host has; the page has the same `Shared`.
pub struct Page(pub Rc<RefCell<Shared>>);

impl Display for Page {
    fn show(&mut self, pixels: &[u32], width: usize, height: usize) {
        let page = &mut self.0.borrow_mut();
        page.width = width;
        page.height = height;
        page.pixels.clear();
        page.pixels.extend(pixels.iter().flat_map(|&dot| {
            let [_, red, green, blue] = dot.to_be_bytes();
            [red, green, blue, 0xFF]
        }));
    }

    fn key(&mut self) -> Option<Key> {
        self.0.borrow_mut().keys.pop_front()
    }

    fn key_change(&mut self) -> Option<(Scancode, bool)> {
        self.0.borrow_mut().key_changes.pop_front()
    }

    fn joystick(&self) -> Joystick {
        self.0.borrow().joystick
    }

    fn gamepad(&self) -> Gamepad {
        self.0.borrow().gamepad
    }

    fn gamepad_2(&self) -> Gamepad {
        self.0.borrow().gamepad_2
    }

    fn pointer(&self) -> Pointer {
        let page = self.0.borrow();
        if page.touching {
            return page.fingers.pointer(clock::seconds());
        }
        page.pointer
    }

    fn key_down(&self) -> bool {
        self.0.borrow().key_down
    }

    fn fast_forward(&self) -> bool {
        self.0.borrow().fast
    }

    fn rewinding(&self) -> bool {
        self.0.borrow().rewind
    }

    fn reset_held(&self) -> bool {
        self.0.borrow().reset
    }

    fn function_key(&mut self) -> Option<u8> {
        self.0.borrow_mut().function_keys.pop_front()
    }

    fn clipboard_text(&mut self) -> Option<String> {
        self.0.borrow_mut().pasted.take()
    }

    fn copy_text(&mut self, text: &str) {
        self.0.borrow_mut().copied = Some(text.to_string());
    }

    fn title(&mut self, title: &str) {
        self.0.borrow_mut().title = title.to_string();
    }

    fn play(&mut self, samples: &[f32]) {
        self.0.borrow_mut().samples.extend_from_slice(samples);
    }
}
