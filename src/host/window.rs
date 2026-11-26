//! The host's window, by SDL2: a machine's frames, scaled up; the keys
//! typed in it; and a joystick, a gamepad's (its left stick or D-pad, A and
//! B its buttons) or the numeric keypad's (8, 2, 4 and 6 and the corners; 0
//! and . its buttons), the Alt keys buttons too; a console's gamepads, the
//! host's first (its D-pad or left stick; its south button A, its west B;
//! Back and Start) or the keyboard's (the arrows; X and Z, A and B; Right
//! Shift or Tab, Select; Return, Start), and the host's second, each
//! keeping its place if the other's taken out; F1 to F10, for the machine's
//! own uses, and F12, its Reset key; and the machine's sound, in stereo,
//! through an audio queue. F11 (on a Mac, Ctrl+Cmd+F) or a double click
//! (but for a machine that's aimed at or has a mouse) switches it to
//! fullscreen and back, the picture its shape still, bars at its sides if
//! the screen is wider. The mouse on the picture, for a machine that's
//! aimed at, its pointer cross hairs then, and its clicks the machine's.
//! For a machine with a mouse, a click on the picture captures it, its
//! cursor gone, its moves the machine's however far they go; Ctrl+M, or
//! another window, lets it go, as the title says. Fingers on a touch
//! screen are a trackpad for a machine with a mouse, and aim a light gun
//! (`fingers`). For a machine that takes
//! the keyboard's keys, each key's pressed and let go, by its place on the
//! keyboard, but those it keeps for its own (F11, F12, the clipboard's,
//! Ctrl+M), and let go as another window's chosen; none of them a
//! gamepad's. Ctrl+Shift+V or Shift+Insert, the clipboard pasted, and
//! Ctrl+Shift+C or Ctrl+Insert, the screen copied to it, for a machine with
//! a keyboard. Closed, it tells the machine, to turn itself off.

use crate::host::clock;
use crate::host::controls::{Action, Controls, REMOTE_OK, said};
use crate::host::display::{
    CHANNELS, COPY_KEY, Display, Gamepad, Joystick, Key, PASTE_KEY, Pointer, SAMPLE_RATE, Scancode,
};
use crate::host::fingers::{Fingers, Way};
use crate::host::pads::{self, Pads};
use crate::host::sound::Stretch;
use crate::host::touch;
use sdl2::EventPump;
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::clipboard::ClipboardUtil;
use sdl2::controller::Button;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::{Cursor, MouseButton, MouseUtil, SystemCursor};
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::FullscreenType;
use std::collections::{HashSet, VecDeque};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Where a window's to open, and how big, if it's said: its X, Y, width and
/// height.
pub static PLACE: OnceLock<[i32; 4]> = OnceLock::new();

/// Whether something else started this, to go back to, and has said so: as
/// any launcher can, a handheld's having no keys and no Guide button either.
pub static LAUNCHED: AtomicBool = AtomicBool::new(false);

/// Whether there's a launcher to go back to: that's said, or it placed us.
fn launched() -> bool {
    PLACE.get().is_some() || LAUNCHED.load(Ordering::Relaxed)
}

pub struct Window {
    canvas: Canvas<sdl2::video::Window>,
    /// The mouse: its pointer hidden in fullscreen.
    mouse: MouseUtil,
    clipboard: ClipboardUtil,
    texture: Texture<'static>,
    events: EventPump,
    keys: VecDeque<Key>,
    /// F1 to F10 pressed, by number, and PASTE_KEY and COPY_KEY, not yet
    /// taken.
    function_keys: VecDeque<u8>,
    audio: Option<AudioQueue<f32>>,
    /// The sound on its way to the queue, stretched or squeezed to keep
    /// as much in it as it's to have; how much it's had lately; and what's
    /// to go in it.
    stretch: Stretch<CHANNELS>,
    queued: f32,
    played: Vec<f32>,
    /// Whether pictures are shown in step with the display's refresh, and
    /// the seconds spent waiting for it, not yet asked for.
    in_step: bool,
    waited: f64,
    /// The players' gamepads.
    pads: Pads,
    /// The keypad's keys held, and the Alt keys.
    keypad_held: HashSet<Keycode>,
    alt_held: [bool; 2],
    right_shift_held: bool,
    /// F12, the Reset key; and the other keys held, but the modifiers and
    /// the function keys.
    reset_held: bool,
    keys_held: HashSet<Keycode>,
    /// The window closed.
    closed: bool,
    /// What's bound; everything held that a binding could say; what's been
    /// pressed, while the host's looking; and since when Select and Start
    /// have both been held.
    controls: Controls,
    held: HashSet<String>,
    pressed: VecDeque<String>,
    capturing: bool,
    both_held: Option<Instant>,
    /// A keypad key's own text, which it would type too: not typed.
    keypad_text: Option<char>,
    /// When the pointer last moved.
    pointer_moved: Instant,
    /// The picture's size, and the window's as SDL scales the mouse's
    /// place to it, its logical size.
    picture: (usize, usize),
    logical: (u32, u32),
    /// The mouse on the picture; and whether the machine's aimed at, and
    /// the cross hairs it's shown as then.
    pointer: Pointer,
    /// Fingers on a touch screen, and whether they're the pointer, not
    /// the mouse, as they were last used.
    fingers: Fingers,
    touching: bool,
    aiming: bool,
    cross_hairs: Option<Cursor>,
    /// Whether the machine has a mouse, and whether it's captured; the
    /// title, as the machine has it.
    has_mouse: bool,
    captured: bool,
    title: String,
    /// Whether the machine takes the keyboard's keys; their presses and
    /// lettings go, for it; those held, to be let go.
    raw_keys: bool,
    /// Whether the machine's controllers are SNES pads.
    snes_pads: bool,
    key_changes: VecDeque<(Scancode, bool)>,
    raw_held: HashSet<Scancode>,
}

/// How long Select and Start are held together to go back to the library.
const BOTH_HELD: Duration = Duration::from_secs(1);
/// How long the pointer stays once it stops moving.
const POINTER_SHOWN: Duration = Duration::from_secs(2);

/// The sound queued ahead, to start with, so that a frame late doesn't run
/// it dry; and the most, beyond which a frame's is dropped, so that the
/// sound can't fall behind the picture.
const AUDIO_CUSHION: f32 = 0.05;
const AUDIO_MOST: f32 = 0.1;
/// The most the sound's stretched or squeezed, to keep the cushion: a
/// share of its speed, 2%, as the queue's empty or at its most. A
/// machine kept in step with a display's refresh a little slower or
/// quicker than its own makes its sound that much slower or quicker.
const MOST_STRETCH: f32 = 0.02;

/// A title bar's height, about: a window's frame isn't known till it's
/// open.
const TITLE_BAR: u32 = 40;

/// `place`, X, Y, width and height (its inside's), moved and made smaller
/// as it has to be to be inside `usable`, X, Y, width and height, the part
/// of the screen a taskbar's not over, its title bar above it inside too.
pub fn placed_inside(place: [i32; 4], usable: (i32, i32, u32, u32)) -> [i32; 4] {
    let (left, top, across, down) = usable;
    let top = top + TITLE_BAR as i32;
    let down = down.saturating_sub(TITLE_BAR) as i32;
    let width = place[2].min(across as i32);
    let height = place[3].min(down);
    let x = place[0].clamp(left, left + across as i32 - width);
    let y = place[1].clamp(top, top + down - height);
    [x, y, width, height]
}

/// `size`, made smaller, its shape kept, as it has to be to fit `usable`'s
/// width and height, a title bar's room left.
pub fn sized_to_fit(size: (u32, u32), usable: (i32, i32, u32, u32)) -> (u32, u32) {
    let (across, down) = (usable.2, usable.3.saturating_sub(TITLE_BAR));
    if size.0 <= across && size.1 <= down {
        return size;
    }
    let scale = (across as f64 / size.0 as f64).min(down as f64 / size.1 as f64);
    (
        (size.0 as f64 * scale) as u32,
        (size.1 as f64 * scale) as u32,
    )
}

impl Window {
    /// A window `title`d, for frames `width` by `height` pixels, shown at
    /// `size`, the window's width and height, its shape kept in
    /// fullscreen; `fullscreen` to start with.
    pub fn new(
        title: &str,
        width: usize,
        height: usize,
        size: (u32, u32),
        fullscreen: bool,
    ) -> Window {
        pads::prepare();
        let sdl = sdl2::init().expect("SDL");
        let video = sdl.video().expect("SDL's video");
        // On the screen it's to be on, in the part of it that's free, room
        // left for its title bar.
        let usable = |x: i32, y: i32| {
            let displays = video.num_video_displays().unwrap_or(1).max(1);
            let display = (0..displays)
                .find(|&n| {
                    video
                        .display_bounds(n)
                        .is_ok_and(|b| b.contains_point((x, y)))
                })
                .unwrap_or(0);
            video
                .display_usable_bounds(display)
                .map(|b| (b.x(), b.y(), b.width(), b.height()))
                .ok()
        };
        let window = match PLACE.get() {
            Some(&place) => {
                let [x, y, width, height] =
                    usable(place[0], place[1]).map_or(place, |usable| placed_inside(place, usable));
                video
                    .window(title, width as u32, height as u32)
                    .position(x, y)
                    .resizable()
                    .build()
            }
            None => {
                let (width, height) =
                    usable(0, 0).map_or(size, |usable| sized_to_fit(size, usable));
                video
                    .window(title, width, height)
                    .position_centered()
                    .build()
            }
        }
        .expect("a window");
        let mut canvas = window
            .into_canvas()
            .accelerated()
            .build()
            .expect("a canvas");
        // Opened by another program, it's not to end up behind that.
        if PLACE.get().is_some() {
            canvas.window_mut().raise();
        }
        canvas
            .set_logical_size(size.0, size.1)
            .expect("the picture's shape");
        // The picture's texture, borrowing what made it: that kept for good,
        // the one window being the process's, as long as it runs.
        let creator: &'static TextureCreator<sdl2::video::WindowContext> =
            Box::leak(Box::new(canvas.texture_creator()));
        let texture = creator
            .create_texture_streaming(PixelFormatEnum::RGB888, width as u32, height as u32)
            .expect("a texture");
        // Typed text, for the machines with keyboards. On Android, starting
        // it puts its keyboard on the screen, over the picture: there, text
        // events only let through, a real keyboard's typing still coming.
        #[cfg(not(target_os = "android"))]
        video.text_input().start();
        #[cfg(target_os = "android")]
        // SAFETY: SDL's video is initialised, on this thread.
        unsafe {
            sdl2::sys::SDL_EventState(
                sdl2::sys::SDL_EventType::SDL_TEXTINPUT as u32,
                sdl2::sys::SDL_ENABLE as i32,
            );
        }
        // The sound, if the host has any: a queue of samples, a cushion of
        // silence in it to start.
        let audio: Option<AudioQueue<f32>> = sdl.audio().ok().and_then(|audio| {
            let spec = AudioSpecDesired {
                freq: Some(SAMPLE_RATE as i32),
                channels: Some(CHANNELS as u8),
                samples: Some(512),
            };
            let queue = audio.open_queue::<f32, _>(None, &spec).ok()?;
            queue
                .queue_audio(&vec![
                    0.0;
                    (SAMPLE_RATE as f32 * AUDIO_CUSHION) as usize
                        * CHANNELS
                ])
                .ok()?;
            queue.resume();
            Some(queue)
        });
        let pads = Pads::new(sdl.game_controller().expect("SDL's game controllers"));
        let events = sdl.event_pump().expect("SDL's events");
        let mut window = Window {
            canvas,
            mouse: sdl.mouse(),
            clipboard: video.clipboard(),
            texture,
            events,
            keys: VecDeque::new(),
            function_keys: VecDeque::new(),
            audio,
            stretch: Stretch::default(),
            queued: AUDIO_CUSHION,
            played: Vec::new(),
            in_step: false,
            waited: 0.0,
            pads,
            keypad_held: HashSet::new(),
            alt_held: [false; 2],
            right_shift_held: false,
            reset_held: false,
            keys_held: HashSet::new(),
            closed: false,
            controls: Controls::load(),
            held: HashSet::new(),
            pressed: VecDeque::new(),
            capturing: false,
            both_held: None,
            keypad_text: None,
            pointer_moved: Instant::now(),
            picture: (width, height),
            logical: size,
            pointer: Pointer::default(),
            fingers: Fingers::new(),
            touching: false,
            aiming: false,
            cross_hairs: None,
            has_mouse: false,
            captured: false,
            title: title.to_string(),
            raw_keys: false,
            snes_pads: false,
            key_changes: VecDeque::new(),
            raw_held: HashSet::new(),
        };
        // On Android, always: SDL's activity then hides the system's bars,
        // and draws to the screen's edges, a camera's cutout too.
        if fullscreen || cfg!(target_os = "android") {
            window.toggle_fullscreen();
        }
        window
    }

    /// Whether a binding's held, for `player`: a key, anyone's; a gamepad's,
    /// their own.
    fn is_held(&self, player: usize, source: &str) -> bool {
        // The keyboard's all the machine's, if it takes its keys: but for
        // what's bound to a function key (the library's F5), behind
        // Ctrl+Shift, as Open Bus's other keys are then.
        if self.raw_keys && source.starts_with("key:") {
            let function = source
                .strip_prefix("key:F")
                .is_some_and(|n| n.parse::<u8>().is_ok());
            let either = |keys: [&str; 2]| keys.iter().any(|key| self.held.contains(*key));
            let chord = either(["key:Left Ctrl", "key:Right Ctrl"])
                && either(["key:Left Shift", "key:Right Shift"]);
            return function && chord && self.held.contains(source);
        }
        match source.strip_prefix("pad:") {
            Some(button) => self.pads.holds(player, button),
            None => self.held.contains(source),
        }
    }

    /// A player's gamepad, as what's held is bound.
    fn mapped(&self, player: usize) -> Gamepad {
        let held = |action| {
            self.controls
                .holds(player, action, |source| self.is_held(player, source))
        };
        let (a, b) = (held(Action::A), held(Action::B));
        let (turbo_a, turbo_b) = (held(Action::TurboA), held(Action::TurboB));
        // A NES pad's A and B, and turbo; or a SNES pad's four, by where
        // they are, as the pad's are bound: the bottom button (A's) its B,
        // the right (turbo A's) its A, the left (B's) its Y and the top
        // (turbo B's) its X; no turbo.
        let (a, b, x, y, turbo_a, turbo_b) = if self.snes_pads {
            (turbo_a, a, turbo_b, b, false, false)
        } else {
            (a, b, false, false, turbo_a, turbo_b)
        };
        Gamepad {
            up: held(Action::Up),
            down: held(Action::Down),
            left: held(Action::Left),
            right: held(Action::Right),
            a,
            b,
            select: held(Action::Select),
            start: held(Action::Start),
            x,
            y,
            l: held(Action::L),
            r: held(Action::R),
            turbo_a,
            turbo_b,
            // A TV remote's OK, always, whatever's bound.
            accept: held(Action::MenuAccept) || self.held.contains(REMOTE_OK),
            back: held(Action::MenuBack),
        }
    }

    /// Whether what's bound to fast-forward is held, not while it's looking
    /// for a press to bind.
    fn fast_held(&self) -> bool {
        self.held_for(Action::FastForward)
    }

    /// Whether what's bound to `action`, the first player's, is held, not
    /// while it's looking for a press to bind.
    fn held_for(&self, action: Action) -> bool {
        !self.capturing
            && self
                .controls
                .holds(0, action, |source| self.is_held(0, source))
    }

    /// Back to the library: by what's bound to it; or if the app opened
    /// this, Select and Start held together a moment.
    fn check_library(&mut self) {
        if self.capturing {
            self.both_held = None;
            return;
        }
        if self
            .controls
            .holds(0, Action::Library, |source| self.is_held(0, source))
        {
            self.closed = true;
        }
        let pad = self.mapped(0);
        if launched() && pad.select && pad.start {
            let since = *self.both_held.get_or_insert_with(Instant::now);
            self.closed |= since.elapsed() >= BOTH_HELD;
        } else {
            self.both_held = None;
        }
    }

    /// Fullscreen, at the screen's own size; or back to the window.
    fn toggle_fullscreen(&mut self) {
        // On Android, fullscreen it stays: windowed, a phone's bars are over
        // the picture.
        let fullscreen = cfg!(target_os = "android")
            || self.canvas.window().fullscreen_state() == FullscreenType::Off;
        let state = if fullscreen {
            FullscreenType::Desktop
        } else {
            FullscreenType::Off
        };
        let _ = self.canvas.window_mut().set_fullscreen(state);
    }

    /// The keys held for the machine let go, as the window's no longer the
    /// one typed in.
    fn let_go_keys(&mut self) {
        for key in self.raw_held.drain() {
            self.key_changes.push_back((key, false));
        }
    }

    /// Ctrl, Shift, Alt and the logo keys let go for the machine, as the
    /// clipboard's keys are pressed with them: they're the host's, and a
    /// paste into an X16 with Ctrl held would scroll as slowly as its
    /// KERNAL scrolls for Ctrl.
    fn let_go_modifiers(&mut self) {
        let modifiers: Vec<Scancode> = self
            .raw_held
            .iter()
            .copied()
            .filter(|key| (0xE0..=0xE7).contains(&key.0))
            .collect();
        for key in modifiers {
            self.raw_held.remove(&key);
            self.key_changes.push_back((key, false));
        }
    }

    /// The mouse captured for the machine, at `at` on the picture, or let
    /// go: its cursor gone, its moves told however far they go, and the
    /// title saying how it's let go.
    fn capture(&mut self, at: Option<(f32, f32)>) {
        self.captured = at.is_some();
        self.mouse.set_relative_mouse_mode(self.captured);
        self.pointer = Pointer {
            at,
            ..Pointer::default()
        };
        self.show_title();
    }

    /// How fingers on the screen are the machine's pointer, if they are:
    /// a trackpad, for its mouse; or where it's aimed.
    fn finger_way(&self) -> Option<Way> {
        if self.has_mouse {
            Some(Way::Trackpad)
        } else if self.aiming {
            Some(Way::Aimed)
        } else {
            None
        }
    }

    /// How far a finger's moved, `dx` and `dy` across and down the window,
    /// a share of it, in the picture's pixels as it's shown.
    fn finger_travel(&self, dx: f32, dy: f32) -> (f32, f32) {
        let (w, h) = self.canvas.output_size().unwrap_or(self.logical);
        let (w, h) = (w as f32, h as f32);
        // The window's pixels to each of the picture's, as it's scaled up.
        let shown = (w / self.logical.0 as f32).min(h / self.logical.1 as f32);
        let (across, down) = scale(self.picture, self.logical);
        (dx * w / shown * across, dy * h / shown * down)
    }

    /// The machine's title, and, the mouse captured, how it's let go.
    fn show_title(&mut self) {
        let title = if self.captured {
            format!("{} (the mouse captured: Ctrl+M lets it go)", self.title)
        } else {
            self.title.clone()
        };
        let _ = self.canvas.window_mut().set_title(&title);
    }

    /// The window's events since the last frame: its keys queued, the
    /// joystick's state kept; closed, kept for the machine to see.
    fn take_events(&mut self) {
        let mut toggle = false; // fullscreen, once the events are taken
        let events: Vec<Event> = self.events.poll_iter().collect();
        for event in events {
            if let Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } = event
            {
                self.let_go_keys();
                self.fingers.lift_all();
            }
            if is_mouse(&event) {
                self.touching = false;
            }
            match event {
                Event::Quit { .. } => self.closed = true,
                Event::TextInput { text, .. } => {
                    if self
                        .keypad_text
                        .take()
                        .is_some_and(|c| text == c.to_string())
                    {
                        continue;
                    }
                    self.keys
                        .extend(text.bytes().filter(u8::is_ascii).map(Key::Char));
                }
                Event::KeyDown {
                    keycode: Some(keycode),
                    scancode,
                    keymod,
                    repeat,
                    ..
                } => {
                    self.keypad_text = None;
                    if self.captured
                        && keycode == Keycode::M
                        && keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD)
                    {
                        self.capture(None);
                        continue;
                    }
                    // On a machine that takes the whole keyboard, F1 to F12
                    // are its own, Open Bus's behind Ctrl+Shift (Ctrl's,
                    // and Alt's, a desktop's); Android's Back is the menu,
                    // always.
                    let hosts = hosts_keys(self.raw_keys, keycode, keymod);
                    let fullscreen =
                        fullscreen_key(keycode, keymod) && (hosts || keycode != Keycode::F11);
                    let reset = keycode == Keycode::F12 && hosts;
                    let hosts_function = function_key(keycode).filter(|_| hosts);
                    let own = fullscreen
                        || reset
                        || hosts_function.is_some()
                        || clipboard_key(keycode, keymod).is_some();
                    if let (true, false, Some(scancode)) = (self.raw_keys, own, scancode) {
                        let key = Scancode(scancode as i32 as u16);
                        self.raw_held.insert(key);
                        self.key_changes.push_back((key, true));
                    }
                    let name = format!("key:{}", keycode.name());
                    if !repeat && self.capturing && !is_fixed(keycode) {
                        self.pressed.push_back(name.clone());
                    }
                    self.held.insert(name);
                    if !is_modifier(keycode)
                        && function_key(keycode).is_none()
                        && !fullscreen_key(keycode, keymod)
                        && keycode != Keycode::F12
                    {
                        self.keys_held.insert(keycode);
                    }
                    if let Some(text) = keypad(keycode) {
                        self.keypad_held.insert(keycode);
                        self.keypad_text = Some(text);
                        continue;
                    }
                    match keycode {
                        Keycode::LAlt => self.alt_held[0] = true,
                        Keycode::RAlt => self.alt_held[1] = true,
                        Keycode::RShift => self.right_shift_held = true,
                        Keycode::F12 if reset => self.reset_held = true,
                        _ => {}
                    }
                    // Fullscreen's key and the function keys, a press each,
                    // not repeating while held.
                    if fullscreen {
                        toggle ^= !repeat;
                        continue;
                    }
                    if let Some(n) = hosts_function {
                        if !repeat {
                            self.function_keys.push_back(n);
                        }
                        continue;
                    }
                    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
                    if let Some(n) = clipboard_key(keycode, keymod) {
                        if !repeat {
                            self.let_go_modifiers();
                            self.function_keys.push_back(n);
                        }
                        continue;
                    }
                    let key = match keycode {
                        Keycode::Return | Keycode::KpEnter => Some(Key::Char(13)),
                        Keycode::Backspace => Some(Key::Char(8)),
                        Keycode::Escape => Some(Key::Char(27)),
                        Keycode::Tab => Some(Key::Char(9)),
                        Keycode::Left => Some(Key::Left),
                        Keycode::Right => Some(Key::Right),
                        Keycode::Up => Some(Key::Up),
                        Keycode::Down => Some(Key::Down),
                        // Ctrl and a letter: its control character, typed
                        // as no text.
                        _ if ctrl => {
                            let name = keycode.name();
                            match name.as_bytes() {
                                [letter] if letter.is_ascii_alphabetic() => {
                                    Some(Key::Char(letter.to_ascii_uppercase() - b'@'))
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    self.keys.extend(key);
                }
                Event::KeyUp {
                    keycode: Some(keycode),
                    scancode,
                    ..
                } => {
                    if let Some(scancode) = scancode {
                        let key = Scancode(scancode as i32 as u16);
                        if self.raw_held.remove(&key) {
                            self.key_changes.push_back((key, false));
                        }
                    }
                    self.keypad_held.remove(&keycode);
                    self.keys_held.remove(&keycode);
                    self.held.remove(&format!("key:{}", keycode.name()));
                    match keycode {
                        Keycode::LAlt => self.alt_held[0] = false,
                        Keycode::RAlt => self.alt_held[1] = false,
                        Keycode::RShift => self.right_shift_held = false,
                        Keycode::F12 => self.reset_held = false,
                        _ => {}
                    }
                }
                // The clicks SDL makes of a finger: the fingers are taken
                // as they are, below.
                Event::MouseMotion {
                    which: TOUCH_MOUSE, ..
                }
                | Event::MouseButtonDown {
                    which: TOUCH_MOUSE, ..
                }
                | Event::MouseButtonUp {
                    which: TOUCH_MOUSE, ..
                } => {}
                Event::FingerDown {
                    touch_id,
                    finger_id,
                    x,
                    y,
                    ..
                } if touch_id != MOUSE_TOUCH => {
                    if let Some(way) = self.finger_way() {
                        if !self.touching && self.captured {
                            let at = self.pointer.at;
                            self.capture(None);
                            self.fingers.start_at(at);
                        }
                        self.touching = true;
                        let picture = (self.picture.0 as f32, self.picture.1 as f32);
                        let at = (x * picture.0, y * picture.1);
                        self.fingers
                            .down(way, finger_id, at, picture, clock::seconds());
                    }
                }
                Event::FingerMotion {
                    touch_id,
                    finger_id,
                    x,
                    y,
                    dx,
                    dy,
                    ..
                } if touch_id != MOUSE_TOUCH => {
                    if let Some(way) = self.finger_way() {
                        let at = (x * self.picture.0 as f32, y * self.picture.1 as f32);
                        let by = self.finger_travel(dx, dy);
                        self.fingers.moved(way, finger_id, at, by);
                    }
                }
                Event::FingerUp {
                    touch_id,
                    finger_id,
                    ..
                } if touch_id != MOUSE_TOUCH => {
                    if let Some(way) = self.finger_way() {
                        self.fingers.up(way, finger_id, clock::seconds());
                    }
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    clicks: 2,
                    ..
                } if !self.aiming && !self.has_mouse => toggle = !toggle,
                // Clicked on the picture, the mouse is captured, that click
                // not the machine's.
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } if self.has_mouse && !self.captured => {
                    if let Some(at) = on_picture(self.picture, self.logical, x, y) {
                        self.capture(Some(at));
                    }
                }
                Event::MouseMotion { xrel, yrel, .. } if self.captured => {
                    let (x, y) = self.pointer.at.unwrap_or_default();
                    let (across, down) = scale(self.picture, self.logical);
                    self.pointer.at = Some((x + xrel as f32 * across, y + yrel as f32 * down));
                }
                Event::MouseButtonDown { mouse_btn, .. } if self.captured => {
                    let at = self.pointer.at;
                    click(&mut self.pointer, mouse_btn, true, at)
                }
                Event::MouseButtonUp { mouse_btn, .. } if self.captured => {
                    let at = self.pointer.at;
                    click(&mut self.pointer, mouse_btn, false, at)
                }
                Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } if self.captured => self.capture(None),
                Event::MouseMotion { x, y, .. } => {
                    self.pointer_moved = Instant::now();
                    self.pointer.at = on_picture(self.picture, self.logical, x, y);
                }
                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } => click(
                    &mut self.pointer,
                    mouse_btn,
                    true,
                    on_picture(self.picture, self.logical, x, y),
                ),
                Event::MouseButtonUp {
                    mouse_btn, x, y, ..
                } => click(
                    &mut self.pointer,
                    mouse_btn,
                    false,
                    on_picture(self.picture, self.logical, x, y),
                ),
                Event::Window {
                    win_event: WindowEvent::Leave,
                    ..
                } if !self.captured => self.pointer.at = None,
                event => {
                    if let Some(source) = self.pads.take(&event).filter(|_| self.capturing) {
                        self.pressed.push_back(source);
                    }
                }
            }
        }
        self.check_library();
        if toggle {
            self.toggle_fullscreen();
        }
        // The pointer: gone in fullscreen, and in the window once it's
        // still; but always there to aim with.
        let windowed = self.canvas.window().fullscreen_state() == FullscreenType::Off;
        self.mouse.show_cursor(
            !self.captured
                && (self.aiming || windowed && self.pointer_moved.elapsed() < POINTER_SHOWN),
        );
    }
}

/// The mouse SDL makes of a finger, and the finger it makes of a mouse.
const TOUCH_MOUSE: u32 = u32::MAX;
const MOUSE_TOUCH: i64 = -1;

/// Whether `event` is a mouse's own, not a finger's as SDL has it.
fn is_mouse(event: &Event) -> bool {
    matches!(
        event,
        Event::MouseMotion { which, .. }
        | Event::MouseButtonDown { which, .. }
        | Event::MouseButtonUp { which, .. } if *which != TOUCH_MOUSE
    )
}

/// Where the mouse at `x`, `y`, in a window's `logical` size, is on its
/// `picture`, in the picture's pixels, if it's on it, not on the bars
/// beside it.
fn on_picture(picture: (usize, usize), logical: (u32, u32), x: i32, y: i32) -> Option<(f32, f32)> {
    let (across, down) = scale(picture, logical);
    let (x, y) = (x as f32 * across, y as f32 * down);
    let on = (0.0..picture.0 as f32).contains(&x) && (0.0..picture.1 as f32).contains(&y);
    on.then_some((x, y))
}

/// The clipboard's keys: Ctrl+Shift+V or Shift+Insert, pasting; and
/// Ctrl+Shift+C or Ctrl+Insert, copying.
fn clipboard_key(keycode: Keycode, keymod: Mod) -> Option<u8> {
    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
    let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
    match keycode {
        Keycode::V if ctrl && shift => Some(PASTE_KEY),
        Keycode::Insert if shift => Some(PASTE_KEY),
        Keycode::C if ctrl && shift => Some(COPY_KEY),
        Keycode::Insert if ctrl => Some(COPY_KEY),
        _ => None,
    }
}

/// The picture's pixels to a window's in its `logical` size, across and
/// down.
fn scale(picture: (usize, usize), logical: (u32, u32)) -> (f32, f32) {
    (
        picture.0 as f32 / logical.0 as f32,
        picture.1 as f32 / logical.1 as f32,
    )
}

/// A mouse `button` `pressed` or let go, the mouse `at` that on the
/// picture: the left the pointer's first button, the right its second.
fn click(pointer: &mut Pointer, button: MouseButton, pressed: bool, at: Option<(f32, f32)>) {
    let n = match button {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        _ => return,
    };
    pointer.buttons[n] = pressed;
    pointer.at = at;
}

/// Shift, Ctrl, Alt, the GUI keys, Caps Lock: not keys of their own.
fn is_modifier(keycode: Keycode) -> bool {
    matches!(
        keycode,
        Keycode::LShift
            | Keycode::RShift
            | Keycode::LCtrl
            | Keycode::RCtrl
            | Keycode::LAlt
            | Keycode::RAlt
            | Keycode::LGui
            | Keycode::RGui
            | Keycode::CapsLock
    )
}

/// Whether it's a key that's always what it is, not to be bound.
fn is_fixed(keycode: Keycode) -> bool {
    function_key(keycode).is_some() || matches!(keycode, Keycode::F11 | Keycode::F12)
}

/// Whether it's the key that switches fullscreen: F11; or on a Mac, where
/// F11 shows the desktop and never gets here, Ctrl+Cmd+F, as its own apps
/// have it.
fn fullscreen_key(keycode: Keycode, keymod: Mod) -> bool {
    keycode == Keycode::F11
        || cfg!(target_os = "macos")
            && keycode == Keycode::F
            && keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD)
            && keymod.intersects(Mod::LGUIMOD | Mod::RGUIMOD)
}

/// Whether a function key's Open Bus's (the menu's F1, the states', F11's
/// fullscreen, F12's Reset): always, unless the machine takes the whole
/// keyboard (`raw_keys`), whose F1 to F12 are then, Open Bus's behind
/// Ctrl+Shift (not Ctrl alone, a desktop's for its workspaces). Android's
/// Back, a TV remote's and a phone's, is the menu, always.
pub fn hosts_keys(raw_keys: bool, keycode: Keycode, keymod: Mod) -> bool {
    let chord = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD)
        && keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
    let back = cfg!(target_os = "android") && keycode == Keycode::AcBack;
    !raw_keys || chord || back
}

/// F1 to F10's numbers.
fn function_key(keycode: Keycode) -> Option<u8> {
    let keys = [
        Keycode::F1,
        Keycode::F2,
        Keycode::F3,
        Keycode::F4,
        Keycode::F5,
        Keycode::F6,
        Keycode::F7,
        Keycode::F8,
        Keycode::F9,
        Keycode::F10,
    ];
    // A TV's remote has Back, not F1: the menu, and out of it again.
    #[cfg(target_os = "android")]
    if keycode == Keycode::AcBack {
        return Some(1);
    }
    keys.iter()
        .position(|&key| key == keycode)
        .map(|n| n as u8 + 1)
}

/// A numeric keypad key the joystick takes, and the text it would type.
fn keypad(keycode: Keycode) -> Option<char> {
    Some(match keycode {
        Keycode::Kp0 => '0',
        Keycode::Kp1 => '1',
        Keycode::Kp2 => '2',
        Keycode::Kp3 => '3',
        Keycode::Kp4 => '4',
        Keycode::Kp6 => '6',
        Keycode::Kp7 => '7',
        Keycode::Kp8 => '8',
        Keycode::Kp9 => '9',
        Keycode::KpPeriod => '.',
        _ => return None,
    })
}

impl Display for Window {
    fn fast_forward(&self) -> bool {
        self.fast_held()
    }

    fn rewinding(&self) -> bool {
        self.held_for(Action::Rewind)
    }

    fn pointer(&self) -> Pointer {
        if self.touching {
            return self.fingers.pointer(clock::seconds());
        }
        // A machine with a mouse has it only as it's captured.
        if self.has_mouse && !self.captured {
            return Pointer::default();
        }
        self.pointer
    }

    fn raw_keys(&mut self, on: bool) {
        self.raw_keys = on;
        self.let_go_keys();
    }

    fn snes_pads(&mut self, on: bool) {
        self.snes_pads = on;
    }

    fn key_change(&mut self) -> Option<(Scancode, bool)> {
        self.key_changes.pop_front()
    }

    fn mouse(&mut self, on: bool) {
        self.has_mouse = on;
        if !on && self.captured {
            self.capture(None);
        }
    }

    fn aiming(&mut self, on: bool) {
        self.aiming = on;
        self.cross_hairs = on
            .then(|| Cursor::from_system(SystemCursor::Crosshair).ok())
            .flatten();
        if let Some(cross_hairs) = &self.cross_hairs {
            cross_hairs.set();
        }
    }

    fn show(&mut self, pixels: &[u32], width: usize, height: usize) {
        self.texture
            .with_lock(None, |buffer, pitch| {
                for (row, line) in buffer
                    .chunks_mut(pitch)
                    .zip(pixels.chunks(width).take(height))
                {
                    for (bytes, pixel) in row.as_chunks_mut::<4>().0.iter_mut().zip(line) {
                        *bytes = pixel.to_ne_bytes();
                    }
                }
            })
            .expect("the texture");
        self.canvas.clear();
        self.canvas
            .copy(&self.texture, None, None)
            .expect("the frame");
        if self.in_step {
            let waiting = Instant::now();
            self.canvas.present();
            self.waited += waiting.elapsed().as_secs_f64();
        } else {
            self.canvas.present();
        }
        self.take_events();
    }

    fn can_close(&self) -> bool {
        true
    }

    fn close(&mut self) {
        self.closed = true;
    }

    fn no_picture(&mut self) {
        self.take_events();
    }

    fn key(&mut self) -> Option<Key> {
        self.keys.pop_front()
    }

    fn play(&mut self, samples: &[f32]) {
        let Some(queue) = &self.audio else {
            return;
        };
        let queued = queue.size() as f32 / (4.0 * CHANNELS as f32 * SAMPLE_RATE as f32);
        if queued >= AUDIO_MOST {
            return;
        }
        // Squeezed while there's more than the cushion, stretched while
        // there's less: by how much lately, the queue going by in the
        // device's blocks.
        self.queued += (queued - self.queued) * 0.1;
        let off = (self.queued - AUDIO_CUSHION) / AUDIO_CUSHION;
        let step = 1.0 + (off * MOST_STRETCH).clamp(-MOST_STRETCH, MOST_STRETCH);
        self.played.clear();
        self.stretch.run(samples, step as f64, &mut self.played);
        let _ = queue.queue_audio(&self.played);
    }

    fn refresh_rate(&self) -> Option<f64> {
        let window = self.canvas.window();
        let display = window.display_index().ok()?;
        let mode = window.subsystem().current_display_mode(display).ok()?;
        (mode.refresh_rate > 0).then_some(mode.refresh_rate as f64)
    }

    fn waited(&mut self) -> f64 {
        std::mem::take(&mut self.waited)
    }

    fn keep_in_step(&mut self, on: bool) {
        self.in_step = on;
        // SAFETY: the renderer's SDL's own, live as long as the canvas.
        unsafe {
            sdl2::sys::SDL_RenderSetVSync(self.canvas.raw(), on as i32);
        }
    }

    /// The gamepad's stick, unless the D-pad or the keypad pushes it all the
    /// way; their buttons, and the Alt keys'.
    fn joystick(&self) -> Joystick {
        let first = self.pads.first();
        let pad = |button: Button| first.is_some_and(|pad| pad.held.contains(&button.string()));
        let stick = first.map_or((0.0, 0.0), |pad| pad.stick);
        let keys = |held: &[Keycode]| held.iter().any(|key| self.keypad_held.contains(key));
        let left = pad(Button::DPadLeft) || keys(&[Keycode::Kp4, Keycode::Kp7, Keycode::Kp1]);
        let right = pad(Button::DPadRight) || keys(&[Keycode::Kp6, Keycode::Kp9, Keycode::Kp3]);
        let up = pad(Button::DPadUp) || keys(&[Keycode::Kp8, Keycode::Kp7, Keycode::Kp9]);
        let down = pad(Button::DPadDown) || keys(&[Keycode::Kp2, Keycode::Kp1, Keycode::Kp3]);
        let axis = |minus: bool, plus: bool, stick: f32| match (minus, plus) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => stick,
        };
        // A touch screen's stick, while it's held, and its two buttons.
        let (x, y) =
            touch::stick().unwrap_or((axis(left, right, stick.0), axis(up, down, stick.1)));
        let touched = touch::held();
        Joystick {
            x,
            y,
            buttons: [
                pad(Button::A)
                    || keys(&[Keycode::Kp0])
                    || self.alt_held[0]
                    || touched & touch::A != 0,
                pad(Button::B)
                    || keys(&[Keycode::KpPeriod])
                    || self.alt_held[1]
                    || touched & touch::B != 0,
                false,
            ],
        }
    }

    /// The first gamepad's, or the
    /// keyboard's.
    fn held_still(&self) -> bool {
        touch::still()
    }

    fn gamepad(&self) -> Gamepad {
        // With a touch screen's buttons, on Android, as it has them.
        touch::onto(self.mapped(0))
    }

    fn menu_pad(&self) -> Gamepad {
        self.mapped(0)
    }

    fn gamepad_2(&self) -> Gamepad {
        self.mapped(1)
    }

    fn pressed(&mut self) -> Option<String> {
        self.pressed.pop_front()
    }

    fn capture(&mut self, on: bool) {
        self.capturing = on;
        self.pressed.clear();
    }

    fn controls(&mut self) -> Option<&mut Controls> {
        Some(&mut self.controls)
    }

    fn said(&self, source: &str) -> String {
        let face = source
            .strip_prefix("pad:")
            .and_then(|button| self.pads.label(button));
        face.map_or_else(|| said(source), |label| format!("PAD {label}"))
    }

    fn hint(&self) -> Option<&'static str> {
        Some(if launched() {
            "SELECT + START: MENU, HELD: BACK"
        } else if self.raw_keys {
            "CTRL+SHIFT+F1 OR SELECT + START: MENU"
        } else {
            "F1 OR SELECT + START: MENU"
        })
    }

    fn reset_held(&self) -> bool {
        self.reset_held
    }

    fn key_down(&self) -> bool {
        !self.keys_held.is_empty()
    }

    fn closed(&self) -> bool {
        self.closed
    }

    fn function_key(&mut self) -> Option<u8> {
        self.function_keys.pop_front()
    }

    fn title(&mut self, title: &str) {
        self.title = title.to_string();
        self.show_title();
    }

    fn clipboard(&self) -> bool {
        true
    }

    fn clipboard_text(&mut self) -> Option<String> {
        self.clipboard.clipboard_text().ok()
    }

    fn copy_text(&mut self, text: &str) {
        let _ = self.clipboard.set_clipboard_text(text);
    }
}
