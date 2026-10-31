//! The Apple II (the II+): a 6502 at 1.023 MHz; 48K of RAM; the keyboard,
//! the speaker and the soft switches at $C000; no cards; and 12K of ROM at
//! $D000, the monitor and BASIC, from a dump of the machine's own (not
//! part of this repository). Its text screen, 40 by 24 in RAM at $0400, is
//! drawn in the host's terminal once a video frame.

use crate::bus::Bus;
use crate::terminal::Terminal;
use std::io::Write;

/// 14.31818 MHz, a fourteenth.
pub const CLOCK_HZ: u32 = 1_022_727;
/// 65 cycles a line, 262 lines.
pub const CYCLES_PER_FRAME: u32 = 17_030;
/// The flashing characters' half period.
const FLASH_FRAMES: u32 = 16;

pub struct Apple2 {
    pub ram: Vec<u8>,
    pub rom: Vec<u8>,
    /// The keyboard: the last key, bit 7 set until $C010 clears it.
    pub keyboard: u8,
    pub terminal: Option<Terminal>,
    // The soft switches.
    pub graphics: bool,
    pub mixed: bool,
    pub page2: bool,
    pub hires: bool,
    /// Cycles into the frame, and frames since power-on.
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
            graphics: false,
            mixed: false,
            page2: false,
            hires: false,
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

    /// The I/O at $C000 to $C0FF, for a read or a write: what a read gives.
    fn io(&mut self, addr: u16) -> u8 {
        match addr & 0xFFF0 {
            0xC000 => {
                if self.keyboard & 0x80 == 0 {
                    if let Some(key) = self.terminal_key() {
                        self.press(key);
                    }
                }
                self.keyboard
            }
            0xC010 => {
                self.keyboard &= 0x7F;
                self.keyboard
            }
            0xC050 if addr <= 0xC057 => {
                self.switch(addr);
                0
            }
            _ => 0,
        }
    }

    /// A text row's 40 screen bytes, on the page shown. The rows are
    /// interleaved: three of every eight to a 128-byte block.
    fn row(&self, row: usize) -> &[u8] {
        let base = if self.page2 { 0x0800 } else { 0x0400 };
        let start = base + (row % 8) * 0x80 + (row / 8) * 0x28;
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
            0xC000..=0xC0FF => self.io(addr),
            0xC100..=0xCFFF => 0, // no cards
            _ => self.rom[addr as usize - 0xD000],
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0xBFFF => self.ram[addr as usize] = value,
            0xC000..=0xC0FF => {
                self.io(addr);
            }
            _ => {} // no cards; ROM
        }
    }

    fn tick(&mut self) {
        self.cycle += 1;
        if self.cycle == CYCLES_PER_FRAME {
            self.cycle = 0;
            self.frames += 1;
            if self.terminal.is_some() {
                self.draw();
            }
        }
    }
}
