//! The host's window, by SDL2: a machine's frames, scaled up; the keys
//! typed in it; and a joystick, a gamepad's (its left stick or D-pad, A and
//! B its buttons) or the numeric keypad's (8, 2, 4 and 6 and the corners;
//! 0 and . its buttons), the Alt keys buttons too; a console's gamepads,
//! the host's first (its D-pad or left stick; its south button A, its
//! west B; Back and Start) or the keyboard's (the arrows; X and Z, A and
//! B; Right Shift or Tab, Select; Return, Start), and the host's second,
//! each keeping its place if the other's taken out; F1 to F10, for the
//! machine's own uses, and F12, its Reset key; and the machine's sound, in
//! stereo, through an audio queue. F11 or a double click switches it to
//! fullscreen and back, the picture its shape still, bars at its sides if
//! the screen is wider. The mouse on the picture, for a machine that's
//! aimed at, its pointer cross hairs then, and its clicks the machine's.
//! Closed, it tells the machine, to turn itself off.

use crate::host::controls::{Action, Controls};
use crate::host::display::{CHANNELS, Display, Gamepad, Joystick, Key, Pointer, SAMPLE_RATE};
use crate::host::pads::{self, Pads};
use sdl2::EventPump;
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::controller::Button;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::{Cursor, MouseButton, MouseUtil, SystemCursor};
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture};
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
    texture: Texture,
    events: EventPump,
    keys: VecDeque<Key>,
    /// F1 to F10 pressed, by number, not yet taken.
    function_keys: VecDeque<u8>,
    audio: Option<AudioQueue<f32>>,
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
    aiming: bool,
    cross_hairs: Option<Cursor>,
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
        let window = match PLACE.get() {
            Some(&[x, y, width, height]) => video
                .window(title, width as u32, height as u32)
                .position(x, y)
                .resizable()
                .build(),
            None => video
                .window(title, size.0, size.1)
                .position_centered()
                .build(),
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
        let texture = canvas
            .texture_creator()
            .create_texture_streaming(PixelFormatEnum::RGB888, width as u32, height as u32)
            .expect("a texture");
        video.text_input().start();
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
            texture,
            events,
            keys: VecDeque::new(),
            function_keys: VecDeque::new(),
            audio,
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
            aiming: false,
            cross_hairs: None,
        };
        if fullscreen {
            window.toggle_fullscreen();
        }
        window
    }

    /// Whether a binding's held, for `player`: a key, anyone's; a gamepad's,
    /// their own.
    fn is_held(&self, player: usize, source: &str) -> bool {
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
        Gamepad {
            up: held(Action::Up),
            down: held(Action::Down),
            left: held(Action::Left),
            right: held(Action::Right),
            a: held(Action::A),
            b: held(Action::B),
            select: held(Action::Select),
            start: held(Action::Start),
            turbo_a: held(Action::TurboA),
            turbo_b: held(Action::TurboB),
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
        let fullscreen = self.canvas.window().fullscreen_state() == FullscreenType::Off;
        let state = if fullscreen {
            FullscreenType::Desktop
        } else {
            FullscreenType::Off
        };
        let _ = self.canvas.window_mut().set_fullscreen(state);
    }

    /// The window's events since the last frame: its keys queued, the
    /// joystick's state kept; closed, kept for the machine to see.
    fn take_events(&mut self) {
        let mut toggle = false; // fullscreen, once the events are taken
        for event in self.events.poll_iter() {
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
                    keymod,
                    repeat,
                    ..
                } => {
                    self.keypad_text = None;
                    let name = format!("key:{}", keycode.name());
                    if !repeat && self.capturing && !is_fixed(keycode) {
                        self.pressed.push_back(name.clone());
                    }
                    self.held.insert(name);
                    if !is_modifier(keycode)
                        && function_key(keycode).is_none()
                        && !matches!(keycode, Keycode::F11 | Keycode::F12)
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
                        Keycode::F12 => self.reset_held = true,
                        _ => {}
                    }
                    // F11 and the function keys, a press each, not repeating
                    // while held.
                    if keycode == Keycode::F11 {
                        toggle ^= !repeat;
                        continue;
                    }
                    if let Some(n) = function_key(keycode) {
                        if !repeat {
                            self.function_keys.push_back(n);
                        }
                        continue;
                    }
                    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
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
                    ..
                } => {
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
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    clicks: 2,
                    ..
                } if !self.aiming => toggle = !toggle,
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
                } => self.pointer.at = None,
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
        self.mouse
            .show_cursor(self.aiming || windowed && self.pointer_moved.elapsed() < POINTER_SHOWN);
    }
}

/// Where the mouse at `x`, `y`, in a window's `logical` size, is on its
/// `picture`, in the picture's pixels, if it's on it, not on the bars
/// beside it.
fn on_picture(picture: (usize, usize), logical: (u32, u32), x: i32, y: i32) -> Option<(f32, f32)> {
    let x = x as f32 * picture.0 as f32 / logical.0 as f32;
    let y = y as f32 * picture.1 as f32 / logical.1 as f32;
    let on = (0.0..picture.0 as f32).contains(&x) && (0.0..picture.1 as f32).contains(&y);
    on.then_some((x, y))
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

/// F1 to F10's numbers.
/// Whether it's a key that's always what it is, not to be bound.
fn is_fixed(keycode: Keycode) -> bool {
    function_key(keycode).is_some() || matches!(keycode, Keycode::F11 | Keycode::F12)
}

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
        self.pointer
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
                for y in 0..height {
                    for x in 0..width {
                        let at = y * pitch + x * 4;
                        buffer[at..at + 4].copy_from_slice(&pixels[y * width + x].to_ne_bytes());
                    }
                }
            })
            .expect("the texture");
        self.canvas.clear();
        self.canvas
            .copy(&self.texture, None, None)
            .expect("the frame");
        self.canvas.present();
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
        if let Some(queue) = &self.audio {
            let queued = queue.size() as f32 / (4.0 * CHANNELS as f32 * SAMPLE_RATE as f32);
            if queued < AUDIO_MOST {
                let _ = queue.queue_audio(samples);
            }
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
        Joystick {
            x: axis(left, right, stick.0),
            y: axis(up, down, stick.1),
            buttons: [
                pad(Button::A) || keys(&[Keycode::Kp0]) || self.alt_held[0],
                pad(Button::B) || keys(&[Keycode::KpPeriod]) || self.alt_held[1],
                false,
            ],
        }
    }

    /// The first gamepad's, or the
    /// keyboard's.
    fn gamepad(&self) -> Gamepad {
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

    fn hint(&self) -> Option<&'static str> {
        Some(if launched() {
            "SELECT + START: MENU, HELD: BACK"
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
        let _ = self.canvas.window_mut().set_title(title);
    }
}
