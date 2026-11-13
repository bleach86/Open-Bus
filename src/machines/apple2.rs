//! The Apple II (the II+, or the IIe): a 6502 at 1.02 MHz; 48K of RAM; the keyboard,
//! the speaker, the soft switches and the joystick at $C000; a language
//! card in slot 0, a clock card in slot 1, Mockingboards in slots 4 and
//! 5, a Disk II card in slot 6 and a hard disk card in slot 7, if there
//! are; and 12K of
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

pub mod clock_card;
pub mod disk2;
pub mod disk_image;
pub mod hard_disk;
pub mod iie;
pub mod language_card;
pub mod mockingboard;
pub mod rom_set;
pub mod video;

#[cfg(test)]
mod tests {
    mod clock_card;
    mod disk2;
    mod disk_writing;
    mod drives;
    mod hard_disk;
    mod iie;
    mod language_card;
    mod machine;
    mod mockingboard;
    mod rom_set;
    mod save_state;
}

use crate::cpu::bus::Bus;
use crate::cpu::{CPU, Variant};
#[cfg(not(target_arch = "wasm32"))]
use crate::host::display::Display;
use crate::host::display::{Joystick, Key};
use crate::host::files;
use crate::host::machine::{Input, Machine, Picture};
#[cfg(not(target_arch = "wasm32"))]
use crate::host::run::Host;
use crate::host::save_state;
use crate::host::sound::{HighPass, Sampler};
#[cfg(not(target_arch = "wasm32"))]
use crate::host::terminal::Terminal;
#[cfg(not(target_arch = "wasm32"))]
use crate::host::window::Window;
use crate::host::zip::crc32;
use disk_image::Disk;
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;
use std::collections::VecDeque;
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
pub const VOLUME: f32 = 0.35;
const VOICE_VOLUME: f32 = 0.75;
/// How fast the speaker's cone settles back, left still: a sample's share.
const SETTLE: f32 = 0.995;

/// The flashing characters' half period.
const FLASH_FRAMES: u32 = 16;

/// Its sound, when it's made: the cone's place and the voices, left and
/// right, a sample at a time; the settling of each channel.
struct Sound {
    sampler: Sampler<3>,
    settle: [HighPass; 2],
}

/// In a save state, all but its ROMs and what's the host's (the picture
/// and sound, the keys and controls), which the machine it's loaded into
/// has; and the hard disk's volumes, written to their images as they go.
/// The disks in the drives are in it, bit by bit: loaded, they're as they
/// were when it was saved, and saved to their images when next written.
#[derive(Serialize, Deserialize)]
pub struct Apple2 {
    pub ram: Vec<u8>,
    #[serde(skip)]
    pub rom: Vec<u8>,
    /// The keyboard: the last key, bit 7 set until $C010 clears it.
    pub keyboard: u8,
    /// The keys typed, as its keyboard has them, not yet taken; and the
    /// controls, as the host last gave them.
    #[serde(skip)]
    keys: VecDeque<u8>,
    #[serde(skip)]
    joystick: Joystick,
    #[serde(skip)]
    key_down: bool,
    /// The IIe's own, on a IIe.
    pub iie: Option<iie::Iie>,
    /// The language card, in slot 0; the IIe's built in.
    pub language_card: Option<language_card::LanguageCard>,
    /// The clock card, in slot 1.
    pub clock: Option<clock_card::ClockCard>,
    /// The Mockingboards, in slots 4 and 5.
    pub mockingboards: [Option<mockingboard::Mockingboard>; 2],
    /// The Disk II card, in slot 6; the hard disk card, in slot 7.
    pub disk2: Option<disk2::Disk2>,
    pub hard_disk: Option<hard_disk::HardDisk>,
    /// The disks to hand, their images: put in the drives in turn, by F9
    /// and F10; and which is in each drive.
    pub disks: Vec<PathBuf>,
    pub in_drive: [Option<usize>; 2],
    /// The picture, as far as this frame has drawn it; and the monitor
    /// it's shown on, colour or monochrome.
    #[serde(skip)]
    pub picture: Vec<u32>,
    #[serde(skip)]
    pub monitor: video::Monitor,
    // The soft switches.
    pub graphics: bool,
    pub mixed: bool,
    pub page2: bool,
    pub hires: bool,
    /// The speaker's cone: out, or in. $C030 flips it.
    speaker: bool,
    /// Its sound, if it's made, and the samples made and not yet taken.
    #[serde(skip)]
    sound: Option<Sound>,
    #[serde(skip)]
    samples: Vec<f32>,
    /// When each paddle's timer runs out, started by $C070.
    paddle_until: [u64; 4],
    /// The Reset key held.
    reset_held: bool,
    /// Cycles since power-on; cycles into the frame, and frames; and
    /// whether a frame has ended, for the host.
    now: u64,
    cycle: u32,
    pub frames: u32,
    #[serde(skip)]
    frame_ended: bool,
}

impl Apple2 {
    /// The machine with `rom`, 12K for $D000 to $FFFF.
    pub fn new(rom: Vec<u8>) -> Self {
        assert_eq!(rom.len(), 0x3000, "the ROM is 12K, $D000 to $FFFF");
        Apple2 {
            ram: vec![0; 0xC000],
            rom,
            keyboard: 0,
            keys: VecDeque::new(),
            joystick: Joystick::CENTRED,
            key_down: false,
            iie: None,
            language_card: None,
            clock: None,
            mockingboards: [None, None],
            disk2: None,
            hard_disk: None,
            disks: Vec::new(),
            in_drive: [None, None],
            picture: vec![0; video::PICTURE_WIDTH * video::HEIGHT],
            monitor: video::Monitor::Colour,
            graphics: false,
            mixed: false,
            page2: false,
            hires: false,
            speaker: false,
            sound: None,
            samples: Vec::new(),
            paddle_until: [0; 4],
            reset_held: false,
            now: 0,
            cycle: 0,
            frames: 0,
            frame_ended: false,
        }
    }

    /// A IIe, enhanced or not, as its ROMs are: its CD ROM's 8K, $C000 to
    /// $DFFF, and its EF ROM's, $E000 to $FFFF; its video ROM's 4K; its
    /// language card built in.
    pub fn new_iie(cd: &[u8], ef: &[u8], characters: Vec<u8>) -> Self {
        assert_eq!(
            (cd.len(), ef.len()),
            (0x2000, 0x2000),
            "the CD and EF ROMs are 8K each"
        );
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

    /// A key, as the Apple's keyboard has it: ← for Backspace and the left
    /// arrow, → for the right, upper case letters; up and down, which it
    /// hasn't, nothing. A IIe's has lower case, Delete for Backspace, and ↑
    /// and ↓, Ctrl-K and Ctrl-J.
    fn keyboard_key(&self, key: Key) -> Option<u8> {
        let iie = self.iie.is_some();
        Some(match key {
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

    /// The paddles' timers started, at $C070: each to run as long as its
    /// position, 0 to 255; the stick's two across and down, and the other
    /// two, with nothing on them, as far as they go.
    fn start_paddles(&mut self) {
        let joystick = self.joystick;
        let position = |axis: f32| ((axis.clamp(-1.0, 1.0) + 1.0) * 127.5).round() as u64;
        let positions = [position(joystick.x), position(joystick.y), 255, 255];
        for (until, position) in self.paddle_until.iter_mut().zip(positions) {
            *until = self.now + position * CYCLES_PER_PADDLE_STEP;
        }
    }

    /// A cycle of sound: where the cone is, and the voices; a sample made
    /// when one is due, the cone's place averaged over its cycles, from -1
    /// to 1: the speaker in both channels, the Mockingboards' voices each
    /// in theirs, let settle back to the middle, as the speaker's cone does
    /// when it's left still, and an amplifier's output.
    fn sound_cycle(&mut self) {
        let Some(sound) = &mut self.sound else { return };
        let mut voices = (0.0, 0.0);
        for card in self.mockingboards.iter().flatten() {
            let (left, right) = card.output();
            voices.0 += left;
            voices.1 += right;
        }
        let place = if self.speaker { 1.0 } else { -1.0 };
        if let Some([place, left, right]) = sound.sampler.cycle([place, voices.0, voices.1]) {
            for (settle, voices) in sound.settle.iter_mut().zip([left, right]) {
                let level = place * VOLUME + voices * VOICE_VOLUME;
                self.samples.push(settle.filter(level));
            }
        }
    }

    /// The disk in drive `drive` (0 or 1) taken out, saved if it has been
    /// written, and the next of the disks put in, read from its image: of
    /// those not in the other drive, in turn, and for drive 2, none after
    /// the last. A notice of it, if there was a disk to change.
    fn change_disk(&mut self, drive: usize) -> Option<String> {
        self.disk2.as_ref()?;
        self.save_disk(drive, true);
        let other = self.in_drive[1 - drive];
        let mut choices: Vec<Option<usize>> = (0..self.disks.len())
            .filter(|&n| Some(n) != other)
            .map(Some)
            .collect();
        if drive == 1 {
            choices.push(None);
        }
        if choices.is_empty() {
            return None;
        }
        // The one after the one in; the first, if it's none of them.
        let at = match choices
            .iter()
            .position(|&choice| choice == self.in_drive[drive])
        {
            Some(at) => (at + 1) % choices.len(),
            None => 0,
        };
        let disk = choices[at].and_then(|n| {
            Disk::open(&self.disks[n])
                .map_err(|e| eprintln!("{}: {e}", self.disks[n].display()))
                .ok()
        });
        self.in_drive[drive] = choices[at].filter(|_| disk.is_some());
        self.disk2.as_mut().unwrap().drives[drive].disk = disk;
        Some(format!("Drive {}: {}", drive + 1, self.disk_name(drive)))
    }

    /// The name of drive `drive`'s disk, its image's; or empty.
    fn disk_name(&self, drive: usize) -> String {
        match self.in_drive[drive] {
            Some(n) => self.disks[n]
                .file_stem()
                .map_or(String::new(), |name| name.to_string_lossy().into_owned()),
            None => "empty".to_string(),
        }
    }

    /// What's in the drives, by their images' names, if it has any: in the
    /// window's title, or under the terminal's screen.
    fn status(&self) -> String {
        if self.disk2.is_none() {
            return String::new();
        }
        format!(
            "Drive 1: {}   Drive 2: {}",
            self.disk_name(0),
            self.disk_name(1)
        )
    }

    /// Drive `drive`'s disk saved to its image, if it has been written:
    /// once it has stopped turning, so once what was writing it has
    /// finished, as DOS or a game lets the motor go when it has; or, `now`,
    /// taken out or the machine going off.
    fn save_disk(&mut self, drive: usize, now: bool) {
        let Some(card) = &mut self.disk2 else { return };
        if card.turning(drive) && !now {
            return;
        }
        let Some(disk) = card.drives[drive].disk.as_mut().filter(|disk| disk.written) else {
            return;
        };
        let Some(path) = disk.path.clone() else {
            return;
        };
        disk.written = false;
        match disk.to_image() {
            Ok(image) => files::save_later(&path, image),
            Err(e) => eprintln!("{}: not saved: {e}", path.display()),
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
        let switches = video::Switches {
            graphics: self.graphics,
            mixed: self.mixed,
            page2: self.page2,
            hires: self.hires,
            ..Default::default()
        };
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
        video::picture_line(&source, switches, self.monitor, flash_on, y, out);
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
        let address =
            video::scanner_address(self.switches(), cycle / line_cycles, cycle % line_cycles);
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
        let (Some(iie), Some(card)) = (&self.iie, &self.language_card) else {
            return false;
        };
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
                    if let Some(key) = self.keys.pop_front() {
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
                let high = if addr == 0xC010 {
                    self.key_down
                } else {
                    self.iie_status(addr)
                };
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
                    button @ 1..=3 => self.joystick.buttons[button as usize - 1],
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
            0xC090 => self
                .clock
                .as_mut()
                .and_then(|card| card.io(addr, written))
                .unwrap_or(floating),
            0xC0E0 => self
                .disk2
                .as_mut()
                .and_then(|card| card.io(addr, written))
                .unwrap_or(floating),
            0xC0F0 => self
                .hard_disk
                .as_mut()
                .and_then(|card| card.io(addr, written))
                .unwrap_or(floating),
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
            Some(iie) if iie.col80 => iie.aux[start..start + 40]
                .iter()
                .zip(main)
                .flat_map(|(&aux, &main)| [aux, main])
                .collect(),
            _ => main.to_vec(),
        }
    }

    /// The text screen as 24 lines of 40 characters: for the tests.
    #[cfg(test)]
    pub fn text(&self) -> Vec<String> {
        let lower = self.lower_case();
        (0..24)
            .map(|row| self.row(row).iter().map(|&b| glyph(b, lower)).collect())
            .collect()
    }

    /// Which of the screen's bytes are lower case: none on a II+; on a
    /// IIe, $E0 up, and with ALTCHARSET $60 up, inverse.
    fn lower_case(&self) -> Option<bool> {
        self.iie.as_ref().map(|iie| iie.altcharset)
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
            // A IIe's own ROM, as its MMU says; the clock's, in slot 1; the
            // Mockingboards' 6522s, in slots 4 and 5; the Disk II's ROM, in
            // slot 6; the hard disk's, in 7; no other cards.
            0xC100..=0xCFFF => {
                if let Some(iie) = &mut self.iie {
                    if iie.internal(addr) {
                        return iie.internal_rom[addr as usize - 0xC000];
                    }
                }
                if let Some(card) = self.mockingboard(addr) {
                    return card.read(addr);
                }
                match (addr >> 8, &self.clock, &self.disk2, &self.hard_disk) {
                    (0xC1, Some(card), _, _) => card.rom(addr),
                    (0xC6, _, Some(card), _) => card.rom(addr),
                    (0xC7, _, _, Some(card)) => card.rom(addr),
                    _ => self.floating_bus(),
                }
            }
            // The language card's RAM, if it is read; or the ROM.
            _ => match self
                .language_card
                .as_ref()
                .and_then(|card| card.read(addr, self.language_card_aux()))
            {
                Some(value) => value,
                None => self.rom[addr as usize - 0xD000],
            },
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0xBFFF => {
                let aux = self
                    .iie
                    .as_ref()
                    .is_some_and(|iie| iie.is_aux(addr, true, self.page2, self.hires));

                if aux {
                    self.iie.as_mut().unwrap().aux[addr as usize] = value;
                } else {
                    self.ram[addr as usize] = value;
                }
            }
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
        self.sound_cycle();
        self.now += 1;
        self.cycle += 1;
        // A line ended: drawn, if it is one of those seen.
        if self.cycle % CYCLES_PER_LINE == 0 {
            let y = (self.cycle / CYCLES_PER_LINE - 1) as usize;
            if y < video::HEIGHT {
                self.draw_line(y);
            }
        }
        // The frame ended: over to the host.
        if self.cycle == CYCLES_PER_FRAME {
            self.cycle = 0;
            self.frames += 1;
            self.frame_ended = true;
        }
    }

    fn reset_held(&self) -> bool {
        self.reset_held
    }

    fn irq(&self) -> bool {
        self.mockingboards
            .iter()
            .flatten()
            .any(mockingboard::Mockingboard::irq)
    }

    fn hand_back(&mut self) -> bool {
        std::mem::take(&mut self.frame_ended)
    }
}

impl Machine for Apple2 {
    fn picture(&mut self) -> Picture<'_> {
        Picture {
            pixels: &self.picture,
            width: video::PICTURE_WIDTH,
            height: video::HEIGHT,
        }
    }

    /// The text screen: inverse in reverse video (below $40), and flashing
    /// (below $80) too while the flash is on; what's in the drives under it.
    fn text_screen(&self) -> Option<String> {
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
        let status = self.status();
        if !status.is_empty() {
            screen.push_str(&format!("\n\x1B[0m\x1B[K{status}"));
        }
        Some(screen)
    }

    /// A Disk II drive's motor on or running on, as its IN USE light is;
    /// or the hard disk's block read or written.
    fn light(&mut self) -> bool {
        let used = self
            .hard_disk
            .as_mut()
            .is_some_and(|card| std::mem::take(&mut card.used));
        used || self.disk2.as_ref().is_some_and(|card| card.spinning())
    }

    fn debug(&self) -> Vec<String> {
        let model = if self.iie.is_some() { "IIE" } else { "II+" };
        let switches = self.switches();
        let double = switches.col80 && switches.dhires;
        let mode = match (switches.graphics, switches.hires) {
            (false, _) if switches.col80 => "TEXT 80",
            (false, _) => "TEXT 40",
            (true, true) if double => "DOUBLE HIRES",
            (true, true) => "HIRES",
            (true, false) if double => "DOUBLE LORES",
            (true, false) => "LORES",
        };
        let mixed = if switches.graphics && switches.mixed {
            " MIXED"
        } else {
            ""
        };
        let page = if switches.page2 { " PAGE 2" } else { "" };
        let mut lines = vec![
            format!("APPLE {model} FRAME {}", self.frames),
            format!("{mode}{mixed}{page} {}", self.monitor.name()),
        ];
        if let Some(card) = &self.disk2 {
            let (drive, track) = card.head();
            let motor = if card.motor { "ON" } else { "OFF" };
            lines.push(format!("DRIVE {} TRACK {track} MOTOR {motor}", drive + 1));
            lines.push(format!("D1 {}", self.disk_name(0)));
            lines.push(format!("D2 {}", self.disk_name(1)));
        }
        if let Some(card) = &self.hard_disk {
            lines.push(format!(
                "HARD DISK {} VOLUMES",
                card.units.iter().flatten().count()
            ));
        }
        let boards = self.mockingboards.iter().flatten().count();
        if boards > 0 {
            lines.push(format!("MOCKINGBOARDS {boards}"));
        }
        lines
    }

    fn title(&self) -> String {
        let name = if self.iie.is_some() {
            "Apple //e"
        } else {
            "Apple ]["
        };
        match self.status() {
            status if status.is_empty() => name.to_string(),
            status => format!("{name}   {status}"),
        }
    }

    fn sound_on(&mut self) {
        // The cone in, at rest.
        let settle = || HighPass::settled(SETTLE, -VOLUME);
        self.sound = Some(Sound {
            sampler: Sampler::new(CLOCK_HZ),
            settle: [settle(), settle()],
        });
    }

    fn sound(&mut self) -> &mut Vec<f32> {
        &mut self.samples
    }

    fn key(&mut self, key: Key) {
        self.keys.extend(self.keyboard_key(key));
    }

    fn keyboard(&self) -> bool {
        true
    }

    /// Each line's end, whichever system's, a Return; a tab a space;
    /// typographers' quotes and dashes as on a typewriter; and what the
    /// keyboard hasn't, left out. A II+'s letters in upper case.
    fn type_text(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        for c in text.chars() {
            let typed = match c {
                '\n' => b'\n',
                '\t' => b' ',
                '‘' | '’' => b'\'',
                '“' | '”' => b'"',
                '–' | '—' => b'-',
                c if c.is_ascii() && !c.is_ascii_control() => c as u8,
                _ => continue,
            };
            self.keys.extend(self.keyboard_key(Key::Char(typed)));
        }
    }

    /// The text screen's 24 lines; or with a picture over the rest, its 4
    /// below; with none, nothing. Inverse and flashing as they'd read.
    fn screen_text(&self) -> Option<String> {
        let switches = self.switches();
        let rows = match (switches.graphics, switches.mixed) {
            (false, _) => 0..24,
            (true, true) => 20..24,
            (true, false) => return None,
        };
        let lower = self.lower_case();
        let lines: Vec<String> = rows
            .map(|row| {
                let line: String = self.row(row).iter().map(|&b| glyph(b, lower)).collect();
                line.trim_end().to_string()
            })
            .collect();
        let first = lines.iter().position(|line| !line.is_empty());
        let last = lines.iter().rposition(|line| !line.is_empty());
        Some(match (first, last) {
            (Some(first), Some(last)) => lines[first..=last].join("\n"),
            _ => String::new(),
        })
    }

    fn input(&mut self, input: Input) {
        self.joystick = input.joystick;
        self.key_down = input.key_down;
    }

    fn menu(&self) -> Vec<(&'static str, u8)> {
        let mut items = vec![("NEXT MONITOR", 8)];
        if self.disk2.is_some() {
            items.extend([("NEXT DISK, DRIVE 1", 9), ("NEXT DISK, DRIVE 2", 10)]);
        }
        items
    }

    /// F8, the next monitor, colour or monochrome; F9, the next disk in
    /// drive 1; F10, the next in drive 2, or none.
    fn function_key(&mut self, key: u8) -> Option<String> {
        match key {
            8 => {
                self.monitor = self.monitor.next();
                Some(format!("Monitor: {}", self.monitor.name()))
            }
            9 => self.change_disk(0),
            10 => self.change_disk(1),
            _ => None,
        }
    }

    /// Held, the cards are held reset, and the CPU; and what's still to be
    /// typed of a paste is dropped.
    fn reset_button(&mut self, held: bool) {
        self.reset_held = held;
        if held {
            self.keys.clear();
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

    /// The disks written saved, once they've stopped; or all of them,
    /// the machine going off.
    fn keep_saved(&mut self, closing: bool) {
        self.save_disk(0, closing);
        self.save_disk(1, closing);
    }

    /// Which Apple, a checksum of its ROMs (the Disk II's too), and the hard
    /// disk's volumes: what a state of it leaves out.
    fn identity(&self) -> String {
        let mut roms = self.rom.clone();
        if let Some(iie) = &self.iie {
            roms.extend(&iie.internal_rom);
            roms.extend(&iie.characters);
        }
        if let Some(card) = &self.disk2 {
            card.roms().iter().for_each(|rom| roms.extend(*rom));
        }
        let name = if self.iie.is_some() {
            "Apple //e"
        } else {
            "Apple ]["
        };
        let mut machine = format!("{name}, ROMs {:08X}", crc32(&roms));
        if self.disk2.is_some() {
            machine += ", a Disk II";
        }
        let units = self.hard_disk.iter().flat_map(|card| card.units.iter());
        for volume in units.flatten() {
            let path = files::canonical(&volume.path);
            machine += &format!(", the hard disk's {}", path.display());
        }
        machine
    }

    fn keep(&mut self, running: &mut Apple2) {
        use std::mem::take;
        self.rom = take(&mut running.rom);
        self.picture = take(&mut running.picture);
        self.monitor = running.monitor;
        self.sound = running.sound.take();
        self.samples = take(&mut running.samples);
        self.joystick = running.joystick;
        self.key_down = running.key_down;
        if let (Some(iie), Some(running)) = (&mut self.iie, &mut running.iie) {
            iie.internal_rom = take(&mut running.internal_rom);
            iie.characters = take(&mut running.characters);
        }
        if let (Some(card), Some(running)) = (&mut self.disk2, &mut running.disk2) {
            card.take_roms(running);
        }
        if let (Some(card), Some(running)) = (&mut self.hard_disk, &mut running.hard_disk) {
            card.units = take(&mut running.units);
        }
        if let (Some(card), Some(running)) = (&mut self.clock, &running.clock) {
            card.fixed = running.fixed;
        }
    }
}

/// Whether the image at `path` is a hard disk's volume, not a floppy: a
/// .hdv or .2mg, or a .po bigger than a floppy's 140K.
pub fn is_volume(path: &Path) -> bool {
    let kind = path
        .extension()
        .map_or(String::new(), |kind| kind.to_string_lossy().to_lowercase());
    let size = files::size(path).unwrap_or(0);
    matches!(kind.as_str(), "hdv" | "2mg") || (kind == "po" && size > 143_360)
}

/// Which Apple: the II+; or a IIe, the first or the enhanced.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Model {
    IIPlus,
    IIe,
    IIeEnhanced,
}

/// An Apple built, ready to run, on whichever host: the CPU, its machine
/// within it, reset; a notice of the disk put in, if there was one; where
/// its save state is kept.
pub struct Built {
    pub cpu: CPU<Apple2>,
    pub notice: Option<String>,
    pub state: PathBuf,
}

impl Model {
    /// How big its picture's shown: the II+'s 560 dots by 192 lines at 4:3,
    /// 840 by 576; a IIe's 80 columns' dots each a whole pixel, 1,120 by 768.
    pub fn size(self) -> (u32, u32) {
        match self {
            Model::IIPlus => (840, 576),
            Model::IIe | Model::IIeEnhanced => (1120, 768),
        }
    }
}

/// An Apple, `model`, with a language card, a clock card in slot 1 and
/// `mockingboards` (0 to 2) in slots 4 and 5, its CPU `variant` (or its
/// own: the 6502, the enhanced IIe's 65C02): `roms`, for the II+ a dump of
/// its ROM (its last 12K, $D000 up), for a IIe the folder its ROMs are in,
/// by their names; `disks` .dsk, .do, .po, .nib or .woz images, or folders
/// or .m3u playlists of them, for a Disk II, whose ROMs are found beside
/// the II+'s or in the IIe's folder, the first in drive 1; and hard disk
/// volumes, .hdv, .2mg and .po bigger than a floppy, the first two the
/// hard disk's drives; on `monitor`. Its save state is kept beside its
/// first disk, or its ROM.
pub fn build(
    model: Model,
    roms: &str,
    disks: &[&str],
    mockingboards: usize,
    variant: Option<Variant>,
    monitor: video::Monitor,
) -> Result<Built, String> {
    let read = |path: &Path| files::read(path).map_err(|e| format!("{}: {e}", path.display()));
    let roms = Path::new(roms);
    // The ROMs: by the name each has, else as they're known wherever they
    // are, a folder of whatever they're called or a zip.
    let found = OnceCell::new();
    let rom_named = |folder: &Path, name: &str| {
        let path = folder.join(name);
        if files::exists(&path) {
            return read(&path);
        }
        let found: &rom_set::Sorted = found.get_or_init(|| rom_set::scan(roms));
        found
            .bytes(name)
            .ok_or_else(|| format!("no {name} in {}", roms.display()))
    };
    let state = save_state::path(disks.first().map_or(roms, |disk| Path::new(disk)));
    let (mut apple, folder, own_variant) = match model {
        Model::IIPlus => {
            let zipped = roms
                .extension()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("zip"));
            let rom = if files::is_folder(roms) || zipped {
                rom_named(roms, rom_set::PLUS_ROM)?
            } else {
                read(roms)?
            };
            if rom.len() < 0x3000 {
                return Err("the ROM is 12K, $D000 to $FFFF".to_string());
            }
            let mut apple = Apple2::new(rom[rom.len() - 0x3000..].to_vec());
            apple.language_card = Some(language_card::LanguageCard::new());
            let folder = if files::is_folder(roms) {
                roms
            } else {
                roms.parent().unwrap_or(Path::new("."))
            };
            (apple, folder.to_path_buf(), Variant::Nmos6502)
        }
        Model::IIe | Model::IIeEnhanced => {
            let enhanced = model == Model::IIeEnhanced;
            let names = if enhanced {
                iie::ENHANCED_ROMS
            } else {
                iie::ROMS
            };
            let cd = rom_named(roms, names[0])?;
            let ef = rom_named(roms, names[1])?;
            let characters = rom_named(roms, names[2])?;
            let apple = Apple2::new_iie(&cd, &ef, characters);
            let variant = if enhanced {
                Variant::Synertek65C02
            } else {
                Variant::Nmos6502
            };
            let zipped = roms
                .extension()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("zip"));
            let folder = if zipped {
                roms.parent().unwrap_or(Path::new("."))
            } else {
                roms
            };
            (apple, folder.to_path_buf(), variant)
        }
    };
    apple.monitor = monitor;
    apple.clock = Some(clock_card::ClockCard::new());
    for card in apple.mockingboards.iter_mut().take(mockingboards) {
        *card = Some(mockingboard::Mockingboard::new());
    }

    // The images, the hard disk's volumes from the Disk II's.
    let mut notice = None;
    let mut images = Vec::new();
    for disk in disks {
        images.extend(disk_image::images(Path::new(disk)).map_err(|e| format!("{disk}: {e}"))?);
    }
    let (volumes, floppies): (Vec<PathBuf>, Vec<PathBuf>) =
        images.into_iter().partition(|image| is_volume(image));
    if !volumes.is_empty() {
        if volumes.len() > 2 {
            return Err(format!(
                "the hard disk has two drives, not {}",
                volumes.len()
            ));
        }
        let mut card = hard_disk::HardDisk::new();
        for (unit, volume) in card.units.iter_mut().zip(&volumes) {
            let opened = hard_disk::Volume::open(volume);
            *unit = Some(opened.map_err(|e| format!("{}: {e}", volume.display()))?);
        }
        apple.hard_disk = Some(card);
    }
    if !floppies.is_empty() {
        let card = disk2::Disk2::new(
            rom_named(&folder, rom_set::DISK_II_P5)?,
            &rom_named(&folder, rom_set::DISK_II_P6)?,
        )
        .map_err(|e| format!("the Disk II's ROMs: {e}"))?;
        apple.disk2 = Some(card);
        // Each image opened, to know it can be; the first put in drive 1.
        for image in &floppies {
            Disk::open(image).map_err(|e| format!("{}: {e}", image.display()))?;
        }
        apple.disks = floppies;
        notice = apple.change_disk(0);
    }

    let mut cpu = CPU::with_bus(apple);
    cpu.clock_hz = CLOCK_HZ;
    cpu.variant = variant.unwrap_or(own_variant);
    cpu.reset();
    Ok(Built { cpu, notice, state })
}

/// An Apple as `build` has it, run in a window, `fullscreen` or not; or
/// `text`, its text screen in the terminal.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(
    model: Model,
    roms: &str,
    disks: &[&str],
    mockingboards: usize,
    variant: Option<Variant>,
    monitor: video::Monitor,
    text: bool,
    fullscreen: bool,
) {
    let built = build(model, roms, disks, mockingboards, variant, monitor)
        .unwrap_or_else(|e| panic!("{e}"));
    let display: Box<dyn Display> = if text {
        Box::new(Terminal::new())
    } else {
        let title = built.cpu.memory.title();
        let (width, height) = (video::PICTURE_WIDTH, video::HEIGHT);
        Box::new(Window::new(&title, width, height, model.size(), fullscreen))
    };
    let mut host = Host::new(built.cpu, display, built.state);
    if let Some(text) = built.notice {
        host.say(text);
    }
    host.run();
}
