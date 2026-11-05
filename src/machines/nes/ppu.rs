//! The PPU, the 2C02: the NES's picture, made a dot at a time, three a
//! CPU cycle; 341 dots a line, 262 lines a frame. Lines 0 to 239 are seen,
//! a dot of each of 256 made on dots 1 to 256; 240 is idle; the vertical
//! blank starts on 241, its flag set (and an NMI, if it's enabled) on its
//! dot 1; and 261, the pre-render line, ends it, clears the flags on its
//! dot 1, and fetches what line 0 begins with. On odd frames, rendering,
//! the pre-render line is a dot short. The PAL 2C07's the same but for its
//! region's timing: 312 lines, 311 the pre-render line, never a dot short;
//! and the black border it draws, its first line and two dots at each side.
//!
//! Its memory, its own bus: the cartridge's pattern tables, $0000 to
//! $1FFF; the nametables, $2000 to $2FFF, its 2K of RAM as the cartridge
//! wires them (and $3000 to $3EFF again); and its palette, 32 bytes, at
//! $3F00, its sprites' backdrop entries the background's.
//!
//! The background: each line its tiles fetched, 8 dots a tile, a tile
//! ahead (the next line's first two at the line's end): the nametable's
//! byte, the attribute's, then the pattern's two planes. Into shift
//! registers, which shift a dot at a time, the fine X scroll picking the
//! bit that's the dot's. Its address is the scroll's, "loopy's" v: coarse
//! X and Y, the nametable, fine Y; moved on a tile across as each is
//! fetched, a line down at dot 256, back to the left (from t) at 257, and
//! on the pre-render line back to the top (from t) at 280 to 304.
//!
//! The sprites: each line, the 64 in OAM looked through for the 8 (at
//! most) on the next line, the next line's fetched at dots 257 to 320; a
//! ninth found sets the overflow flag, but by the PPU's bug, its search
//! goes on looking at the wrong bytes. Sprite 0's dot drawn over a
//! background's, neither of them clear, sets the sprite 0 hit flag.
//!
//! The CPU's registers, $2000 to $2007: PPUCTRL, PPUMASK, PPUSTATUS,
//! OAMADDR, OAMDATA, PPUSCROLL, PPUADDR and PPUDATA. Reading PPUSTATUS
//! clears the vertical blank flag and the write toggle; PPUDATA's reads
//! are a read behind, from a buffer, but for the palette's. What's read
//! of a register without all of its bits, or of one only written, is the
//! registers' bus: what was last on it, a write's or a read's bits, each
//! bit decaying to 0 if it isn't driven 1 again for 600 ms or so.

use super::cartridge::Cartridge;
use super::region::Region;
use crate::host::save_state;
use serde::{Deserialize, Serialize};

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;
pub const DOTS: u16 = 341;
const VBLANK: u16 = 241;
/// How long a bit on the registers' bus lasts, not driven 1, in NTSC's
/// frames and PAL's: 600 ms.
const DECAY_FRAMES: [u32; 2] = [36, 30];
/// The border's colour, black.
const BORDER: u16 = 0x0E;

/// A sprite fetched for the line: its X, its pattern's two planes (turned
/// round if flipped), its attributes.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
struct Sprite {
    x: u8,
    low: u8,
    high: u8,
    attributes: u8,
}

#[derive(Serialize, Deserialize)]
pub struct Ppu {
    /// Whose timing it has: in the machine it's loaded into, not a state.
    #[serde(skip)]
    pub region: Region,
    ctrl: u8,
    mask: u8,
    vblank: bool,
    sprite_zero_hit: bool,
    overflow: bool,
    oam_addr: u8,
    #[serde(with = "save_state::bytes")]
    oam: [u8; 256],
    // The scroll: the address, the address to be, fine X, the write toggle.
    v: u16,
    t: u16,
    fine_x: u8,
    w: bool,
    /// PPUDATA's buffer; the registers' bus, and the frame each of its bits
    /// was last driven 1 in; the frames.
    buffer: u8,
    latch: u8,
    driven: [u32; 8],
    frames: u32,
    /// Its 2K of nametable RAM, and the 2K more a four-screen cartridge
    /// has; the palette.
    nametables: Vec<u8>,
    palette: [u8; 32],
    /// The line and dot to come, and whether this frame is odd; and the
    /// dots since power-on.
    pub line: u16,
    pub dot: u16,
    dots: u64,
    odd: bool,
    short_line: bool,
    /// PPUSTATUS read the dot before the vertical blank's: its flag not set
    /// this frame.
    suppress_vblank: bool,
    /// Writes to PPUCTRL, PPUMASK, PPUSCROLL and PPUADDR do nothing until
    /// the first frame's pre-render line, after power-on or reset.
    warming_up: bool,
    // The background: the tile being fetched; the shift registers.
    tile: u8,
    attribute: u8,
    pattern_low: u8,
    pattern_high: u8,
    shift_low: u16,
    shift_high: u16,
    shift_attribute_low: u16,
    shift_attribute_high: u16,
    // The sprites: the next line's found, in secondary OAM, how many, and
    // whether sprite 0 is; this line's, fetched.
    secondary: [u8; 32],
    found: usize,
    /// The dot the overflow flag is set at, this line, if it is.
    overflow_at: Option<u16>,
    zero_found: bool,
    sprites: [Sprite; 8],
    sprite_count: usize,
    zero_on_line: bool,
    /// The picture as far as this frame has drawn it: each dot's colour,
    /// 6 bits, and the emphasis, 3 more.
    #[serde(skip)]
    pub frame: Vec<u16>,
    /// A frame finished, to be shown.
    #[serde(skip)]
    pub frame_ready: bool,
}

impl Ppu {
    pub fn new(region: Region) -> Self {
        Ppu {
            region,
            ctrl: 0,
            mask: 0,
            vblank: false,
            sprite_zero_hit: false,
            overflow: false,
            oam_addr: 0,
            oam: [0; 256],
            v: 0,
            t: 0,
            fine_x: 0,
            w: false,
            buffer: 0,
            latch: 0,
            driven: [0; 8],
            frames: 0,
            nametables: vec![0; 0x1000],
            palette: [0; 32],
            line: 0,
            dot: 0,
            dots: 0,
            odd: false,
            short_line: false,
            suppress_vblank: false,
            warming_up: true,
            tile: 0,
            attribute: 0,
            pattern_low: 0,
            pattern_high: 0,
            shift_low: 0,
            shift_high: 0,
            shift_attribute_low: 0,
            shift_attribute_high: 0,
            secondary: [0xFF; 32],
            found: 0,
            overflow_at: None,
            zero_found: false,
            sprites: [Sprite::default(); 8],
            sprite_count: 0,
            zero_on_line: false,
            frame: vec![0; WIDTH * HEIGHT],
            frame_ready: false,
        }
    }

    /// RESET: PPUCTRL and PPUMASK cleared, the write toggle and the buffer;
    /// writes ignored again, until the pre-render line.
    pub fn reset(&mut self) {
        self.ctrl = 0;
        self.mask = 0;
        self.w = false;
        self.buffer = 0;
        self.warming_up = true;
    }

    /// The NMI line: pulled while the vertical blank flag and PPUCTRL's
    /// NMI enable both are.
    pub fn nmi(&self) -> bool {
        self.vblank && self.ctrl & 0x80 != 0
    }

    fn rendering(&self) -> bool {
        self.mask & 0x18 != 0
    }

    fn sprite_height(&self) -> i16 {
        if self.ctrl & 0x20 != 0 { 16 } else { 8 }
    }

    /// Its own bus, read: the address on it the cartridge's to see, but the
    /// palette's, inside the PPU.
    fn read(&mut self, addr: u16, cart: &mut Cartridge) -> u8 {
        let addr = addr & 0x3FFF;
        if addr < 0x3F00 {
            cart.ppu_bus(addr, self.dots);
        }
        match addr {
            0x0000..=0x1FFF => cart.chr_read(addr),
            0x2000..=0x3EFF => self.nametables[cart.mirroring.page(addr) * 0x400 + (addr as usize & 0x3FF)],
            _ => self.palette[palette_index(addr)],
        }
    }

    fn write(&mut self, addr: u16, value: u8, cart: &mut Cartridge) {
        let addr = addr & 0x3FFF;
        if addr < 0x3F00 {
            cart.ppu_bus(addr, self.dots);
        }
        match addr {
            0x0000..=0x1FFF => cart.chr_write(addr, value),
            0x2000..=0x3EFF => self.nametables[cart.mirroring.page(addr) * 0x400 + (addr as usize & 0x3FF)] = value,
            _ => self.palette[palette_index(addr)] = value & 0x3F,
        }
    }

    /// A register read, by the CPU: $2000 to $2007, `register` 0 to 7.
    pub fn read_register(&mut self, register: u16, cart: &mut Cartridge) -> u8 {
        let value = match register & 7 {
            2 => {
                let flags = (self.vblank as u8) << 7 | (self.sprite_zero_hit as u8) << 6 | (self.overflow as u8) << 5;
                self.vblank = false;
                if self.line == VBLANK && self.dot == 1 {
                    self.suppress_vblank = true;
                }
                self.w = false;
                self.drive(flags, 0xE0)
            }
            4 => {
                let byte = self.oam[self.oam_addr as usize];
                // An attribute byte's bits 2 to 4 aren't there.
                self.drive(if self.oam_addr & 3 == 2 { byte & 0xE3 } else { byte }, 0xFF)
            }
            7 => {
                let addr = self.v & 0x3FFF;
                let value = if addr >= 0x3F00 {
                    // The palette's, at once, the buffer the nametable's
                    // under it; its top two bits the bus's.
                    self.buffer = self.read(addr - 0x1000, cart);
                    let colour = self.read(addr, cart);
                    self.drive(colour, 0x3F)
                } else {
                    let value = self.buffer;
                    self.buffer = self.read(addr, cart);
                    self.drive(value, 0xFF)
                };
                self.next_address(cart);
                value
            }
            _ => self.latch,
        };
        value
    }

    /// `value`'s `bits` driven on the registers' bus, the others as they
    /// are: what's read.
    fn drive(&mut self, value: u8, bits: u8) -> u8 {
        self.latch = (self.latch & !bits) | (value & bits);
        for bit in 0..8 {
            if (bits & value) >> bit & 1 != 0 {
                self.driven[bit] = self.frames;
            }
        }
        self.latch
    }

    /// A register written, by the CPU.
    pub fn write_register(&mut self, register: u16, value: u8, cart: &mut Cartridge) {
        self.drive(value, 0xFF);
        match register & 7 {
            0 if !self.warming_up => {
                self.ctrl = value;
                self.t = (self.t & !0x0C00) | ((value as u16 & 3) << 10);
            }
            1 if !self.warming_up => self.mask = value,
            3 => self.oam_addr = value,
            4 => {
                self.oam[self.oam_addr as usize] = value;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            5 if !self.warming_up => {
                if !self.w {
                    self.t = (self.t & !0x001F) | (value as u16 >> 3);
                    self.fine_x = value & 7;
                } else {
                    self.t = (self.t & !0x73E0) | ((value as u16 & 7) << 12) | ((value as u16 & 0xF8) << 2);
                }
                self.w = !self.w;
            }
            6 if !self.warming_up => {
                if !self.w {
                    self.t = (self.t & 0x00FF) | ((value as u16 & 0x3F) << 8);
                } else {
                    self.t = (self.t & 0xFF00) | value as u16;
                    self.v = self.t;
                    cart.ppu_bus(self.v & 0x3FFF, self.dots);
                }
                self.w = !self.w;
            }
            7 => {
                self.write(self.v, value, cart);
                self.next_address(cart);
            }
            _ => {}
        }
    }

    /// OAM DMA's byte, written to OAMDATA.
    pub fn dma_write(&mut self, value: u8) {
        self.oam[self.oam_addr as usize] = value;
        self.oam_addr = self.oam_addr.wrapping_add(1);
    }

    /// PPUDATA's address moved on after an access: by 1, or 32 a line
    /// down, the new address on its bus, as it is while it's idle; or,
    /// while it renders, as rendering moves it, a tile across and a line
    /// down at once.
    fn next_address(&mut self, cart: &mut Cartridge) {
        if self.rendering() && (self.line < 240 || self.line == self.pre_render()) {
            self.next_tile();
            self.next_line();
        } else {
            self.v = (self.v + if self.ctrl & 0x04 != 0 { 32 } else { 1 }) & 0x7FFF;
            cart.ppu_bus(self.v & 0x3FFF, self.dots);
        }
    }

    /// v a tile across: coarse X on, into the next nametable across past
    /// the 32nd.
    fn next_tile(&mut self) {
        if self.v & 0x001F == 31 {
            self.v = (self.v & !0x001F) ^ 0x0400;
        } else {
            self.v += 1;
        }
    }

    /// v a line down: fine Y on, then coarse Y, into the nametable below
    /// past the 30th row (but not past rows 30 and 31, the attributes').
    fn next_line(&mut self) {
        if self.v & 0x7000 != 0x7000 {
            self.v += 0x1000;
            return;
        }
        self.v &= !0x7000;
        let mut y = (self.v & 0x03E0) >> 5;
        if y == 29 {
            y = 0;
            self.v ^= 0x0800;
        } else if y == 31 {
            y = 0;
        } else {
            y += 1;
        }
        self.v = (self.v & !0x03E0) | (y << 5);
    }

    /// The pre-render line, a frame's last.
    fn pre_render(&self) -> u16 {
        self.region.lines() - 1
    }

    /// A dot.
    pub fn tick(&mut self, cart: &mut Cartridge) {
        let (line, dot) = (self.line, self.dot);
        let seen = line < 240;
        let pre_render = line == self.pre_render();

        if line == VBLANK && dot == 1 {
            self.vblank = !self.suppress_vblank;
            self.suppress_vblank = false;
            self.frame_ready = true;
            // The bus's bits not driven 1 lately, decayed.
            self.frames += 1;
            for bit in 0..8 {
                if self.frames - self.driven[bit] > DECAY_FRAMES[self.region as usize] {
                    self.latch &= !(1 << bit);
                }
            }
        }
        if pre_render && dot == 1 {
            self.vblank = false;
            self.sprite_zero_hit = false;
            self.overflow = false;
            self.warming_up = false;
        }

        if self.rendering() && (seen || pre_render) {
            self.background(cart);
            self.sprite_fetches(cart);
            if seen && dot == 65 {
                self.evaluate_sprites();
            }
            if seen && self.overflow_at == Some(dot) {
                self.overflow = true;
            }
        }
        if seen && (1..=256).contains(&dot) {
            self.pixel();
        }

        // On, a dot; the pre-render line a dot short on odd frames, if
        // it's rendering as of dot 338.
        if pre_render && dot == 338 {
            self.short_line = self.odd && self.rendering() && self.region == Region::Ntsc;
        }
        if pre_render && dot == 339 && self.short_line {
            self.dot = 340;
        }
        self.dot += 1;
        self.dots += 1;
        if self.dot == DOTS {
            self.dot = 0;
            self.line += 1;
            if self.line == self.region.lines() {
                self.line = 0;
                self.odd = !self.odd;
            }
        }
    }

    /// The background's fetches and shifts, for a dot of a line rendered;
    /// and v moved on as they move it.
    fn background(&mut self, cart: &mut Cartridge) {
        let dot = self.dot;
        if (2..=257).contains(&dot) || (321..=337).contains(&dot) {
            self.shift_low <<= 1;
            self.shift_high <<= 1;
            self.shift_attribute_low <<= 1;
            self.shift_attribute_high <<= 1;
            match (dot - 1) % 8 {
                0 => {
                    self.load_shifters();
                    self.tile = self.read(0x2000 | (self.v & 0x0FFF), cart);
                }
                2 => {
                    let v = self.v;
                    let addr = 0x23C0 | (v & 0x0C00) | ((v >> 4) & 0x38) | ((v >> 2) & 0x07);
                    let shift = ((v >> 4) & 4) | (v & 2);
                    self.attribute = (self.read(addr, cart) >> shift) & 3;
                }
                4 => self.pattern_low = self.read(self.pattern_address(), cart),
                6 => self.pattern_high = self.read(self.pattern_address() + 8, cart),
                7 => self.next_tile(),
                _ => {}
            }
        }
        match dot {
            256 => self.next_line(),
            257 => self.v = (self.v & !0x041F) | (self.t & 0x041F),
            280..=304 if self.line == self.pre_render() => self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0),
            // Two nametable fetches more, their bytes not used.
            338 | 340 => {
                self.read(0x2000 | (self.v & 0x0FFF), cart);
            }
            _ => {}
        }
    }

    /// The background tile's pattern's first plane's address: its table's,
    /// as PPUCTRL picks it, the tile's, fine Y's row of it.
    fn pattern_address(&self) -> u16 {
        let table = if self.ctrl & 0x10 != 0 { 0x1000 } else { 0 };
        table | (self.tile as u16) << 4 | (self.v >> 12)
    }

    /// The tile fetched into the shift registers' low bytes, its attribute
    /// each of its 8 dots'.
    fn load_shifters(&mut self) {
        self.shift_low = (self.shift_low & 0xFF00) | self.pattern_low as u16;
        self.shift_high = (self.shift_high & 0xFF00) | self.pattern_high as u16;
        let fill = |bit: u8| if bit != 0 { 0xFF } else { 0x00 };
        self.shift_attribute_low = (self.shift_attribute_low & 0xFF00) | fill(self.attribute & 1);
        self.shift_attribute_high = (self.shift_attribute_high & 0xFF00) | fill(self.attribute & 2);
    }

    /// The sprites looked through, dots 65 to 256, for the next line's,
    /// from wherever OAMADDR is at dot 65: 2 dots for each looked at, 6
    /// more for each of the first 8 on the line copied into secondary OAM,
    /// OAMADDR on 1 a byte copied; a sprite not on it, OAMADDR on 4 and its
    /// low 2 bits cleared (realigning it, if it wasn't). The first looked
    /// at, if it's on the line, is sprite 0 for sprite 0 hit. A ninth sets
    /// the overflow flag, at the dot it's found; but past the eighth, by
    /// the PPU's bug, OAMADDR goes on 4 and 1 more without carrying, taking
    /// each sprite's next byte as its Y. Done once OAMADDR passes $FF.
    fn evaluate_sprites(&mut self) {
        let height = self.sprite_height();
        let on_line = |y: u8| (0..height).contains(&(self.line as i16 - y as i16));
        self.secondary = [0xFF; 32];
        self.zero_found = false;
        self.overflow_at = None;
        let (mut addr, mut found, mut dot) = (self.oam_addr as usize, 0, 65);
        while dot <= 256 {
            let on = on_line(self.oam[addr]);
            let next = if found < 8 {
                if on {
                    for byte in 0..4 {
                        self.secondary[found * 4 + byte] = self.oam[(addr + byte) & 0xFF];
                    }
                    self.zero_found |= dot == 65;
                    found += 1;
                    dot += 6;
                    addr + 4
                } else {
                    (addr + 4) & !3
                }
            } else if on {
                self.overflow_at = Some(dot);
                break;
            } else {
                ((addr & !3) + 4) | ((addr + 1) & 3)
            };
            dot += 2;
            if next > 0xFF {
                break;
            }
            addr = next;
        }
        self.found = found;
    }

    /// The next line's sprites fetched, dots 257 to 320, a sprite each 8,
    /// its pattern's planes on its fifth and seventh: OAMADDR held at 0
    /// meanwhile. Each of the 8 has its pattern fetched, those not found
    /// tile $FF's.
    fn sprite_fetches(&mut self, cart: &mut Cartridge) {
        let dot = self.dot;
        if !(257..=320).contains(&dot) {
            return;
        }
        self.oam_addr = 0;
        if dot == 257 {
            let found = if self.line == self.pre_render() { 0 } else { self.found };
            self.sprite_count = found;
            self.zero_on_line = self.zero_found && found > 0;
        }
        let (slot, step) = ((dot - 257) as usize / 8, (dot - 257) % 8);
        if step != 4 && step != 6 {
            return;
        }
        let entry = &self.secondary[slot * 4..slot * 4 + 4];
        let (y, tile, attributes, x) = (entry[0], entry[1], entry[2], entry[3]);
        let height = self.sprite_height() as u16;
        let mut row = (self.line as i16 - y as i16) as u16 & (height - 1);
        if attributes & 0x80 != 0 {
            row = height - 1 - row;
        }
        let addr = if height == 16 {
            let table = (tile as u16 & 1) * 0x1000;
            table | ((tile as u16 & 0xFE) + (row >> 3)) << 4 | (row & 7)
        } else {
            let table = if self.ctrl & 0x08 != 0 { 0x1000 } else { 0 };
            table | (tile as u16) << 4 | row
        };
        let mut plane = self.read(addr + if step == 6 { 8 } else { 0 }, cart);
        if slot >= self.sprite_count {
            return;
        }
        if attributes & 0x40 != 0 {
            plane = plane.reverse_bits();
        }
        let sprite = &mut self.sprites[slot];
        if step == 4 {
            *sprite = Sprite { x, low: plane, high: 0, attributes };
        } else {
            sprite.high = plane;
        }
    }

    /// A dot of a line seen: the background's and the first sprite's there
    /// not clear, either in front; or neither, the backdrop. With rendering
    /// off, the backdrop, or the palette's colour v is at if it's there.
    fn pixel(&mut self) {
        let x = self.dot as usize - 1;
        let colour = if !self.rendering() {
            let addr = if self.v & 0x3F00 == 0x3F00 { self.v } else { 0x3F00 };
            self.palette[palette_index(addr)]
        } else {
            let (background, palette) = self.background_dot(x);
            let sprite = self.sprite_dot(x);
            if let Some((zero, _, _, _)) = sprite {
                if zero && background != 0 && x != 255 {
                    self.sprite_zero_hit = true;
                }
            }
            let index = match sprite {
                Some((_, colour, sprite_palette, behind)) if background == 0 || !behind => 0x10 | sprite_palette << 2 | colour,
                _ if background != 0 => palette << 2 | background,
                _ => 0,
            };
            self.palette[palette_index(0x3F00 | index as u16)]
        };
        let grey = if self.mask & 0x01 != 0 { 0x30 } else { 0x3F };
        let emphasis = (self.mask as u16 >> 5) << 6;
        let border = self.region == Region::Pal && (self.line == 0 || !(2..WIDTH - 2).contains(&x));
        self.frame[self.line as usize * WIDTH + x] = if border { BORDER } else { (colour & grey) as u16 | emphasis };
    }

    /// The background's dot at `x`: its colour, 0 to 3, and its palette;
    /// clear where it's off, or in the leftmost 8 hidden.
    fn background_dot(&self, x: usize) -> (u8, u8) {
        if self.mask & 0x08 == 0 || (x < 8 && self.mask & 0x02 == 0) {
            return (0, 0);
        }
        let bit = 15 - self.fine_x as u16;
        let pick = |shift: u16| ((shift >> bit) & 1) as u8;
        (pick(self.shift_high) << 1 | pick(self.shift_low), pick(self.shift_attribute_high) << 1 | pick(self.shift_attribute_low))
    }

    /// The first sprite's dot at `x` that isn't clear: whether it's sprite
    /// 0, its colour, its palette, and whether it's behind the background;
    /// none where they're off, or in the leftmost 8 hidden.
    fn sprite_dot(&self, x: usize) -> Option<(bool, u8, u8, bool)> {
        if self.mask & 0x10 == 0 || (x < 8 && self.mask & 0x04 == 0) {
            return None;
        }
        self.sprites[..self.sprite_count].iter().enumerate().find_map(|(slot, sprite)| {
            let along = x.checked_sub(sprite.x as usize).filter(|&along| along < 8)?;
            let bit = 7 - along;
            let colour = ((sprite.high >> bit) & 1) << 1 | ((sprite.low >> bit) & 1);
            (colour != 0).then(|| (slot == 0 && self.zero_on_line, colour, sprite.attributes & 3, sprite.attributes & 0x20 != 0))
        })
    }
}

/// Where a palette address is in its 32 bytes: the sprites' backdrop
/// entries, $3F10, $3F14, $3F18 and $3F1C, the background's.
fn palette_index(addr: u16) -> usize {
    let index = addr as usize & 0x1F;
    if index & 0x13 == 0x10 { index & 0x0F } else { index }
}
