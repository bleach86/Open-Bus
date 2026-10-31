//! The Apple II (the II+): a 6502 at 1.02 MHz; 48K of RAM; the keyboard,
//! the speaker, the soft switches and the joystick at $C000; a language
//! card in slot 0, a Mockingboard in slot 4 and a Disk II card in slot 6,
//! if there are; and 12K of
//! ROM at $D000, the monitor and BASIC, from a dump of the machine's own
//! (not part of this repository). Its picture is drawn a line at a time, as
//! the video circuit draws it, and shown in a window once a frame; or its
//! text screen alone in the host's terminal. Its Reset key holds the RESET
//! line, the CPU's and the cards'. A read of an address nothing answers
//! gets the floating bus: the byte the video circuit last read from RAM.

pub mod disk2;
pub mod disk_image;
pub mod mockingboard;
pub mod language_card;
pub mod video;

#[cfg(test)]
mod tests {
    mod disk2;
    mod disk_writing;
    mod drives;
    mod language_card;
    mod machine;
    mod mockingboard;
}

use crate::cpu::CPU;
use crate::cpu::bus::Bus;
use crate::host::display::{Display, Joystick, Key, SAMPLE_RATE};
use crate::host::terminal::Terminal;
use crate::host::window::Window;
use disk_image::Disk;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The 14.31818 MHz crystal's, on average: a cycle is 14 of its ticks, but
/// each line's last is 16, keeping the colour signal in step line to line;
/// so 65 cycles a line are 912 ticks.
pub const CLOCK_HZ: u32 = 1_020_484;
/// 65 cycles a line, 262 lines: 192 seen, the rest the beam's way back.
pub const CYCLES_PER_LINE: u32 = 65;
pub const CYCLES_PER_FRAME: u32 = 17_030;
/// A paddle's timer runs 11 cycles a step of its 0 to 255: as long as the
/// monitor's PREAD takes to count one.
const CYCLES_PER_PADDLE_STEP: u64 = 11;

/// How loud the speaker is, of the host's full volume; and each of the
/// Mockingboard's voices, at its loudest.
const VOLUME: f32 = 0.3;
const VOICE_VOLUME: f32 = 0.15;
/// How fast the speaker's cone settles back, left still: a sample's share.
const SETTLE: f32 = 0.995;

/// The flashing characters' half period.
const FLASH_FRAMES: u32 = 16;

/// The Reset key pressed in the terminal, which has no key let go: held 6
/// frames, a tenth of a second.
const RESET_FRAMES: u32 = 6;

pub struct Apple2 {
    pub ram: Vec<u8>,
    pub rom: Vec<u8>,
    /// The keyboard: the last key, bit 7 set until $C010 clears it.
    pub keyboard: u8,
    pub terminal: Option<Terminal>,
    /// The language card, in slot 0.
    pub language_card: Option<language_card::LanguageCard>,
    /// The Mockingboard, in slot 4.
    pub mockingboard: Option<mockingboard::Mockingboard>,
    /// The Disk II card, in slot 6.
    pub disk2: Option<disk2::Disk2>,
    /// The disks to hand, their images: put in the drives in turn, by F9
    /// and F10; and which is in each drive.
    pub disks: Vec<PathBuf>,
    pub in_drive: [Option<usize>; 2],
    /// What's in the drives, said under the terminal's screen.
    status: String,
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
    sample_voices: f32,
    sample_phase: u32,
    settle: (f32, f32),
    sound: Vec<f32>,
    /// When each paddle's timer runs out, started by $C070.
    paddle_until: [u64; 4],
    /// The Reset key held, and the frames a terminal's press holds it on for.
    reset_held: bool,
    reset_frames: u32,
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
            language_card: None,
            mockingboard: None,
            disk2: None,
            disks: Vec::new(),
            in_drive: [None, None],
            status: String::new(),
            display: None,
            picture: vec![0; video::WIDTH * video::HEIGHT],
            graphics: false,
            mixed: false,
            page2: false,
            hires: false,
            speaker: false,
            sample_cycles: 0,
            sample_out: 0,
            sample_voices: 0.0,
            sample_phase: 0,
            settle: (-VOLUME, 0.0), // the cone in, at rest
            sound: Vec::new(),
            paddle_until: [0; 4],
            reset_held: false,
            reset_frames: 0,
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
        if let Some(card) = &self.mockingboard {
            self.sample_voices += card.output();
        }
        self.sample_phase += SAMPLE_RATE;
        if self.sample_phase >= CLOCK_HZ {
            self.sample_phase -= CLOCK_HZ;
            let cycles = self.sample_cycles as f32;
            let place = 2.0 * self.sample_out as f32 / cycles - 1.0;
            // The Mockingboard's voices with it, its output let settle too,
            // as an amplifier's.
            let level = place * VOLUME + self.sample_voices / cycles * VOICE_VOLUME;
            let (last_level, last) = self.settle;
            let sample = level - last_level + SETTLE * last;
            self.settle = (level, sample);
            self.sound.push(sample);
            self.sample_cycles = 0;
            self.sample_out = 0;
            self.sample_voices = 0.0;
        }
    }

    /// The Reset key, as a frame ends: held in the window; pressed in the
    /// terminal, held RESET_FRAMES. While it is, the cards are held reset,
    /// and the CPU.
    fn reset_key(&mut self) {
        self.reset_held = self.display.as_ref().is_some_and(|display| display.reset_held()) || self.reset_frames > 0;
        self.reset_frames = self.reset_frames.saturating_sub(1);
        if self.reset_held {
            if let Some(card) = &mut self.language_card {
                card.reset();
            }
            if let Some(card) = &mut self.disk2 {
                card.reset();
            }
            if let Some(card) = &mut self.mockingboard {
                card.reset();
            }
        }
    }

    /// The function keys pressed, as a frame ends, in the window or the
    /// terminal: F9, the next disk in drive 1; F10, the next in drive 2, or
    /// none; F12, in the terminal, the Reset key.
    fn function_keys(&mut self) {
        let mut keys = Vec::new();
        if let Some(terminal) = &self.terminal {
            keys.extend(std::iter::from_fn(|| terminal.function_key()));
        }
        if let Some(display) = &mut self.display {
            keys.extend(std::iter::from_fn(|| display.function_key()));
        }
        for key in keys {
            match key {
                9 => self.change_disk(0),
                10 => self.change_disk(1),
                12 => self.reset_frames = RESET_FRAMES,
                _ => {}
            }
        }
    }

    /// The disk in drive `drive` (0 or 1) taken out, saved if it has been
    /// written, and the next of the disks put in, read from its image: of
    /// those not in the other drive, in turn, and for drive 2, none after
    /// the last.
    fn change_disk(&mut self, drive: usize) {
        if self.disk2.is_none() {
            return;
        }
        self.save_disk(drive, true);
        let other = self.in_drive[1 - drive];
        let mut choices: Vec<Option<usize>> = (0..self.disks.len()).filter(|&n| Some(n) != other).map(Some).collect();
        if drive == 1 {
            choices.push(None);
        }
        if choices.is_empty() {
            return;
        }
        // The one after the one in; the first, if it's none of them.
        let at = match choices.iter().position(|&choice| choice == self.in_drive[drive]) {
            Some(at) => (at + 1) % choices.len(),
            None => 0,
        };
        let disk = choices[at].and_then(|n| {
            Disk::open(&self.disks[n]).map_err(|e| eprintln!("{}: {e}", self.disks[n].display())).ok()
        });
        self.in_drive[drive] = choices[at].filter(|_| disk.is_some());
        self.disk2.as_mut().unwrap().drives[drive].disk = disk;
        self.show_disks();
    }

    /// What's in the drives, by their images' names: in the window's
    /// title, or under the terminal's screen.
    fn show_disks(&mut self) {
        let name = |drive: usize| match self.in_drive[drive] {
            Some(n) => self.disks[n].file_stem().map_or(String::new(), |name| name.to_string_lossy().into_owned()),
            None => "empty".to_string(),
        };
        self.status = format!("Drive 1: {}   Drive 2: {}", name(0), name(1));
        if let Some(display) = &mut self.display {
            display.title(&format!("Apple ][   {}", self.status));
        }
        self.shown.clear(); // the terminal's drawn again
    }

    /// Drive `drive`'s disk saved to its image, if it has been written:
    /// once it has stopped turning, so once what was writing it has
    /// finished, as DOS or a game lets the motor go when it has; or, `now`,
    /// taken out or the machine going off. Saved to a file beside it, then
    /// put in its place, so the image is never half saved.
    fn save_disk(&mut self, drive: usize, now: bool) {
        let Some(card) = &mut self.disk2 else { return };
        if card.turning(drive) && !now {
            return;
        }
        let Some(disk) = card.drives[drive].disk.as_mut().filter(|disk| disk.written) else { return };
        let Some(path) = disk.path.clone() else { return };
        disk.written = false;
        let saving = path.with_extension("saving");
        let saved = disk.to_image().and_then(|image| {
            std::fs::write(&saving, image).and_then(|_| std::fs::rename(&saving, &path)).map_err(|e| e.to_string())
        });
        if let Err(e) = saved {
            eprintln!("{}: not saved: {e}", path.display());
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

    /// The floating bus: what a read of an address nothing answers gets,
    /// the byte the video circuit read from RAM in the cycle before, still
    /// on the bus. The cycle in the frame is the one the read is in: the
    /// bus is ticked before a read.
    fn floating_bus(&self) -> u8 {
        let cycle = ((self.cycle + CYCLES_PER_FRAME - 1) % CYCLES_PER_FRAME) as usize;
        let line_cycles = CYCLES_PER_LINE as usize;
        let address = video::scanner_address(self.switches(), cycle / line_cycles, cycle % line_cycles);
        self.ram[address]
    }

    /// The I/O at $C000 to $C0FF, for a read or a write (`written`, the
    /// byte): what a read gives. Only the keyboard and the Disk II's data
    /// register give a byte of their own, and the game port its bit 7;
    /// the rest is the floating bus.
    fn io(&mut self, addr: u16, written: Option<u8>) -> u8 {
        let floating = self.floating_bus();
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
                floating
            }
            0xC030 => {
                self.speaker = !self.speaker;
                floating
            }
            0xC050 if addr <= 0xC057 => {
                self.switch(addr);
                floating
            }
            // The game port: the buttons, bit 7 while pressed, and the
            // paddles, while their timers run; the cassette input, not.
            0xC060 => {
                let high = match addr & 7 {
                    button @ 1..=3 => self.joystick().buttons[button as usize - 1],
                    paddle @ 4..=7 => self.now < self.paddle_until[paddle as usize - 4],
                    _ => false,
                };
                (high as u8) << 7 | (floating & 0x7F)
            }
            0xC070 => {
                self.start_paddles();
                floating
            }
            0xC080 => {
                if let Some(card) = &mut self.language_card {
                    card.switch(addr, written.is_none());
                }
                floating
            }
            0xC0E0 => self.disk2.as_mut().and_then(|card| card.io(addr, written)).unwrap_or(floating),
            _ => floating,
        }
    }

    /// A text row's 40 screen bytes, on the page shown.
    fn row(&self, row: usize) -> &[u8] {
        let start = video::text_address(self.page2, row);
        &self.ram[start..start + 40]
    }

    /// The text screen as 24 lines of 40 characters: for the tests.
    #[cfg(test)]
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
        if !self.status.is_empty() {
            screen.push_str(&format!("\n\x1B[0m\x1B[K{}", self.status));
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
            // The Mockingboard's 6522s, in slot 4; the Disk II's ROM, in
            // slot 6; no other cards.
            0xC100..=0xCFFF => match (addr >> 8 & 0x0F, &mut self.mockingboard, &self.disk2) {
                (4, Some(card), _) => card.read(addr),
                (6, _, Some(card)) => card.rom(addr),
                _ => self.floating_bus(),
            },
            // The language card's RAM, if it is read; or the ROM.
            _ => match self.language_card.as_ref().and_then(|card| card.read(addr)) {
                Some(value) => value,
                None => self.rom[addr as usize - 0xD000],
            },
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0xBFFF => self.ram[addr as usize] = value,
            0xC000..=0xC0FF => {
                self.io(addr, Some(value));
            }
            0xC400..=0xC4FF => {
                if let Some(card) = &mut self.mockingboard {
                    card.write(addr, value);
                }
            }
            0xD000..=0xFFFF => {
                if let Some(card) = &mut self.language_card {
                    card.write(addr, value);
                }
            }
            _ => {} // no cards there
        }
    }

    fn tick(&mut self) {
        if let Some(card) = &mut self.disk2 {
            card.tick();
        }
        if let Some(card) = &mut self.mockingboard {
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
            self.function_keys();
            self.reset_key();
            self.save_disk(0, false);
            self.save_disk(1, false);
            // The window closed: the disks saved, and off.
            if self.display.as_ref().is_some_and(|display| display.closed()) {
                self.save_disk(0, true);
                self.save_disk(1, true);
                std::process::exit(0);
            }
        }
    }

    fn reset_held(&self) -> bool {
        self.reset_held
    }

    fn irq(&self) -> bool {
        self.mockingboard.as_ref().is_some_and(mockingboard::Mockingboard::irq)
    }
}

/// An Apple II+ run, with a language card and a Mockingboard: `rom_path` a dump of its ROM (its
/// last 12K, $D000 up); `disks` .dsk, .do, .po, .nib or .woz images, for a
/// Disk II, whose ROMs are found beside it, the first in drive 1; in a
/// window, or `text`, its text screen in the terminal.
pub fn run(rom_path: &str, disks: &[&str], text: bool) {
    let read = |path: &Path| std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let rom_path = Path::new(rom_path);
    let rom = read(rom_path);
    assert!(rom.len() >= 0x3000, "the ROM is 12K, $D000 to $FFFF");
    let mut apple = Apple2::new(rom[rom.len() - 0x3000..].to_vec());
    apple.language_card = Some(language_card::LanguageCard::new());
    apple.mockingboard = Some(mockingboard::Mockingboard::new());

    if text {
        apple.terminal = Some(Terminal::new());
        print!("\x1B[2J"); // the screen cleared, for the Apple's
    } else {
        apple.display = Some(Box::new(Window::new("Apple ][", video::WIDTH, video::HEIGHT, 3)));
    }
    if !disks.is_empty() {
        let beside = |name: &str| read(&rom_path.with_file_name(name));
        let card = disk2::Disk2::new(beside("disk2-p5-341-0027.bin"), &beside("disk2-p6-341-0028.bin"))
            .unwrap_or_else(|e| panic!("the Disk II's ROMs: {e}"));
        apple.disk2 = Some(card);
        // Each image opened, to know it can be; the first put in drive 1.
        for disk in disks {
            Disk::open(Path::new(disk)).unwrap_or_else(|e| panic!("{disk}: {e}"));
        }
        apple.disks = disks.iter().map(PathBuf::from).collect();
        apple.change_disk(0);
    }

    let mut cpu = CPU::with_bus(apple);
    cpu.clock_hz = CLOCK_HZ;
    cpu.reset();
    cpu.execute_forever();
}
