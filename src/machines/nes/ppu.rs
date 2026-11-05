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
//! ahead (the next line's first two at the line's end, then two reads of
//! the nametable at 337 and 339, the third of them at the next's dot 1
//! its first tile): the nametable's byte, the attribute's, then the
//! pattern's two planes. Into shift
//! registers, which shift a dot at a time, the fine X scroll picking the
//! bit that's the dot's. Its address is the scroll's, "loopy's" v: coarse
//! X and Y, the nametable, fine Y; moved on a tile across as each is
//! fetched, a line down at dot 256, back to the left (from t) at 257, and
//! on the pre-render line back to the top (from t) at 280 to 304.
//!
//! The sprites: each line, a byte each 2 dots, secondary OAM cleared; the
//! 64 in OAM looked through, from OAMADDR, for the 8 (at most) on the next
//! line, copied there; those read back at dots 257 to 320 for their
//! patterns. A ninth found sets the overflow flag, but by the PPU's bug,
//! its search goes on looking at the wrong bytes. All of it's what $2004
//! reads while rendering. Sprite 0's dot drawn over a background's, neither
//! of them clear, sets the sprite 0 hit flag.
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
/// The dots from the write of PPUADDR's second byte to v having it: 1 to
/// 1.5 from when the write's done, a dot after it's made here. Past dot
/// 256 by the time it arrives, a line isn't moved on over it.
const V_DELAY: u8 = 2;
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
    pub v: u16,
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
    /// When the sprite flags, set, can first be read in $2002: a dot after
    /// they're set here.
    #[serde(skip)]
    hit_at: u64,
    #[serde(skip)]
    overflowed_at: u64,
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
    // The sprites: the next line's found, in secondary OAM, and whether
    // sprite 0 is; this line's, fetched.
    secondary: [u8; 32],
    /// In the save state's format, no longer used.
    found: usize,
    overflow_at: Option<u16>,
    zero_found: bool,
    sprites: [Sprite; 8],
    sprite_count: usize,
    zero_on_line: bool,
    /// The byte last read for the sprites; secondary OAM's address, and
    /// whether it's gone past the end; which byte of a sprite's being
    /// looked at, and whether the looking's done; and a sprite as read back
    /// for its pattern.
    #[serde(skip)]
    oam_buffer: u8,
    #[serde(skip)]
    secondary_addr: u8,
    #[serde(skip)]
    secondary_full: bool,
    #[serde(skip)]
    sprite_byte: u8,
    #[serde(skip)]
    evaluated: bool,
    #[serde(skip)]
    fetched: [u8; 4],
    /// Whether it was rendering as of the last dot, and the one before: a
    /// write of PPUMASK reaches the fetches a dot late, and what moves v a
    /// dot later still.
    #[serde(skip)]
    rendering_was: bool,
    #[serde(skip)]
    rendering_before: bool,
    /// The picture as far as this frame has drawn it: each dot's colour,
    /// 6 bits, and the emphasis, 3 more.
    #[serde(skip)]
    pub frame: Vec<u16>,
    /// A frame finished, to be shown.
    #[serde(skip)]
    pub frame_ready: bool,
    /// The dots till v takes t, after a write of PPUADDR: gone in a moment,
    /// so not in a state, taken between frames.
    #[serde(skip)]
    v_due: u8,
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
            hit_at: 0,
            overflowed_at: 0,
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
            oam_buffer: 0,
            secondary_addr: 0,
            secondary_full: false,
            sprite_byte: 0,
            evaluated: false,
            fetched: [0; 4],
            rendering_was: false,
            rendering_before: false,
            sprites: [Sprite::default(); 8],
            sprite_count: 0,
            zero_on_line: false,
            frame: vec![0; WIDTH * HEIGHT],
            frame_ready: false,
            v_due: 0,
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
            cart.ppu_read(addr);
        }
        match addr {
            0x0000..=0x1FFF => cart.chr_read(addr),
            0x2000..=0x3EFF => match cart.nametable_read(addr) {
                Some(byte) => byte,
                None => self.nametables[cart.nametable_page(addr) * 0x400 + (addr as usize & 0x3FF)],
            },
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
            0x2000..=0x3EFF => {
                if !cart.nametable_write(addr, value) {
                    self.nametables[cart.nametable_page(addr) * 0x400 + (addr as usize & 0x3FF)] = value;
                }
            }
            _ => self.palette[palette_index(addr)] = value & 0x3F,
        }
    }

    /// A register read, by the CPU: $2000 to $2007, `register` 0 to 7.
    pub fn read_register(&mut self, register: u16, cart: &mut Cartridge) -> u8 {
        let value = match register & 7 {
            2 => {
                // Vblank's as the read begins, the sprites' as it ends, nearer
                // 2 dots on: they've gone a dot before vblank's seen to.
                let sprites_gone = self.line == self.pre_render() && self.dot == 1;
                let set = |flag: bool, at: u64| flag && self.dots > at;
                let sprites = (set(self.sprite_zero_hit, self.hit_at) as u8) << 6 | (set(self.overflow, self.overflowed_at) as u8) << 5;
                let flags = (self.vblank as u8) << 7 | if sprites_gone { 0 } else { sprites };
                self.vblank = false;
                if self.line == VBLANK && self.dot == 1 {
                    self.suppress_vblank = true;
                }
                self.w = false;
                self.drive(flags, 0xE0)
            }
            // While rendering, what the PPU's reading for itself.
            4 if self.rendering() && self.line < 240 => self.drive(self.oam_buffer, 0xFF),
            4 => {
                let byte = self.oam_byte(self.oam_addr);
                self.drive(byte, 0xFF)
            }
            7 => {
                let addr = self.v & 0x3FFF;
                let value = if addr >= 0x3F00 {
                    // The palette's, at once, the buffer the nametable's
                    // under it; its top two bits the bus's.
                    self.buffer = self.read(addr - 0x1000, cart);
                    let colour = self.read(addr, cart);
                    let grey = if self.mask & 0x01 != 0 { 0x30 } else { 0x3F };
                    self.drive(colour & grey, 0x3F)
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
                // While rendering it's OAM's address that moves, a sprite on,
                // and nothing's written.
                if self.rendering() && (self.line < 240 || self.line == self.pre_render()) {
                    self.oam_addr = self.oam_addr.wrapping_add(4) & 0xFC;
                } else {
                    self.oam[self.oam_addr as usize] = value;
                    self.oam_addr = self.oam_addr.wrapping_add(1);
                }
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
                    self.v_due = V_DELAY;
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
        if self.v_due > 0 {
            self.v_due -= 1;
            if self.v_due == 0 {
                self.v = self.t;
                cart.ppu_bus(self.v & 0x3FFF, self.dots);
            }
        }
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

        if self.rendering_was && (seen || pre_render) {
            self.background(cart);
            self.sprite_fetches(cart);
            self.oam_dot();
        }
        if self.rendering_before && (seen || pre_render) {
            self.scroll_dot();
        }
        self.rendering_before = self.rendering_was;
        self.rendering_was = self.rendering();
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
                _ => {}
            }
        }
        match dot {
            280..=304 if self.line == self.pre_render() => self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0),
            // One more of the nametable at the end of a line, and the
            // next line's own at its start, of the same address, its tile
            // the one fetched ahead. On a frame a dot short there's still
            // this one, the dot gone being 340: a mapper counts these.
            339 => {
                self.read(0x2000 | (self.v & 0x0FFF), cart);
            }
            1 => self.tile = self.read(0x2000 | (self.v & 0x0FFF), cart),
            _ => {}
        }
    }

    /// v moved on, as the fetches have it for a dot of a line rendered: a
    /// tile on each 8, a line down at 256, back to the left at 257.
    fn scroll_dot(&mut self) {
        let dot = self.dot;
        if matches!(dot, 1..=256 | 321..=336) && dot % 8 == 0 {
            self.next_tile();
        }
        match dot {
            256 => self.next_line(),
            257 => self.v = (self.v & !0x041F) | (self.t & 0x041F),
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

    /// OAM's byte at `addr`: an attribute byte's bits 2 to 4 aren't there.
    fn oam_byte(&self, addr: u8) -> u8 {
        let byte = self.oam[addr as usize];
        if addr & 3 == 2 { byte & 0xE3 } else { byte }
    }

    /// Secondary OAM's address, a byte on; past its end it stays at 0, full.
    fn next_secondary(&mut self) {
        if !self.secondary_full {
            self.secondary_addr = (self.secondary_addr + 1) & 31;
            self.secondary_full = self.secondary_addr == 0;
        }
    }

    /// A dot's work on the sprites' memories, while rendering: this on the
    /// dot after the one being drawn, as $2004 sees it. Secondary OAM's
    /// cleared, a byte each 2 dots to 64 of a line seen; the next line's
    /// sprites looked for, to 256; read, to 320; and it's its first byte
    /// that's read after that, its address back at 0 by 340.
    fn oam_dot(&mut self) {
        let dot = self.dot + 1;
        let seen = self.line < 240;
        match dot {
            1..=64 if seen => {
                self.oam_buffer = 0xFF;
                if dot % 2 == 0 {
                    self.secondary[self.secondary_addr as usize] = 0xFF;
                    self.next_secondary();
                }
            }
            65..=256 if seen => self.evaluation_dot(dot),
            257..=320 => {
                self.oam_addr = 0;
                let step = (dot - 257) % 8;
                self.oam_buffer = self.secondary[self.secondary_addr as usize];
                if step < 4 {
                    self.fetched[step as usize] = self.oam_buffer;
                }
                if step < 3 || step == 7 {
                    self.next_secondary();
                }
            }
            321..=340 => self.oam_buffer = self.secondary[self.secondary_addr as usize],
            _ => {}
        }
        if matches!(dot, 64 | 256 | 340) {
            self.secondary_addr = 0;
            self.secondary_full = false;
        }
    }

    /// A dot of looking for sprites, from OAMADDR. On odd dots a byte's read
    /// of OAM; on even ones it's looked at, and written to secondary OAM,
    /// whose address moves on only for a sprite on the line. A Y that isn't
    /// on it moves OAMADDR 4 on, and back to a sprite's first byte; one
    /// that's on it, a byte at a time, and an X not "on the line" realigns
    /// it too. Secondary OAM full, nothing's written: what's read of it
    /// goes to the buffer instead. The next on the line sets the overflow
    /// flag; but until it's found, by the PPU's bug, a miss moves both parts
    /// of OAMADDR, so that what it looks at isn't a Y. All 64 seen, or that
    /// flag set, only the sprite part of OAMADDR goes on.
    fn evaluation_dot(&mut self, dot: u16) {
        if dot % 2 == 1 {
            if dot == 65 {
                self.sprite_byte = 0;
                self.evaluated = false;
                self.zero_found = false;
            }
            self.oam_buffer = self.oam_byte(self.oam_addr);
            return;
        }
        let secondary = self.secondary_addr as usize;
        if self.evaluated {
            self.oam_buffer = self.secondary[secondary];
            self.oam_addr = self.oam_addr.wrapping_add(4);
            return;
        }
        let (byte, addr, full) = (self.oam_buffer, self.oam_addr, self.secondary_full);
        let on_line = (0..self.sprite_height()).contains(&(self.line as i16 - byte as i16));
        let next_sprite_too = (addr.wrapping_add(4) & 0xFC) | (addr.wrapping_add(1) & 3);
        if full {
            self.oam_buffer = self.secondary[secondary];
        } else {
            self.secondary[secondary] = byte;
        }
        match self.sprite_byte {
            0 if !on_line => self.oam_addr = if full { next_sprite_too } else { addr.wrapping_add(4) & 0xFC },
            0 | 1 | 2 => {
                if self.sprite_byte == 0 {
                    if full && !self.overflow {
                        self.overflow = true;
                        self.overflowed_at = self.dots + 1;
                    }
                    self.zero_found |= dot == 66;
                }
                self.next_secondary();
                self.sprite_byte += 1;
                self.oam_addr = addr.wrapping_add(1);
            }
            _ => {
                self.sprite_byte = 0;
                self.next_secondary();
                self.oam_addr = addr.wrapping_add(1);
                if full || !on_line {
                    self.oam_addr &= 0xFC;
                }
                self.evaluated |= full;
            }
        }
        self.evaluated |= self.oam_addr >> 2 < addr >> 2;
    }

    /// The next line's sprites fetched, dots 257 to 320, a sprite each 8,
    /// its pattern's planes on its fifth and seventh, from what was read of
    /// secondary OAM: OAMADDR held at 0 meanwhile. Each of the 8 has its
    /// pattern fetched, but drawn only if it's on the line, by the low byte
    /// of its number: on the pre-render line, what's left in secondary OAM
    /// can be, for line 0.
    fn sprite_fetches(&mut self, cart: &mut Cartridge) {
        let dot = self.dot;
        if !(257..=320).contains(&dot) {
            return;
        }
        if dot == 257 {
            self.sprite_count = 8;
            self.zero_on_line = self.zero_found;
        }
        let (slot, step) = ((dot - 257) as usize / 8, (dot - 257) % 8);
        if step != 4 && step != 6 {
            return;
        }
        let [y, tile, attributes, x] = self.fetched;
        let height = self.sprite_height() as u16;
        let distance = (self.line & 0xFF) as i16 - y as i16;
        let on_line = (0..height as i16).contains(&distance);
        let mut row = distance as u16 & (height - 1);
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
        cart.sprite_fetch = true;
        let mut plane = self.read(addr + if step == 6 { 8 } else { 0 }, cart);
        cart.sprite_fetch = false;
        if !on_line {
            plane = 0;
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
                if zero && background != 0 && x != 255 && !self.sprite_zero_hit {
                    self.sprite_zero_hit = true;
                    self.hit_at = self.dots + 1;
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
