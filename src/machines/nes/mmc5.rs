//! The MMC5, mapper 5, Nintendo's biggest: Castlevania III's, and Koei's.
//! Its registers are at $5000 up, and it watches the PPU's bus as closely
//! as the PPU uses it, to do what no other mapper does.
//!
//! PRG: $5100's mode has 32K, 16K twice, 16K and 8K twice, or 8K four
//! times at $8000 up, the last's register $5117, always ROM, the rest's
//! bit 7 choosing ROM or PRG RAM, and $5113 8K of RAM at $6000. RAM's
//! written only if $5102 is 2 and $5103 1, kept here as 64K, the most any
//! board has, whatever a game's has: each uses it as its own would be.
//!
//! CHR: $5101's mode has 8K, 4K, 2K or 1K, each register a bank of that
//! size, of 10 bits, $5130's two with the write. $5120 to $5127 are the
//! sprites', and everything's with 8 by 8 sprites. With 8 by 16 sprites,
//! which it sees written to $2000, the background has $5128 to $512B, and
//! so does PPUDATA, if they were last written. It knows a sprite's fetch
//! by when it comes; here the PPU says.
//!
//! The nametables: $5105 says of each 1K of them, the PPU's 2K, a page of
//! its own 1K of ExRAM, or a fill, $5106's tile everywhere and $5107's
//! palette. ExRAM is at $5C00 up: by $5104's mode, nametable, written only
//! while rendering (0); that with extended attributes (1); plain RAM (2);
//! or read only (3). Extended attributes are a byte for each tile, of the
//! nametable the first seen of it, read as its tile's: its palette, and a
//! 4K bank of CHR, $5130's bits above them.
//!
//! The vertical split ($5200): tiles to one side of a count are drawn
//! instead from ExRAM as a nametable, at the scroll $5201 and from the 4K
//! bank $5202.
//!
//! The scanline IRQ: three reads of one nametable address (as the PPU
//! does, a line's two unused then the next's first), and the read after
//! them is a new line. The first starts the frame, in frame set, the line
//! 0; each after it counts a line, and at $5203's sets the pending flag,
//! which $5204 reads and clears; with $5204's enable, the IRQ's held. Three
//! CPU cycles without a PPU read end the frame, as $2001 clearing the
//! rendering bits does, and reading the NMI's vector.
//!
//! Its multiplier: $5205 and $5206 written, read back the product.
//!
//! Left out: its sound (at $5000 to $5015), and the MMC5A's timer and pins.

use serde::{Deserialize, Serialize};

/// PRG RAM, kept as all a board can have.
pub const PRG_RAM: usize = 0x10000;

const EXRAM: usize = 0x400;
/// A nametable's attribute table, in it.
const ATTRIBUTES: usize = 0x3C0;

/// Where a CPU address is in the PRG, by the banks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Rom(usize),
    Ram(usize),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mmc5 {
    prg_mode: u8,
    chr_mode: u8,
    ram_protect: [u8; 2],
    exram_mode: u8,
    nametables: u8,
    fill_tile: u8,
    fill_palette: u8,
    /// $5113 to $5117.
    prg: [u8; 5],
    /// $5120 to $512B, and whether the last written were the last four.
    chr: [u16; 12],
    chr_upper: u8,
    last_chr_b: bool,
    split: u8,
    split_scroll: u8,
    split_bank: u8,
    irq_line: u8,
    irq_enabled: bool,
    irq_pending: bool,
    factors: [u8; 2],
    exram: Vec<u8>,
    // What it's seen of the CPU's writes to the PPU: its 8 by 16 sprites,
    // and rendering.
    tall_sprites: bool,
    rendering: bool,
    // The scanline detector: the last address read, the matches, whether
    // the next read's a line; in frame; the line.
    last_read: Option<u16>,
    matches: u8,
    line_next: bool,
    in_frame: bool,
    line: u8,
    /// Whether the PPU's read since the last CPU cycle, and the cycles it
    /// hasn't.
    reading: bool,
    idle: u8,
    // This tile's: how many of the line's fetched, the last's address; a
    // 4K bank instead of the banks; and its attribute's palette, if it has
    // its own; whether it's in the split.
    tiles: u8,
    tile_address: Option<u16>,
    tile_bank: Option<usize>,
    tile_palette: Option<u8>,
    in_split: bool,
}

impl Mmc5 {
    pub fn new() -> Self {
        Mmc5 {
            prg_mode: 3,
            chr_mode: 0,
            ram_protect: [1, 2],
            exram_mode: 3,
            nametables: 0,
            fill_tile: 0,
            fill_palette: 0,
            prg: [0xFF; 5],
            chr: [0; 12],
            chr_upper: 0,
            last_chr_b: false,
            split: 0,
            split_scroll: 0,
            split_bank: 0,
            irq_line: 0,
            irq_enabled: false,
            irq_pending: false,
            factors: [0xFF; 2],
            exram: vec![0; EXRAM],
            tall_sprites: false,
            rendering: false,
            last_read: None,
            matches: 0,
            line_next: false,
            in_frame: false,
            line: 0,
            reading: false,
            idle: 0,
            tiles: 0,
            tile_address: None,
            tile_bank: None,
            tile_palette: None,
            in_split: false,
        }
    }

    /// Where `addr`, $6000 up, is, by the mode and banks.
    fn place(&self, addr: u16) -> Place {
        let at = addr as usize;
        // A register's bank, of `size` in 8Ks, the rest of the address.
        let window = |register: usize, size: usize| {
            let bank = self.prg[register] as usize;
            let offset = (bank & !(size - 1) & 0x7F) * 0x2000 + (at & (size * 0x2000 - 1));
            if bank & 0x80 != 0 || register == 4 {
                Place::Rom(offset)
            } else {
                Place::Ram(
                    ((bank & 7 & !(size - 1)) * 0x2000 + (at & (size * 0x2000 - 1))) % PRG_RAM,
                )
            }
        };
        match (self.prg_mode, addr) {
            (_, 0x6000..=0x7FFF) => Place::Ram((self.prg[0] as usize & 7) * 0x2000 + (at & 0x1FFF)),
            (0, _) => window(4, 4),
            (1, 0x8000..=0xBFFF) => window(2, 2),
            (1, _) => window(4, 2),
            (2, 0x8000..=0xBFFF) => window(2, 2),
            (2, 0xC000..=0xDFFF) => window(3, 1),
            (2, _) => window(4, 1),
            (_, 0x8000..=0x9FFF) => window(1, 1),
            (_, 0xA000..=0xBFFF) => window(2, 1),
            (_, 0xC000..=0xDFFF) => window(3, 1),
            _ => window(4, 1),
        }
    }

    /// Where each 4K of $8000 up is in `len` of ROM, if that's where they
    /// all are.
    pub fn rom_pages(&self, len: usize) -> Option<[u32; 8]> {
        let mut pages = [0; 8];
        for (page, offset) in pages.iter_mut().enumerate() {
            match self.place(0x8000 + page as u16 * 0x1000) {
                Place::Rom(at) if len % 0x1000 == 0 => *offset = (at % len) as u32,
                _ => return None,
            }
        }
        Some(pages)
    }

    fn ram_writable(&self) -> bool {
        self.ram_protect == [2, 1]
    }

    /// A read of $5000 up: `rom` and `ram` its PRG's; None's open bus.
    pub fn cpu_read(&mut self, addr: u16, rom: &[u8], ram: &[u8]) -> Option<u8> {
        match addr {
            0x5204 => {
                let status = (self.irq_pending as u8) << 7 | (self.in_frame as u8) << 6;
                self.irq_pending = false;
                Some(status)
            }
            0x5205 | 0x5206 => {
                let product = self.factors[0] as u16 * self.factors[1] as u16;
                Some(product.to_le_bytes()[addr as usize - 0x5205])
            }
            0x5C00..=0x5FFF if self.exram_mode >= 2 => Some(self.exram[addr as usize & 0x3FF]),
            0x6000..=0xFFFF => {
                if matches!(addr, 0xFFFA | 0xFFFB) {
                    self.frame_over();
                    self.irq_pending = false;
                }
                match self.place(addr) {
                    Place::Rom(at) => Some(rom[at % rom.len()]),
                    Place::Ram(at) => ram.get(at).copied(),
                }
            }
            _ => None,
        }
    }

    /// A write of $5000 up: true if it was to `ram`.
    pub fn cpu_write(&mut self, addr: u16, value: u8, ram: &mut [u8]) -> bool {
        match addr {
            0x5100 => self.prg_mode = value & 3,
            0x5101 => self.chr_mode = value & 3,
            0x5102 | 0x5103 => self.ram_protect[addr as usize - 0x5102] = value & 3,
            0x5104 => self.exram_mode = value & 3,
            0x5105 => self.nametables = value,
            0x5106 => self.fill_tile = value,
            0x5107 => self.fill_palette = value & 3,
            0x5113..=0x5117 => self.prg[addr as usize - 0x5113] = value,
            0x5120..=0x512B => {
                let register = addr as usize - 0x5120;
                self.chr[register] = value as u16 | (self.chr_upper as u16) << 8;
                self.last_chr_b = register >= 8;
            }
            0x5130 => self.chr_upper = value & 3,
            0x5200 => self.split = value,
            0x5201 => self.split_scroll = value,
            0x5202 => self.split_bank = value,
            0x5203 => self.irq_line = value,
            0x5204 => self.irq_enabled = value & 0x80 != 0,
            0x5205 | 0x5206 => self.factors[addr as usize - 0x5205] = value,
            0x5C00..=0x5FFF => {
                let writable = match self.exram_mode {
                    0 | 1 => self.in_frame,
                    2 => true,
                    _ => false,
                };
                if writable {
                    self.exram[addr as usize & 0x3FF] = value;
                }
            }
            0x6000..=0xFFFF => {
                if let (Place::Ram(at), true) = (self.place(addr), self.ram_writable()) {
                    if let Some(byte) = ram.get_mut(at) {
                        *byte = value;
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// What it sees of a CPU write to the PPU's $2000, $2001, or $4014.
    pub fn ppu_register_written(&mut self, addr: u16, value: u8) {
        match addr {
            0x2000 => self.tall_sprites = value & 0x20 != 0,
            0x2001 => {
                self.rendering = value & 0x18 != 0;
                if !self.rendering {
                    self.frame_over();
                }
            }
            0x4014 => self.frame_over(),
            _ => {}
        }
    }

    fn frame_over(&mut self) {
        self.in_frame = false;
        self.last_read = None;
        self.matches = 0;
        self.line_next = false;
    }

    /// A PPU read of `addr`: looked for what it says of the line.
    pub fn ppu_read(&mut self, addr: u16) {
        self.reading = true;
        if self.line_next {
            self.line_next = false;
            self.new_line();
        }
        if (0x2000..0x3000).contains(&addr) && self.last_read == Some(addr) {
            self.matches += 1;
            self.line_next = self.matches == 2;
        } else {
            self.matches = 0;
        }
        self.last_read = Some(addr);
    }

    /// The PPU's fetching, this cycle, without saying what.
    pub fn fetching(&mut self) {
        self.reading = true;
    }

    fn new_line(&mut self) {
        if !self.in_frame {
            self.in_frame = true;
            self.line = 0;
            self.irq_pending = false;
            return;
        }
        self.line = self.line.wrapping_add(1);
        if self.line == self.irq_line && self.irq_line != 0 {
            self.irq_pending = true;
        }
    }

    /// A CPU cycle: three of them with no PPU read end the frame.
    pub fn cycle(&mut self) {
        if self.reading {
            self.idle = 0;
        } else {
            self.idle = self.idle.saturating_add(1);
            if self.idle == 3 {
                self.frame_over();
            }
        }
        self.reading = false;
    }

    pub fn irq(&self) -> bool {
        self.irq_pending && self.irq_enabled
    }

    /// Reset: all it does is disable the IRQ.
    pub fn reset(&mut self) {
        self.irq_enabled = false;
    }

    /// Whether it's substituting what's fetched: drawing, and rendering.
    fn substituting(&self) -> bool {
        self.in_frame && self.rendering
    }

    /// Whether the PPU's fetches are only what they seem: no split, no
    /// extended attributes, no nametable of its own, so that all it has to
    /// do with them is count lines.
    pub fn plain_fetches(&self) -> bool {
        self.exram_mode != 1
            && (self.split & 0x80 == 0 || self.exram_mode > 1)
            && self.nametables & 0xAA == 0
    }

    /// Whether the tile `tiles` of the line is in the split.
    fn split_has(&self, tile: u8) -> bool {
        let (enabled, right, count) = (
            self.split & 0x80 != 0,
            self.split & 0x40 != 0,
            self.split & 0x1F,
        );
        enabled && self.exram_mode <= 1 && if right { tile >= count } else { tile < count }
    }

    /// The nametable each 1K of them is: 0 or 1 the PPU's, 2 ExRAM, 3 fill.
    fn source(&self, addr: u16) -> u8 {
        (self.nametables >> (2 * ((addr as usize >> 10) & 3))) & 3
    }

    /// The page of the PPU's RAM `addr` is in, if it's in it.
    pub fn nametable_page(&self, addr: u16) -> usize {
        self.source(addr) as usize & 1
    }

    /// A PPU read of a nametable, if it's the MMC5's to answer.
    pub fn nametable_read(&mut self, addr: u16) -> Option<u8> {
        let at = addr as usize & 0x3FF;
        let attribute = at >= ATTRIBUTES;
        if !attribute && self.tile_address != Some(addr) {
            self.tile_address = Some(addr);
            self.next_tile(at);
        }
        if self.substituting() {
            if let (true, Some(palette)) = (attribute, self.tile_palette) {
                return Some(palette * 0x55);
            }
            if self.in_split && !attribute {
                return Some(self.exram[self.split_row() * 32 + self.split_column()]);
            }
        }
        match self.source(addr) {
            2 if self.exram_mode <= 1 => Some(self.exram[at]),
            2 => Some(0),
            3 if attribute => Some(self.fill_palette * 0x55),
            3 => Some(self.fill_tile),
            _ => None,
        }
    }

    /// The split's nametable row and column, this line and tile.
    fn split_row(&self) -> usize {
        ((self.split_scroll as usize + self.line as usize) % 240) / 8
    }

    fn split_column(&self) -> usize {
        self.tiles.saturating_sub(1) as usize % 32
    }

    /// The tile at `at` of a nametable's coming: what it's to be drawn with.
    fn next_tile(&mut self, at: usize) {
        self.tiles = self.tiles.saturating_add(1);
        self.tile_bank = None;
        self.tile_palette = None;
        self.in_split = false;
        if !self.substituting() {
            return;
        }
        if self.split_has(self.tiles - 1) {
            self.in_split = true;
            self.tile_bank = Some(self.split_bank as usize);
            let (row, column) = (self.split_row(), self.split_column());
            let attribute = self.exram[ATTRIBUTES + (row / 4) * 8 + column / 4];
            let shift = (column & 2) | ((row & 2) << 1);
            self.tile_palette = Some((attribute >> shift) & 3);
        } else if self.exram_mode == 1 {
            let byte = self.exram[at] as usize;
            self.tile_bank = Some((byte & 0x3F) | (self.chr_upper as usize) << 6);
            self.tile_palette = Some(byte as u8 >> 6);
        }
    }

    /// A PPU write of a nametable: true if it's the MMC5's.
    pub fn nametable_write(&mut self, addr: u16, value: u8) -> bool {
        match self.source(addr) {
            2 => {
                if self.exram_mode <= 1 {
                    self.exram[addr as usize & 0x3FF] = value;
                }
                true
            }
            3 => true,
            _ => false,
        }
    }

    /// Where CHR address `addr` is, `sprite` a sprite's fetch.
    pub fn chr_offset(&mut self, addr: usize, sprite: bool) -> usize {
        if sprite {
            self.tiles = 0;
            self.tile_address = None;
        }
        if let (false, true, Some(bank)) = (sprite, self.substituting(), self.tile_bank) {
            return bank * 0x1000 + (addr & 0x0FFF);
        }
        let tall = self.tall_sprites && self.rendering;
        let set_b = tall && !sprite && (self.in_frame || self.last_chr_b);
        let slot = addr >> 10;
        let (register, size) = match (self.chr_mode, set_b) {
            (0, false) => (7, 0x2000),
            (0, true) => (11, 0x2000),
            (1, false) => (if addr < 0x1000 { 3 } else { 7 }, 0x1000),
            (1, true) => (11, 0x1000),
            (2, false) => ([1, 3, 5, 7][slot >> 1], 0x800),
            (2, true) => ([9, 11][(slot >> 1) & 1], 0x800),
            (_, false) => (slot, 0x400),
            (_, true) => (8 + (slot & 3), 0x400),
        };
        self.chr[register] as usize * size + (addr & (size - 1))
    }
}
