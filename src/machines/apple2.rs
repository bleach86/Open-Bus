//! The Apple II (the II+): a 6502 at 1.023 MHz; 48K of RAM; the keyboard,
//! the speaker, the soft switches and the joystick at $C000; a Disk II card in slot 6,
//! if there is one; and 12K of ROM at $D000, the monitor and BASIC, from a
//! dump of the machine's own (not part of this repository). Its picture is drawn a line at a time, as the
//! video circuit draws it, and shown in a window once a frame; or its text
//! screen alone in the host's terminal.

pub mod disk2;
pub mod disk_image;
pub mod video;

#[cfg(test)]
mod tests {
    mod disk2;
    mod machine;
}

use crate::cpu::CPU;
use crate::cpu::bus::Bus;
use crate::host::display::{Display, Joystick, Key, SAMPLE_RATE};
use crate::host::terminal::Terminal;
use crate::host::window::Window;
use std::io::Write;
use std::path::Path;

/// 14.31818 MHz, a fourteenth.
pub const CLOCK_HZ: u32 = 1_022_727;
/// 65 cycles a line, 262 lines: 192 seen, the rest the beam's way back.
pub const CYCLES_PER_LINE: u32 = 65;
pub const CYCLES_PER_FRAME: u32 = 17_030;
/// A paddle's timer runs 11 cycles a step of its 0 to 255: as long as the
/// monitor's PREAD takes to count one.
const CYCLES_PER_PADDLE_STEP: u64 = 11;

/// How loud the speaker is, of the host's full volume.
const VOLUME: f32 = 0.3;
/// How fast the speaker's cone settles back, left still: a sample's share.
const SETTLE: f32 = 0.995;

/// The flashing characters' half period.
const FLASH_FRAMES: u32 = 16;

pub struct Apple2 {
    pub ram: Vec<u8>,
    pub rom: Vec<u8>,
    /// The keyboard: the last key, bit 7 set until $C010 clears it.
    pub keyboard: u8,
    pub terminal: Option<Terminal>,
    /// The Disk II card, in slot 6.
    pub disk2: Option<disk2::Disk2>,
    /// The window the picture is shown in, and the keys come from.
    pub display: Option<Box<dyn Display>>,
    /// The picture, as far as this frame has drawn it.
    pub picture: Vec<u32>,
    // The soft switches.
    pub graphics: bool,
    pub mixed: bool,
    pub page2: bool,
    pub hires: bool,
    /// The speaker's cone: out, or in. $C030 flips it.
    speaker: bool,
    /// The cycles of the sample being made, and how many the cone was out;
    /// how far into a sample it is, in parts of CLOCK_HZ; the speaker's
    /// settling; and the samples made this frame.
    sample_cycles: u32,
    sample_out: u32,
    sample_phase: u32,
    settle: (f32, f32),
    sound: Vec<f32>,
    /// When each paddle's timer runs out, started by $C070.
    paddle_until: [u64; 4],
    /// Cycles since power-on; cycles into the frame, and frames.
    now: u64,
    cycle: u32,
    pub frames: u32,
    /// The screen as last drawn in the terminal.
    shown: String,
}

impl Apple2 {
    /// The machine with `rom`, 12K for $D000 to $FFFF.
    pub fn new(rom: Vec<u8>) -> Self {
        assert_eq!(rom.len(), 0x3000, "the ROM is 12K, $D000 to $FFFF");
        Apple2 {
            ram: vec![0; 0xC000],
            rom,
            keyboard: 0,
            terminal: None,
            disk2: None,
            display: None,
            picture: vec![0; video::WIDTH * video::HEIGHT],
            graphics: false,
            mixed: false,
            page2: false,
            hires: false,
            speaker: false,
            sample_cycles: 0,
            sample_out: 0,
            sample_phase: 0,
            settle: (-1.0, 0.0), // the cone in, at rest
            sound: Vec::new(),
            paddle_until: [0; 4],
            now: 0,
            cycle: 0,
            frames: 0,
            shown: String::new(),
        }
    }

    /// A key pressed: the keyboard's latch, as the key's ASCII with bit 7
    /// set, the last key over any not yet taken.
    pub fn press(&mut self, key: u8) {
        self.keyboard = key | 0x80;
    }

    /// The next key from the terminal, as the Apple's keyboard has it:
    /// Return for Enter, ← for Backspace and the left arrow, → for the
    /// right, upper case letters; another escape sequence, nothing.
    fn terminal_key(&mut self) -> Option<u8> {
        let terminal = self.terminal.as_ref()?;
        let key = terminal.key()?;
        Some(match key {
            b'\n' | b'\r' => 0x0D,
            0x7F => 0x08,
            b'a'..=b'z' => key - 0x20,
            0x1B => match terminal.key() {
                Some(b'[') | Some(b'O') => match terminal.key()? {
                    b'D' => 0x08,
                    b'C' => 0x15,
                    _ => return None,
                },
                Some(_) => return None,
                None => 0x1B, // Esc on its own
            },
            _ => key,
        })
    }

    /// The next key from the window, as the Apple's keyboard has it: ← for
    /// Backspace and the left arrow, → for the right, upper case letters;
    /// up and down, which it hasn't, nothing.
    fn display_key(&mut self) -> Option<u8> {
        Some(match self.display.as_mut()?.key()? {
            Key::Char(b'\n') => 0x0D,
            Key::Char(0x7F) => 0x08,
            Key::Char(c @ b'a'..=b'z') => c - 0x20,
            Key::Char(c) if c < 0x80 => c,
            Key::Left => 0x08,
            Key::Right => 0x15,
            _ => return None,
        })
    }

    /// The joystick, from the window: centred, without one.
    fn joystick(&self) -> Joystick {
        self.display.as_ref().map_or(Joystick::CENTRED, |display| display.joystick())
    }

    /// The paddles' timers started, at $C070: each to run as long as its
    /// position, 0 to 255; the stick's two across and down, and the other
    /// two, with nothing on them, as far as they go.
    fn start_paddles(&mut self) {
        let joystick = self.joystick();
        let position = |axis: f32| ((axis.clamp(-1.0, 1.0) + 1.0) * 127.5).round() as u64;
        let positions = [position(joystick.x), position(joystick.y), 255, 255];
        for (until, position) in self.paddle_until.iter_mut().zip(positions) {
            *until = self.now + position * CYCLES_PER_PADDLE_STEP;
        }
    }

    /// A cycle of sound: where the cone is; a sample made when one is due,
    /// the cone's place averaged over its cycles, from -1 to 1, then let
    /// settle back to the middle, as the speaker's cone does when it's left
    /// still.
    fn sound_cycle(&mut self) {
        self.sample_cycles += 1;
        self.sample_out += self.speaker as u32;
        self.sample_phase += SAMPLE_RATE;
        if self.sample_phase >= CLOCK_HZ {
            self.sample_phase -= CLOCK_HZ;
            let place = 2.0 * self.sample_out as f32 / self.sample_cycles as f32 - 1.0;
            let (last_place, last) = self.settle;
            let sample = place - last_place + SETTLE * last;
            self.settle = (place, sample);
            self.sound.push(sample * VOLUME);
            self.sample_cycles = 0;
            self.sample_out = 0;
        }
    }

    /// The soft switches, for the video circuit.
    fn switches(&self) -> video::Switches {
        video::Switches { graphics: self.graphics, mixed: self.mixed, page2: self.page2, hires: self.hires }
    }

    /// Picture line `y`, drawn from RAM by the switches as they are now.
    fn draw_line(&mut self, y: usize) {
        let flash_on = (self.frames / FLASH_FRAMES) % 2 == 0;
        let switches = self.switches();
        let out = &mut self.picture[y * video::WIDTH..(y + 1) * video::WIDTH];
        video::line(&self.ram, switches, flash_on, y, out);
    }

    /// The soft switches at $C050 to $C057, read or written alike.
    fn switch(&mut self, addr: u16) {
        let on = addr & 1 != 0;
        match addr & 0x0E {
            0x0 => self.graphics = !on,
            0x2 => self.mixed = on,
            0x4 => self.page2 = on,
            _ => self.hires = on,
        }
    }

    /// The I/O at $C000 to $C0FF, for a read or a write (`written`, the
    /// byte): what a read gives.
    fn io(&mut self, addr: u16, written: Option<u8>) -> u8 {
        match addr & 0xFFF0 {
            0xC000 => {
                if self.keyboard & 0x80 == 0 {
                    if let Some(key) = self.terminal_key().or_else(|| self.display_key()) {
                        self.press(key);
                    }
                }
                self.keyboard
            }
            0xC010 => {
                self.keyboard &= 0x7F;
                self.keyboard
            }
            0xC030 => {
                self.speaker = !self.speaker;
                0
            }
            0xC050 if addr <= 0xC057 => {
                self.switch(addr);
                0
            }
            // The game port: the buttons, bit 7 while pressed, and the
            // paddles, while their timers run.
            0xC060 => match addr & 7 {
                button @ 1..=3 => (self.joystick().buttons[button as usize - 1] as u8) << 7,
                paddle @ 4..=7 => ((self.now < self.paddle_until[paddle as usize - 4]) as u8) << 7,
                _ => 0,
            },
            0xC070 => {
                self.start_paddles();
                0
            }
            0xC0E0 => self.disk2.as_mut().map_or(0, |card| card.io(addr, written)),
            _ => 0,
        }
    }

    /// A text row's 40 screen bytes, on the page shown.
    fn row(&self, row: usize) -> &[u8] {
        let start = video::text_address(self.page2, row);
        &self.ram[start..start + 40]
    }

    /// The text screen as 24 lines of 40 characters.
    pub fn text(&self) -> Vec<String> {
        (0..24).map(|row| self.row(row).iter().map(|&b| glyph(b)).collect()).collect()
    }

    /// The text screen in the terminal, if it has changed: inverse in
    /// reverse video (below $40), and flashing (below $80) too while the
    /// flash is on.
    fn draw(&mut self) {
        let flash_on = (self.frames / FLASH_FRAMES) % 2 == 0;
        let mut screen = String::from("\x1B[H");
        for row in 0..24 {
            let mut reversed = false;
            for &b in self.row(row) {
                let reverse = b < 0x40 || (b < 0x80 && flash_on);
                if reverse != reversed {
                    screen.push_str(if reverse { "\x1B[7m" } else { "\x1B[0m" });
                    reversed = reverse;
                }
                screen.push(glyph(b));
            }
            if reversed {
                screen.push_str("\x1B[0m");
            }
            if row < 23 {
                screen.push('\n');
            }
        }
        if screen != self.shown {
            print!("{screen}");
            let _ = std::io::stdout().flush();
            self.shown = screen;
        }
    }
}

/// A screen byte's character: the II+'s 64, upper case letters and
/// symbols, whichever of its normal, inverse or flashing forms.
fn glyph(byte: u8) -> char {
    let c = byte & 0x3F;
    (if c < 0x20 { c + 0x40 } else { c }) as char
}

impl Bus for Apple2 {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0xBFFF => self.ram[addr as usize],
            0xC000..=0xC0FF => self.io(addr, None),
            0xC600..=0xC6FF => self.disk2.as_ref().map_or(0, |card| card.rom(addr)),
            0xC100..=0xCFFF => 0, // no other cards
            _ => self.rom[addr as usize - 0xD000],
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0xBFFF => self.ram[addr as usize] = value,
            0xC000..=0xC0FF => {
                self.io(addr, Some(value));
            }
            _ => {} // no cards; ROM
        }
    }

    fn tick(&mut self) {
        if let Some(card) = &mut self.disk2 {
            card.tick();
        }
        if self.display.is_some() {
            self.sound_cycle();
        }
        self.now += 1;
        self.cycle += 1;
        // A line ended: drawn, if it is one of those seen.
        if self.cycle % CYCLES_PER_LINE == 0 {
            let y = (self.cycle / CYCLES_PER_LINE - 1) as usize;
            if y < video::HEIGHT {
                self.draw_line(y);
            }
        }
        // The frame ended: shown.
        if self.cycle == CYCLES_PER_FRAME {
            self.cycle = 0;
            self.frames += 1;
            if let Some(display) = &mut self.display {
                display.show(&self.picture, video::WIDTH, video::HEIGHT);
                display.play(&self.sound);
                self.sound.clear();
            }
            if self.terminal.is_some() {
                self.draw();
            }
        }
    }
}

/// An Apple II+ run: `rom_path` a dump of its ROM (its last 12K, $D000 up);
/// `disk` a .dsk or .nib in a Disk II, whose ROMs are found beside it; in a
/// window, or `text`, its text screen in the terminal.
pub fn run(rom_path: &str, disk: Option<&str>, text: bool) {
    let read = |path: &Path| std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let rom_path = Path::new(rom_path);
    let rom = read(rom_path);
    assert!(rom.len() >= 0x3000, "the ROM is 12K, $D000 to $FFFF");
    let mut apple = Apple2::new(rom[rom.len() - 0x3000..].to_vec());

    if let Some(disk) = disk {
        let beside = |name: &str| read(&rom_path.with_file_name(name));
        let mut card = disk2::Disk2::new(beside("disk2-p5-341-0027.bin"), &beside("disk2-p6-341-0028.bin"))
            .unwrap_or_else(|e| panic!("the Disk II's ROMs: {e}"));
        let image = read(Path::new(disk));
        let lower = disk.to_lowercase();
        card.disk = Some(
            if lower.ends_with(".nib") {
                disk_image::Disk::from_nib(&image)
            } else if lower.ends_with(".dsk") || lower.ends_with(".do") {
                disk_image::Disk::from_dsk(&image)
            } else {
                Err("a .dsk, .do or .nib".to_string())
            }
            .unwrap_or_else(|e| panic!("{disk}: {e}")),
        );
        apple.disk2 = Some(card);
    }
    if text {
        apple.terminal = Some(Terminal::new());
        print!("\x1B[2J"); // the screen cleared, for the Apple's
    } else {
        apple.display = Some(Box::new(Window::new("Apple ][", video::WIDTH, video::HEIGHT, 3)));
    }

    let mut cpu = CPU::with_bus(apple);
    cpu.clock_hz = CLOCK_HZ;
    cpu.reset();
    cpu.execute_forever();
}
