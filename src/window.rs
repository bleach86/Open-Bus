//! The host's window, by SDL2: a machine's frames, scaled up; the keys
//! typed in it; and a joystick, a gamepad's (its left stick or D-pad, A and
//! B its buttons) or the numeric keypad's (8, 2, 4 and 6 and the corners;
//! 0 and . its buttons), the Alt keys buttons too. Closing it turns the
//! machine off.

use crate::display::{Display, Joystick, Key};
use sdl2::EventPump;
use sdl2::controller::{Axis, Button, GameController};
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture};
use sdl2::GameControllerSubsystem;
use std::collections::{HashSet, VecDeque};

pub struct Window {
    canvas: Canvas<sdl2::video::Window>,
    texture: Texture,
    events: EventPump,
    keys: VecDeque<Key>,
    controllers: GameControllerSubsystem,
    gamepad: Option<GameController>,
    /// The gamepad's left stick, -1 to 1 each way.
    stick: (f32, f32),
    /// The gamepad's buttons held, the keypad's keys and the Alt keys.
    pad_held: HashSet<Button>,
    keypad_held: HashSet<Keycode>,
    alt_held: [bool; 2],
    /// A keypad key's own text, which it would type too: not typed.
    keypad_text: Option<char>,
}

/// A stick's travel taken as centred: a resting gamepad's isn't quite 0.
const DEAD_ZONE: f32 = 0.15;

impl Window {
    /// A window `title`d, for frames `width` by `height` pixels, shown
    /// `scale` times their size.
    pub fn new(title: &str, width: usize, height: usize, scale: u32) -> Window {
        let sdl = sdl2::init().expect("SDL");
        let video = sdl.video().expect("SDL's video");
        let window = video
            .window(title, width as u32 * scale, height as u32 * scale)
            .position_centered()
            .build()
            .expect("a window");
        let canvas = window.into_canvas().accelerated().build().expect("a canvas");
        let texture = canvas
            .texture_creator()
            .create_texture_streaming(PixelFormatEnum::RGB888, width as u32, height as u32)
            .expect("a texture");
        video.text_input().start();
        let controllers = sdl.game_controller().expect("SDL's game controllers");
        let gamepad = (0..controllers.num_joysticks().unwrap_or(0))
            .filter(|&n| controllers.is_game_controller(n))
            .find_map(|n| controllers.open(n).ok());
        let events = sdl.event_pump().expect("SDL's events");
        Window {
            canvas,
            texture,
            events,
            keys: VecDeque::new(),
            controllers,
            gamepad,
            stick: (0.0, 0.0),
            pad_held: HashSet::new(),
            keypad_held: HashSet::new(),
            alt_held: [false; 2],
            keypad_text: None,
        }
    }

    /// The window's events since the last frame: its keys queued, the
    /// joystick's state kept; closed, the machine off.
    fn take_events(&mut self) {
        for event in self.events.poll_iter() {
            match event {
                Event::Quit { .. } => std::process::exit(0),
                Event::TextInput { text, .. } => {
                    if self.keypad_text.take().is_some_and(|c| text == c.to_string()) {
                        continue;
                    }
                    self.keys.extend(text.bytes().filter(u8::is_ascii).map(Key::Char));
                }
                Event::KeyDown { keycode: Some(keycode), keymod, .. } => {
                    self.keypad_text = None;
                    if let Some(text) = keypad(keycode) {
                        self.keypad_held.insert(keycode);
                        self.keypad_text = Some(text);
                        continue;
                    }
                    match keycode {
                        Keycode::LAlt => self.alt_held[0] = true,
                        Keycode::RAlt => self.alt_held[1] = true,
                        _ => {}
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
                    match keycode {
                        Keycode::LAlt => self.alt_held[0] = false,
                        Keycode::RAlt => self.alt_held[1] = false,
                        _ => {}
                    }
                }
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
    }
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
}
