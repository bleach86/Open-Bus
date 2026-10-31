//! The host's window, by SDL2: a machine's frames, scaled up, and the keys
//! typed in it. Closing it turns the machine off.

use crate::display::{Display, Key};
use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture};
use std::collections::VecDeque;

pub struct Window {
    canvas: Canvas<sdl2::video::Window>,
    texture: Texture,
    events: EventPump,
    keys: VecDeque<Key>,
}

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
        let events = sdl.event_pump().expect("SDL's events");
        Window { canvas, texture, events, keys: VecDeque::new() }
    }

    /// The window's events since the last frame: its keys queued; closed,
    /// the machine off.
    fn take_events(&mut self) {
        for event in self.events.poll_iter() {
            match event {
                Event::Quit { .. } => std::process::exit(0),
                Event::TextInput { text, .. } => {
                    self.keys.extend(text.bytes().filter(u8::is_ascii).map(Key::Char));
                }
                Event::KeyDown { keycode: Some(keycode), keymod, .. } => {
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
                _ => {}
            }
        }
    }
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
}
