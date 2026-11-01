//! The Apple II (the II+, or the IIe): a 6502 at 1.02 MHz; 48K of RAM; the keyboard,
//! the speaker, the soft switches and the joystick at $C000; a language
//! card in slot 0, Mockingboards in slots 4 and 5, a Disk II card in
//! slot 6 and a hard disk card in slot 7, if there are; and 12K of
//! ROM at $D000, the monitor and BASIC, from a dump of the machine's own
//! (not part of this repository). Its picture is drawn a line at a time, as
//! the video circuit draws it, and shown in a window once a frame; or its
//! text screen alone in the host's terminal. Its Reset key holds the RESET
//! line, the CPU's and the cards'. A read of an address nothing answers
//! gets the floating bus: the byte the video circuit last read from RAM.
//!
//! The IIe is the II+ with what `iie` adds: its 128K, the MMU's and IOU's
//! switches, its ROM's own firmware in the slots' places, its characters,
//! 80 columns and double graphics; its language card built in, its keyboard
//! with lower case and four arrows, its Open-Apple and Solid-Apple keys
//! the joystick's buttons.

pub mod disk2;
pub mod disk_image;
pub mod hard_disk;
pub mod iie;
pub mod mockingboard;
pub mod language_card;
pub mod video;

#[cfg(test)]
mod tests {
    mod disk2;
    mod disk_writing;
    mod drives;
    mod hard_disk;
    mod iie;
    mod language_card;
    mod machine;
    mod mockingboard;
}

use crate::cpu::{Variant, CPU};
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
pub const VOLUME: f32 = 0.15;
const VOICE_VOLUME: f32 = 0.25;
/// How fast the speaker's cone settles back, left still: a sample's share.
const SETTLE: f32 = 0.995;

/// The flashing characters' half period.
const FLASH_FRAMES: u32 = 16;

/// The Reset key pressed in the terminal, which has no key let go: held 6
/// frames, a tenth of a second.
const RESET_FRAMES: u32 = 6;

/// How long a notice is shown over the picture: 3 seconds.
const NOTICE_FRAMES: u32 = 180;

pub struct Apple2 {
    pub ram: Vec<u8>,
    pub rom: Vec<u8>,
    /// The keyboard: the last key, bit 7 set until $C010 clears it.
    pub keyboard: u8,
    pub terminal: Option<Terminal>,
    /// The IIe's own, on a IIe.
    pub iie: Option<iie::Iie>,
    /// The language card, in slot 0; the IIe's built in.
    pub language_card: Option<language_card::LanguageCard>,
    /// The Mockingboards, in slots 4 and 5.
    pub mockingboards: [Option<mockingboard::Mockingboard>; 2],
    /// The Disk II card, in slot 6; the hard disk card, in slot 7.
    pub disk2: Option<disk2::Disk2>,
    pub hard_disk: Option<hard_disk::HardDisk>,
    /// The disks to hand, their images: put in the drives in turn, by F9
    /// and F10; and which is in each drive.
    pub disks: Vec<PathBuf>,
    pub in_drive: [Option<usize>; 2],
    /// What's in the drives, said under the terminal's screen; and a
    /// notice over the window's picture, a disk put in, and the frames
    /// more it's shown.
    status: String,
    notice: Option<(String, u32)>,
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
    sample_voices: (f32, f32),
    sample_phase: u32,
    settle: [(f32, f32); 2],
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
            iie: None,
            language_card: None,
            mockingboards: [None, None],
            disk2: None,
            hard_disk: None,
            disks: Vec::new(),
            in_drive: [None, None],
            status: String::new(),
            notice: None,
            display: None,
            picture: vec![0; video::PICTURE_WIDTH * video::HEIGHT],
            graphics: false,
            mixed: false,
            page2: false,
            hires: false,
            speaker: false,
            sample_cycles: 0,
            sample_out: 0,
            sample_voices: (0.0, 0.0),
            sample_phase: 0,
            settle: [(-VOLUME, 0.0); 2], // the cone in, at rest
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

    /// A IIe, enhanced or not, as its ROMs are: its CD ROM's 8K, $C000 to
    /// $DFFF, and its EF ROM's, $E000 to $FFFF; its video ROM's 4K; its
    /// language card built in.
    pub fn new_iie(cd: &[u8], ef: &[u8], characters: Vec<u8>) -> Self {
        assert_eq!((cd.len(), ef.len()), (0x2000, 0x2000), "the CD and EF ROMs are 8K each");
        let mut apple = Apple2::new([&cd[0x1000..], ef].concat());
        apple.iie = Some(iie::Iie::new(cd[..0x1000].to_vec(), characters));
        apple.language_card = Some(language_card::LanguageCard::with_aux());
        apple
    }

    /// A key pressed: the keyboard's latch, as the key's ASCII with bit 7
    /// set, the last key over any not yet taken.
    pub fn press(&mut self, key: u8) {
        self.keyboard = key | 0x80;
    }

    /// The next key from the terminal, as the Apple's keyboard has it:
    /// Return for Enter, ← for Backspace and the left arrow, → for the
    /// right, upper case letters; another escape sequence, nothing. A IIe's
    /// has lower case, Delete for Backspace, and up and down too.
    fn terminal_key(&mut self) -> Option<u8> {
        let iie = self.iie.is_some();
        let terminal = self.terminal.as_ref()?;
        let key = terminal.key()?;
        Some(match key {
            b'\n' | b'\r' => 0x0D,
            0x7F if iie => 0x7F,
            0x7F => 0x08,
            b'a'..=b'z' if !iie => key - 0x20,
            0x1B => match terminal.key() {
                Some(b'[') | Some(b'O') => match terminal.key()? {
                    b'D' => 0x08,
                    b'C' => 0x15,
                    b'A' if iie => 0x0B,
                    b'B' if iie => 0x0A,
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
    /// up and down, which it hasn't, nothing. A IIe's has lower case,
    /// Delete for Backspace, and ↑ and ↓, Ctrl-K and Ctrl-J.
    fn display_key(&mut self) -> Option<u8> {
        let iie = self.iie.is_some();
        Some(match self.display.as_mut()?.key()? {
            Key::Char(b'\n') => 0x0D,
            Key::Char(0x08 | 0x7F) if iie => 0x7F,
            Key::Char(0x7F) => 0x08,
            Key::Char(c @ b'a'..=b'z') if !iie => c - 0x20,
            Key::Char(c) if c < 0x80 => c,
            Key::Left => 0x08,
            Key::Right => 0x15,
            Key::Up if iie => 0x0B,
            Key::Down if iie => 0x0A,
            _ => return None,
        })
    }

    /// Whether a key is held down: the IIe's keyboard tells, at $C010.
    fn any_key_down(&self) -> bool {
        self.display.as_ref().is_some_and(|display| display.key_down())
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
        for card in self.mockingboards.iter().flatten() {
            let (left, right) = card.output();
            self.sample_voices.0 += left;
            self.sample_voices.1 += right;
        }
        self.sample_phase += SAMPLE_RATE;
        if self.sample_phase >= CLOCK_HZ {
            self.sample_phase -= CLOCK_HZ;
            let cycles = self.sample_cycles as f32;
            let place = 2.0 * self.sample_out as f32 / cycles - 1.0;
            // The speaker in both channels, the Mockingboards' voices each in
            // theirs, their output let settle too, as an amplifier's.
            let (left, right) = self.sample_voices;
            for (channel, voices) in [left, right].into_iter().enumerate() {
                let level = place * VOLUME + voices / cycles * VOICE_VOLUME;
                let (last_level, last) = self.settle[channel];
                let sample = level - last_level + SETTLE * last;
                self.settle[channel] = (level, sample);
                self.sound.push(sample);
            }
            self.sample_cycles = 0;
            self.sample_out = 0;
            self.sample_voices = (0.0, 0.0);
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
            if let Some(iie) = &mut self.iie {
                iie.reset();
            }
            if let Some(card) = &mut self.disk2 {
                card.reset();
            }
            for card in self.mockingboards.iter_mut().flatten() {
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
        self.notice = Some((format!("Drive {}: {}", drive + 1, self.disk_name(drive)), NOTICE_FRAMES));
    }

    /// The name of drive `drive`'s disk, its image's; or empty.
    fn disk_name(&self, drive: usize) -> String {
        match self.in_drive[drive] {
            Some(n) => self.disks[n].file_stem().map_or(String::new(), |name| name.to_string_lossy().into_owned()),
            None => "empty".to_string(),
        }
    }

    /// What's in the drives, by their images' names: in the window's
    /// title, or under the terminal's screen.
    fn show_disks(&mut self) {
        self.status = format!("Drive 1: {}   Drive 2: {}", self.disk_name(0), self.disk_name(1));
        if let Some(display) = &mut self.display {
            let name = if self.iie.is_some() { "Apple //e" } else { "Apple ][" };
            display.title(&format!("{name}   {}", self.status));
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

    /// The Mockingboard whose slot `addr` is in, if there's one there.
    fn mockingboard(&mut self, addr: u16) -> Option<&mut mockingboard::Mockingboard> {
        match addr >> 8 {
            0xC4 => self.mockingboards[0].as_mut(),
            0xC5 => self.mockingboards[1].as_mut(),
            _ => None,
        }
    }

    /// The soft switches, for the video circuit.
    fn switches(&self) -> video::Switches {
        let switches = video::Switches { graphics: self.graphics, mixed: self.mixed, page2: self.page2, hires: self.hires, ..Default::default() };
        match &self.iie {
            None => switches,
            Some(iie) => video::Switches {
                page2: iie.shown_page2(self.page2),
                iie: true,
                col80: iie.col80,
                altcharset: iie.altcharset,
                dhires: iie.dhires,
                ..switches
            },
        }
    }

    /// Picture line `y`, drawn from RAM by the switches as they are now.
    fn draw_line(&mut self, y: usize) {
        let flash_on = (self.frames / FLASH_FRAMES) % 2 == 0;
        let switches = self.switches();
        let source = video::Source {
            main: &self.ram,
            aux: self.iie.as_ref().map_or(&[][..], |iie| &iie.aux),
            characters: self.iie.as_ref().map(|iie| &iie.characters[..]),
        };
        let out = &mut self.picture[y * video::PICTURE_WIDTH..(y + 1) * video::PICTURE_WIDTH];
        video::picture_line(&source, switches, flash_on, y, out);
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
        let cycle = self.frame_cycle();
        let line_cycles = CYCLES_PER_LINE as usize;
        let address = video::scanner_address(self.switches(), cycle / line_cycles, cycle % line_cycles);
        self.ram[address]
    }

    /// The cycle in the frame a read is in: the bus is ticked before it.
    fn frame_cycle(&self) -> usize {
        ((self.cycle + CYCLES_PER_FRAME - 1) % CYCLES_PER_FRAME) as usize
    }

    /// A IIe's switch read back at $C011 to $C01F, in bit 7: the language
    /// card's bank 2 and its RAM read; the MMU's; the vertical blank's,
    /// low while it is ($C019); TEXT, MIXED, PAGE2 and HIRES; ALTCHARSET
    /// and 80COL.
    fn iie_status(&self, addr: u16) -> bool {
        let (Some(iie), Some(card)) = (&self.iie, &self.language_card) else { return false };
        match addr & 0x0F {
            0x1 => card.bank2,
            0x2 => card.read_ram,
            0x3 => iie.ramrd,
            0x4 => iie.ramwrt,
            0x5 => iie.intcxrom,
            0x6 => iie.altzp,
            0x7 => iie.slotc3rom,
            0x8 => iie.store80,
            0x9 => self.frame_cycle() / (CYCLES_PER_LINE as usize) < video::HEIGHT,
            0xA => !self.graphics,
            0xB => self.mixed,
            0xC => self.page2,
            0xD => self.hires,
            0xE => iie.altcharset,
            _ => iie.col80,
        }
    }

    /// $0000 to $BFFF read: main RAM, or on a IIe the auxiliary, as its MMU
    /// says.
    fn ram_read(&self, addr: u16) -> u8 {
        match &self.iie {
            Some(iie) if iie.is_aux(addr, false, self.page2, self.hires) => iie.aux[addr as usize],
            _ => self.ram[addr as usize],
        }
    }

    /// The language card's 16K to use: on a IIe, the auxiliary with ALTZP.
    fn language_card_aux(&self) -> bool {
        self.iie.as_ref().is_some_and(|iie| iie.altzp)
    }

    /// The I/O at $C000 to $C0FF, for a read or a write (`written`, the
    /// byte): what a read gives. Only the keyboard and the Disk II's data
    /// register give a byte of their own, and the game port its bit 7;
    /// the rest is the floating bus.
    fn io(&mut self, addr: u16, written: Option<u8>) -> u8 {
        let floating = self.floating_bus();
        match addr & 0xFFF0 {
            // A IIe's switches, written.
            0xC000 if written.is_some() && self.iie.is_some() => {
                self.iie.as_mut().unwrap().switch(addr);
                floating
            }
            0xC000 => {
                if self.keyboard & 0x80 == 0 {
                    if let Some(key) = self.terminal_key().or_else(|| self.display_key()) {
                        self.press(key);
                    }
                }
                self.keyboard
            }
            // A IIe's $C010 read: whether a key is down, in bit 7, with the
            // key; its $C011 to $C01F, a switch, with the key. Each written,
            // and $C010 read, the strobe cleared.
            0xC010 if self.iie.is_some() => {
                let key = self.keyboard & 0x7F;
                if addr == 0xC010 || written.is_some() {
                    self.keyboard = key;
                }
                let high = if addr == 0xC010 { self.any_key_down() } else { self.iie_status(addr) };
                (high as u8) << 7 | key
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
            // The annunciators: on a IIe, the third's, double high-res, on
            // at $C05E, off at $C05F.
            0xC050 => {
                if let Some(iie) = &mut self.iie {
                    if addr & 0x0E == 0x0E {
                        iie.dhires = addr & 1 == 0;
                    }
                }
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
            0xC0F0 => self.hard_disk.as_mut().and_then(|card| card.io(addr, written)).unwrap_or(floating),
            _ => floating,
        }
    }

    /// A text row's 40 screen bytes, on the page shown.
    /// A text row's screen bytes, on the page shown: 40; or a IIe's in 80
    /// columns, its auxiliary and main bytes in turn.
    fn row(&self, row: usize) -> Vec<u8> {
        let switches = self.switches();
        let start = video::text_address(switches.page2, row);
        let main = &self.ram[start..start + 40];
        match &self.iie {
            Some(iie) if iie.col80 => iie.aux[start..start + 40].iter().zip(main).flat_map(|(&aux, &main)| [aux, main]).collect(),
            _ => main.to_vec(),
        }
    }

    /// The text screen as 24 lines of 40 characters: for the tests.
    #[cfg(test)]
    pub fn text(&self) -> Vec<String> {
        let lower = self.lower_case();
        (0..24).map(|row| self.row(row).iter().map(|&b| glyph(b, lower)).collect()).collect()
    }

    /// Which of the screen's bytes are lower case: none on a II+; on a
    /// IIe, $E0 up, and with ALTCHARSET $60 up, inverse.
    fn lower_case(&self) -> Option<bool> {
        self.iie.as_ref().map(|iie| iie.altcharset)
    }

    /// The text screen in the terminal, if it has changed: inverse in
    /// reverse video (below $40), and flashing (below $80) too while the
    /// flash is on.
    fn draw(&mut self) {
        let flash_on = (self.frames / FLASH_FRAMES) % 2 == 0;
        let mut screen = String::from("\x1B[H");
        let lower = self.lower_case();
        for row in 0..24 {
            let mut reversed = false;
            for b in self.row(row) {
                let reverse = b < 0x40 || (b < 0x80 && flash_on);
                if reverse != reversed {
                    screen.push_str(if reverse { "\x1B[7m" } else { "\x1B[0m" });
                    reversed = reverse;
                }
                screen.push(glyph(b, lower));
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
fn glyph(byte: u8, lower: Option<bool>) -> char {
    if let Some(altcharset) = lower {
        if byte >= 0xE0 || (altcharset && (0x60..0x80).contains(&byte)) {
            return (byte & 0x7F) as char;
        }
    }
    let c = byte & 0x3F;
    (if c < 0x20 { c + 0x40 } else { c }) as char
}

impl Bus for Apple2 {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0xBFFF => self.ram_read(addr),
            0xC000..=0xC0FF => self.io(addr, None),
            // A IIe's own ROM, as its MMU says; the Mockingboards' 6522s, in
            // slots 4 and 5; the Disk II's ROM, in slot 6; the hard disk's,
            // in 7; no other cards.
            0xC100..=0xCFFF => {
                if let Some(iie) = &mut self.iie {
                    if iie.internal(addr) {
                        return iie.internal_rom[addr as usize - 0xC000];
                    }
                }
                if let Some(card) = self.mockingboard(addr) {
                    return card.read(addr);
                }
                match (addr >> 8, &self.disk2, &self.hard_disk) {
                    (0xC6, Some(card), _) => card.rom(addr),
                    (0xC7, _, Some(card)) => card.rom(addr),
                    _ => self.floating_bus(),
                }
            }
            // The language card's RAM, if it is read; or the ROM.
            _ => match self.language_card.as_ref().and_then(|card| card.read(addr, self.language_card_aux())) {
                Some(value) => value,
                None => self.rom[addr as usize - 0xD000],
            },
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0xBFFF => match &mut self.iie {
                Some(iie) if iie.is_aux(addr, true, self.page2, self.hires) => iie.aux[addr as usize] = value,
                _ => self.ram[addr as usize] = value,
            },
            0xC000..=0xC0FF => {
                self.io(addr, Some(value));
            }
            // A IIe's own ROM, as its MMU says, written to nothing; the
            // Mockingboards.
            0xC100..=0xCFFF => {
                if let Some(iie) = &mut self.iie {
                    if iie.internal(addr) {
                        return;
                    }
                }
                if let Some(card) = self.mockingboard(addr) {
                    card.write(addr, value);
                }
            }
            0xD000..=0xFFFF => {
                let aux = self.language_card_aux();
                if let Some(card) = &mut self.language_card {
                    card.write(addr, value, aux);
                }
            }
        }
    }

    fn tick(&mut self) {
        if let Some(card) = &mut self.disk2 {
            card.tick();
        }
        for card in self.mockingboards.iter_mut().flatten() {
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
            // A notice over the picture, the next frame's lines drawing
            // over it again.
            if let Some((text, frames)) = &mut self.notice {
                video::notice(&mut self.picture, text);
                *frames -= 1;
                if *frames == 0 {
                    self.notice = None;
                }
            }
            if let Some(display) = &mut self.display {
                display.show(&self.picture, video::PICTURE_WIDTH, video::HEIGHT);
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
        self.mockingboards.iter().flatten().any(mockingboard::Mockingboard::irq)
    }
}

/// Whether the image at `path` is a hard disk's volume, not a floppy: a
/// .hdv or .2mg, or a .po bigger than a floppy's 140K.
fn is_volume(path: &Path) -> bool {
    let kind = path.extension().map_or(String::new(), |kind| kind.to_string_lossy().to_lowercase());
    let size = std::fs::metadata(path).map_or(0, |file| file.len());
    matches!(kind.as_str(), "hdv" | "2mg") || (kind == "po" && size > 143_360)
}

/// Which Apple: the II+; or a IIe, the first or the enhanced.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Model {
    IIPlus,
    IIe,
    IIeEnhanced,
}

/// An Apple run, `model`, with a language card and `mockingboards` (0 to
/// 2) in slots 4 and 5, its CPU `variant` (or its own: the 6502, the
/// enhanced IIe's 65C02): `roms`, for the II+ a dump of its ROM (its last
/// 12K, $D000 up), for a IIe the folder its ROMs are in, by their names;
/// `disks` .dsk, .do, .po, .nib or .woz images, or folders or .m3u
/// playlists of them, for a Disk II, whose ROMs are found beside the II+'s
/// or in the IIe's folder, the first in drive 1; and hard disk volumes,
/// .hdv, .2mg and .po bigger than a floppy, the first two the hard disk's
/// drives;
/// in a window, `fullscreen` or not, or `text`, its text screen in the
/// terminal.
pub fn run(model: Model, roms: &str, disks: &[&str], mockingboards: usize, variant: Option<Variant>, text: bool, fullscreen: bool) {
    let read = |path: &Path| std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let roms = Path::new(roms);
    let (mut apple, folder, title, size, own_variant) = match model {
        Model::IIPlus => {
            let rom = read(roms);
            assert!(rom.len() >= 0x3000, "the ROM is 12K, $D000 to $FFFF");
            let mut apple = Apple2::new(rom[rom.len() - 0x3000..].to_vec());
            apple.language_card = Some(language_card::LanguageCard::new());
            // Its 560 dots by 192 lines, shown 4:3: 840 by 576.
            (apple, roms.parent().unwrap_or(Path::new(".")).to_path_buf(), "Apple ][", (840, 576), Variant::Nmos6502)
        }
        Model::IIe | Model::IIeEnhanced => {
            let enhanced = model == Model::IIeEnhanced;
            let [cd, ef, characters] = if enhanced { iie::ENHANCED_ROMS } else { iie::ROMS }.map(|name| read(&roms.join(name)));
            let apple = Apple2::new_iie(&cd, &ef, characters);
            let variant = if enhanced { Variant::Synertek65C02 } else { Variant::Nmos6502 };
            // 80 columns' dots each a whole pixel: 1,120 by 768.
            (apple, roms.to_path_buf(), "Apple //e", (1120, 768), variant)
        }
    };
    for card in apple.mockingboards.iter_mut().take(mockingboards) {
        *card = Some(mockingboard::Mockingboard::new());
    }

    if text {
        apple.terminal = Some(Terminal::new());
        print!("\x1B[2J"); // the screen cleared, for the Apple's
    } else {
        apple.display = Some(Box::new(Window::new(title, video::PICTURE_WIDTH, video::HEIGHT, size, fullscreen)));
    }
    // The images, the hard disk's volumes from the Disk II's.
    let mut images = Vec::new();
    for disk in disks {
        images.extend(disk_image::images(Path::new(disk)).unwrap_or_else(|e| panic!("{disk}: {e}")));
    }
    let (volumes, floppies): (Vec<PathBuf>, Vec<PathBuf>) = images.into_iter().partition(|image| is_volume(image));
    if !volumes.is_empty() {
        assert!(volumes.len() <= 2, "the hard disk has two drives, not {}", volumes.len());
        let mut card = hard_disk::HardDisk::new();
        for (unit, volume) in card.units.iter_mut().zip(&volumes) {
            *unit = Some(hard_disk::Volume::open(volume).unwrap_or_else(|e| panic!("{}: {e}", volume.display())));
        }
        apple.hard_disk = Some(card);
    }
    if !floppies.is_empty() {
        let beside = |name: &str| read(&folder.join(name));
        let card = disk2::Disk2::new(beside("disk2-p5-341-0027.bin"), &beside("disk2-p6-341-0028.bin"))
            .unwrap_or_else(|e| panic!("the Disk II's ROMs: {e}"));
        apple.disk2 = Some(card);
        // Each image opened, to know it can be; the first put in drive 1.
        for image in &floppies {
            Disk::open(image).unwrap_or_else(|e| panic!("{}: {e}", image.display()));
        }
        apple.disks = floppies;
        apple.change_disk(0);
    }

    let mut cpu = CPU::with_bus(apple);
    cpu.clock_hz = CLOCK_HZ;
    cpu.variant = variant.unwrap_or(own_variant);
    cpu.reset();
    cpu.execute_forever();
}
