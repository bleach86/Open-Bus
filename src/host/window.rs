//! The host's window, by SDL2: a machine's frames, scaled up; the keys
//! typed in it; and a joystick, a gamepad's (its left stick or D-pad, A and
//! B its buttons) or the numeric keypad's (8, 2, 4 and 6 and the corners;
//! 0 and . its buttons), the Alt keys buttons too; a console's gamepad,
//! the host's gamepad (its D-pad or left stick; its east button A, its
//! south B; Back and Start) or the keyboard's (the arrows; X and Z, A and
//! B; Right Shift or Tab, Select; Return, Start); F1 to F10, for the
//! machine's own uses, and F12, its Reset key; and the machine's sound, in
//! stereo, through an audio queue. F11 or a double click switches it to
//! fullscreen and back, the picture its shape still, bars at its sides if
//! the screen is wider. Closed, it tells the machine, to turn itself off.

use crate::host::display::{Display, Gamepad, Joystick, Key, CHANNELS, SAMPLE_RATE};
use sdl2::EventPump;
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::controller::{Axis, Button, GameController};
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::{MouseButton, MouseUtil};
use sdl2::video::FullscreenType;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture};
use sdl2::GameControllerSubsystem;
use std::collections::{HashSet, VecDeque};

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
    controllers: GameControllerSubsystem,
    gamepad: Option<GameController>,
    /// The gamepad's left stick, -1 to 1 each way.
    stick: (f32, f32),
    /// The gamepad's buttons held, the keypad's keys and the Alt keys.
    pad_held: HashSet<Button>,
    keypad_held: HashSet<Keycode>,
    alt_held: [bool; 2],
    right_shift_held: bool,
    /// F12, the Reset key; and the other keys held, but the modifiers and
    /// the function keys.
    reset_held: bool,
    keys_held: HashSet<Keycode>,
    /// The window closed.
    closed: bool,
    /// A keypad key's own text, which it would type too: not typed.
    keypad_text: Option<char>,
}

/// A stick's travel taken as centred: a resting gamepad's isn't quite 0.
const DEAD_ZONE: f32 = 0.15;

/// The sound queued ahead, to start with, so that a frame late doesn't run
/// it dry; and the most, beyond which a frame's is dropped, so that the
/// sound can't fall behind the picture.
const AUDIO_CUSHION: f32 = 0.05;
const AUDIO_MOST: f32 = 0.1;

impl Window {
    /// A window `title`d, for frames `width` by `height` pixels, shown at
    /// `size`, the window's width and height, its shape kept in
    /// fullscreen; `fullscreen` to start with.
    pub fn new(title: &str, width: usize, height: usize, size: (u32, u32), fullscreen: bool) -> Window {
        let sdl = sdl2::init().expect("SDL");
        let video = sdl.video().expect("SDL's video");
        let window = video
            .window(title, size.0, size.1)
            .position_centered()
            .build()
            .expect("a window");
        let mut canvas = window.into_canvas().accelerated().build().expect("a canvas");
        canvas.set_logical_size(size.0, size.1).expect("the picture's shape");
        let texture = canvas
            .texture_creator()
            .create_texture_streaming(PixelFormatEnum::RGB888, width as u32, height as u32)
            .expect("a texture");
        video.text_input().start();
        // The sound, if the host has any: a queue of samples, a cushion of
        // silence in it to start.
        let audio = sdl.audio().ok().and_then(|audio| {
            let spec = AudioSpecDesired { freq: Some(SAMPLE_RATE as i32), channels: Some(CHANNELS as u8), samples: Some(512) };
            let queue = audio.open_queue::<f32, _>(None, &spec).ok()?;
            queue.queue_audio(&vec![0.0; (SAMPLE_RATE as f32 * AUDIO_CUSHION) as usize * CHANNELS]).ok()?;
            queue.resume();
            Some(queue)
        });
        let controllers = sdl.game_controller().expect("SDL's game controllers");
        let gamepad = (0..controllers.num_joysticks().unwrap_or(0))
            .filter(|&n| controllers.is_game_controller(n))
            .find_map(|n| controllers.open(n).ok());
        let events = sdl.event_pump().expect("SDL's events");
        let mut window = Window {
            canvas,
            mouse: sdl.mouse(),
            texture,
            events,
            keys: VecDeque::new(),
            function_keys: VecDeque::new(),
            audio,
            controllers,
            gamepad,
            stick: (0.0, 0.0),
            pad_held: HashSet::new(),
            keypad_held: HashSet::new(),
            alt_held: [false; 2],
            right_shift_held: false,
            reset_held: false,
            keys_held: HashSet::new(),
            closed: false,
            keypad_text: None,
        };
        if fullscreen {
            window.toggle_fullscreen();
        }
        window
    }

    /// Fullscreen, at the screen's own size, the pointer hidden; or back to
    /// the window.
    fn toggle_fullscreen(&mut self) {
        let fullscreen = self.canvas.window().fullscreen_state() == FullscreenType::Off;
        let state = if fullscreen { FullscreenType::Desktop } else { FullscreenType::Off };
        let _ = self.canvas.window_mut().set_fullscreen(state);
        self.mouse.show_cursor(!fullscreen);
    }

    /// The window's events since the last frame: its keys queued, the
    /// joystick's state kept; closed, kept for the machine to see.
    fn take_events(&mut self) {
        let mut toggle = false; // fullscreen, once the events are taken
        for event in self.events.poll_iter() {
            match event {
                Event::Quit { .. } => self.closed = true,
                Event::TextInput { text, .. } => {
                    if self.keypad_text.take().is_some_and(|c| text == c.to_string()) {
                        continue;
                    }
                    self.keys.extend(text.bytes().filter(u8::is_ascii).map(Key::Char));
                }
                Event::KeyDown { keycode: Some(keycode), keymod, repeat, .. } => {
                    self.keypad_text = None;
                    if !is_modifier(keycode) && function_key(keycode).is_none() && !matches!(keycode, Keycode::F11 | Keycode::F12) {
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
                Event::KeyUp { keycode: Some(keycode), .. } => {
                    self.keypad_held.remove(&keycode);
                    self.keys_held.remove(&keycode);
                    match keycode {
                        Keycode::LAlt => self.alt_held[0] = false,
                        Keycode::RAlt => self.alt_held[1] = false,
                        Keycode::RShift => self.right_shift_held = false,
                        Keycode::F12 => self.reset_held = false,
                        _ => {}
                    }
                }
                Event::MouseButtonDown { mouse_btn: MouseButton::Left, clicks: 2, .. } => toggle = !toggle,
                Event::ControllerDeviceAdded { which, .. } if self.gamepad.is_none() => {
                    self.gamepad = self.controllers.open(which).ok();
                }
                Event::ControllerDeviceRemoved { .. } => {
                    self.gamepad = None;
                    self.stick = (0.0, 0.0);
                    self.pad_held.clear();
                }
                Event::ControllerAxisMotion { axis, value, .. } => {
                    let travel = value as f32 / 32767.0;
                    let travel = if travel.abs() < DEAD_ZONE { 0.0 } else { travel.clamp(-1.0, 1.0) };
                    match axis {
                        Axis::LeftX => self.stick.0 = travel,
                        Axis::LeftY => self.stick.1 = travel,
                        _ => {}
                    }
                }
                Event::ControllerButtonDown { button, .. } => {
                    self.pad_held.insert(button);
                }
                Event::ControllerButtonUp { button, .. } => {
                    self.pad_held.remove(&button);
                }
                _ => {}
            }
        }
        if toggle {
            self.toggle_fullscreen();
        }
    }
}

/// Shift, Ctrl, Alt, the GUI keys, Caps Lock: not keys of their own.
fn is_modifier(keycode: Keycode) -> bool {
    matches!(
        keycode,
        Keycode::LShift | Keycode::RShift | Keycode::LCtrl | Keycode::RCtrl | Keycode::LAlt | Keycode::RAlt
            | Keycode::LGui | Keycode::RGui | Keycode::CapsLock
    )
}

/// F1 to F10's numbers.
fn function_key(keycode: Keycode) -> Option<u8> {
    let keys = [
        Keycode::F1, Keycode::F2, Keycode::F3, Keycode::F4, Keycode::F5,
        Keycode::F6, Keycode::F7, Keycode::F8, Keycode::F9, Keycode::F10,
    ];
    keys.iter().position(|&key| key == keycode).map(|n| n as u8 + 1)
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
        self.canvas.copy(&self.texture, None, None).expect("the frame");
        self.canvas.present();
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
        let pad = |button| self.pad_held.contains(&button);
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
            x: axis(left, right, self.stick.0),
            y: axis(up, down, self.stick.1),
            buttons: [
                pad(Button::A) || keys(&[Keycode::Kp0]) || self.alt_held[0],
                pad(Button::B) || keys(&[Keycode::KpPeriod]) || self.alt_held[1],
                false,
            ],
        }
    }

    /// The gamepad's D-pad, or its stick pushed well over; or the
    /// keyboard's.
    fn gamepad(&self) -> Gamepad {
        let pad = |button| self.pad_held.contains(&button);
        let key = |keycode| self.keys_held.contains(&keycode);
        let pushed = 0.5;
        Gamepad {
            up: pad(Button::DPadUp) || self.stick.1 < -pushed || key(Keycode::Up),
            down: pad(Button::DPadDown) || self.stick.1 > pushed || key(Keycode::Down),
            left: pad(Button::DPadLeft) || self.stick.0 < -pushed || key(Keycode::Left),
            right: pad(Button::DPadRight) || self.stick.0 > pushed || key(Keycode::Right),
            a: pad(Button::B) || key(Keycode::X),
            b: pad(Button::A) || key(Keycode::Z),
            select: pad(Button::Back) || self.right_shift_held || key(Keycode::Tab),
            start: pad(Button::Start) || key(Keycode::Return),
        }
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
