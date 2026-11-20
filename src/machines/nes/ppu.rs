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
//!
//! Dots that nothing could see the doing of till later can be owed: gone by
//! for the CPU, run all at once when something's to look, as `owe` says.
//! They're the same dots, in the same order, at the same dot of its count,
//! only run later, together: much quicker than one at a time between the
//! CPU's cycles.

use super::cartridge::Cartridge;
use super::region::Region;
use crate::host::save_state;
use serde::{Deserialize, Serialize};

mod fast;

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
    /// Performance mode: a line at a time, as `fast` has it, not a dot; and
    /// in it, the dot with something to do next.
    #[serde(skip)]
    pub fast: bool,
    #[serde(skip)]
    nothing_until: u16,
    /// Dots owed: gone by for the CPU, but not yet run, as they can be on a
    /// line being drawn. Never any between frames, where a state's taken.
    #[serde(skip)]
    owed: u16,
    /// Whether they're of the vertical blank, where it does nothing.
    #[serde(skip)]
    owed_idle: bool,
    /// This frame's picture isn't wanted: only what the game can see of it.
    #[serde(skip)]
    pub leaving_out: bool,
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
    /// What those come to at each X of the line, as `lay_out_sprites` has
    /// it: looking through 8 of them for every dot is much of a frame's
    /// time, and they don't change while it's drawn.
    #[serde(skip)]
    sprite_dots: Vec<u8>,
    #[serde(skip)]
    sprites_laid_out: bool,
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
    /// The palette's 32 colours as they're drawn, greyed and emphasised as
    /// PPUMASK has them: made again once either's written.
    #[serde(skip)]
    colours: Option<[u16; 32]>,
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
            fast: false,
            nothing_until: 0,
            owed: 0,
            owed_idle: false,
            leaving_out: false,
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
            sprite_dots: Vec::new(),
            sprites_laid_out: false,
            frame: vec![0; WIDTH * HEIGHT],
            colours: None,
            frame_ready: false,
            v_due: 0,
        }
    }

    /// Its warm-up over already: a Famicom's, started before its CPU.
    pub fn warmed_up(&mut self) {
        self.warming_up = false;
    }

    /// RESET: PPUCTRL and PPUMASK cleared, the write toggle and the buffer;
    /// writes ignored again, until the pre-render line.
    pub fn reset(&mut self) {
        self.ctrl = 0;
        self.mask = 0;
        self.colours = None;
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
            0x0000..=0x1FFF => match cart.chr_nametable(addr) {
                Some(page) => self.nametables[page * 0x400 + (addr as usize & 0x3FF)],
                None => cart.chr_read(addr),
            },
            0x2000..=0x3EFF => match cart.nametable_read(addr) {
                Some(byte) => byte,
                None => {
                    self.nametables[cart.nametable_page(addr) * 0x400 + (addr as usize & 0x3FF)]
                }
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
            0x0000..=0x1FFF => match cart.chr_nametable(addr) {
                Some(page) => self.nametables[page * 0x400 + (addr as usize & 0x3FF)] = value,
                None => cart.chr_write(addr, value),
            },
            0x2000..=0x3EFF => {
                if !cart.nametable_write(addr, value) {
                    self.nametables[cart.nametable_page(addr) * 0x400 + (addr as usize & 0x3FF)] =
                        value;
                }
            }
            _ => {
                self.palette[palette_index(addr)] = value & 0x3F;
                self.colours = None;
            }
        }
    }

    /// A register read, by the CPU: $2000 to $2007, `register` 0 to 7.
    pub fn read_register(&mut self, register: u16, cart: &mut Cartridge) -> u8 {
        match register & 7 {
            2 => {
                // Vblank's as the read begins, the sprites' as it ends, nearer
                // 2 dots on: they've gone a dot before vblank's seen to.
                let sprites_gone = self.line == self.pre_render() && self.dot == 1;
                let set = |flag: bool, at: u64| flag && self.dots > at;
                let sprites = (set(self.sprite_zero_hit, self.hit_at) as u8) << 6
                    | (set(self.overflow, self.overflowed_at) as u8) << 5;
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
        }
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
            1 if !self.warming_up => {
                self.mask = value;
                self.colours = None;
            }
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
                    self.t = (self.t & !0x73E0)
                        | ((value as u16 & 7) << 12)
                        | ((value as u16 & 0xF8) << 2);
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

    /// A write of PPUADDR's second byte, reaching v once it's due.
    #[inline(always)]
    fn settle_v(&mut self, cart: &mut Cartridge) {
        if self.v_due > 0 {
            self.v_due -= 1;
            if self.v_due == 0 {
                self.v = self.t;
                cart.ppu_bus(self.v & 0x3FFF, self.dots);
            }
        }
    }

    fn vblank_starts(&mut self) {
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

    fn vblank_ends(&mut self) {
        self.vblank = false;
        self.sprite_zero_hit = false;
        self.overflow = false;
        self.warming_up = false;
    }

    /// The pre-render line's end: a dot short on odd frames, if it's
    /// rendering as of dot 338.
    #[inline(always)]
    fn pre_render_end(&mut self) {
        if self.dot == 338 {
            self.short_line = self.odd && self.rendering() && self.region == Region::Ntsc;
        }
        if self.dot == 339 && self.short_line {
            self.dot = 340;
        }
    }

    /// On, a dot.
    #[inline(always)]
    fn next_dot(&mut self) {
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

    /// Whether fetches are being made that performance mode doesn't make
    /// when they would be, for a mapper that goes by there being some.
    #[inline]
    pub fn fetching_unseen(&self) -> bool {
        self.fast && self.line < 240 && self.rendering()
    }

    /// `dots` dots gone by at once, if that's all they'd be, as they can be
    /// in performance mode: whether they have.
    #[inline]
    pub fn passed(&mut self, dots: u16) -> bool {
        let quiet = self.fast && self.dot + dots <= self.nothing_until && self.v_due == 0;
        if quiet {
            self.dot += dots;
            self.dots += dots as u64;
        }
        quiet
    }

    /// `dots` to come owed, if they can be, after any owed already: run
    /// later, all at once, when something's to see what they've done, or
    /// they can't be. Whether they are, and if so, whether there are
    /// fetches in them.
    ///
    /// They can be on a line being drawn, from its dot 2 to 255, rendering
    /// on and as it was: then the PPU only fetches its tiles, looks for the
    /// next line's sprites and draws, so nothing the CPU can see changes
    /// but by its registers, and nothing a mapper does: its NMI's not due,
    /// an MMC3 sees A12 high for no more than a few dots at a time, and an
    /// MMC5 sees no line start (but, `quiet` not, the one it's to count at
    /// the next read, its third dot's). What a mapper reads it's to read
    /// in the same order, and the dot it's at the same, only later. Or in
    /// the lines after the picture's, but where the vertical blank starts:
    /// there it does nothing at all.
    #[inline]
    pub fn owe(&mut self, dots: u16, quiet: bool) -> Option<bool> {
        if self.fast || self.v_due != 0 {
            return None;
        }
        if self.line < 240 {
            let owing = (quiet || self.owed > 0)
                && self.dot >= 2
                && self.dot + self.owed + dots <= 256
                && self.rendering_was
                && self.rendering_before
                && self.rendering();
            if !owing {
                return None;
            }
        } else {
            // From the line's start to the vertical blank's dot, or from
            // the dot after it to the pre-render line.
            let at = |line: u16, dot: u16| line as u32 * DOTS as u32 + dot as u32;
            let (from, to) = (
                at(self.line, self.dot),
                at(self.line, self.dot) + (self.owed + dots) as u32,
            );
            let owing = if from <= at(VBLANK, 1) {
                to <= at(VBLANK, 1)
            } else {
                from >= at(VBLANK, 2) && to <= at(self.pre_render(), 0)
            };
            if !owing {
                return None;
            }
        }
        self.owed_idle = self.line >= 240;
        self.owed += dots;
        Some(!self.owed_idle)
    }

    /// The dots owed, run.
    #[inline]
    pub fn catch_up(&mut self, cart: &mut Cartridge) {
        if self.owed > 0 {
            self.run_owed(cart);
        }
    }

    #[inline(never)]
    fn run_owed(&mut self, cart: &mut Cartridge) {
        let owed = std::mem::take(&mut self.owed);
        if !self.owed_idle {
            for _ in 0..owed {
                self.drawing_dot(cart);
            }
            return;
        }
        // Nothing but the time, and whether it was rendering.
        self.rendering_before = if owed == 1 {
            self.rendering_was
        } else {
            self.rendering()
        };
        self.rendering_was = self.rendering();
        self.dots += owed as u64;
        let dot = self.dot + owed;
        self.line += dot / DOTS;
        self.dot = dot % DOTS;
    }

    /// A dot.
    #[inline]
    pub fn tick(&mut self, cart: &mut Cartridge) {
        if !self.fast {
            // Most are a line's being drawn, rendering, as it was, or its
            // sprites' being fetched: those on their own, without looking
            // for what the others do.
            let steady = self.line < 240
                && self.v_due == 0
                && self.rendering_was
                && self.rendering_before
                && self.rendering();
            if steady && self.dot.wrapping_sub(2) < 254 {
                self.drawing_dot(cart);
            } else if steady && self.dot.wrapping_sub(258) < 63 {
                self.sprite_fetches(cart);
                self.oam_dot();
                self.dot += 1;
                self.dots += 1;
            } else {
                self.exact_tick(cart);
            }
        } else if self.dot < self.nothing_until && self.v_due == 0 {
            self.dot += 1;
            self.dots += 1;
        } else {
            self.quick_tick(cart);
        }
    }

    /// A dot of everything it does.
    #[inline(never)]
    fn exact_tick(&mut self, cart: &mut Cartridge) {
        self.settle_v(cart);
        let (line, dot) = (self.line, self.dot);
        let seen = line < 240;
        let pre_render = !seen && line == self.pre_render();

        if dot == 1 && !seen {
            if line == VBLANK {
                self.vblank_starts();
            }
            if pre_render {
                self.vblank_ends();
            }
        }

        if seen || pre_render {
            if self.rendering_was {
                self.background(cart);
                self.sprite_fetches(cart);
                self.oam_dot();
            }
            if self.rendering_before {
                self.scroll_dot();
            }
        }
        self.rendering_before = self.rendering_was;
        self.rendering_was = self.rendering();
        if seen && dot.wrapping_sub(1) < 256 {
            self.pixel();
        }

        if pre_render && dot >= 338 {
            self.pre_render_end();
        }
        self.next_dot();
    }

    /// A dot from 2 to 255 of a line seen, rendering on and as it was the
    /// two dots before: as `exact_tick` has it, but only what it does then.
    #[inline(always)]
    fn drawing_dot(&mut self, cart: &mut Cartridge) {
        let dot = self.dot;
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
        // The sprites' memories, a dot on.
        let next = dot + 1;
        if next <= 64 {
            self.oam_buffer = 0xFF;
            if next & 1 == 0 {
                self.secondary[self.secondary_addr as usize] = 0xFF;
                self.next_secondary();
            }
        } else {
            self.evaluation_dot(next);
        }
        if next == 64 || next == 256 {
            self.secondary_addr = 0;
            self.secondary_full = false;
        }
        if dot & 7 == 0 {
            self.next_tile();
        }
        self.drawn_pixel(dot as usize - 1);
        self.dot += 1;
        self.dots += 1;
    }

    /// `pixel`, rendering: the dot at `x`.
    #[inline(always)]
    fn drawn_pixel(&mut self, x: usize) {
        let bit = 15 - self.fine_x as u16;
        let pick = |shift: u16| ((shift >> bit) & 1) as usize;
        let shown = |on: u8, left: u8| self.mask & on != 0 && (x >= 8 || self.mask & left != 0);
        let background = if shown(0x08, 0x02) {
            pick(self.shift_high) << 1 | pick(self.shift_low)
        } else {
            0
        };
        // The first sprite's there that isn't clear, as `lay_out_sprites`
        // has it.
        let sprite = if shown(0x10, 0x04) {
            if !self.sprites_laid_out {
                self.lay_out_sprites();
            }
            self.sprite_dots[x] as usize
        } else {
            0
        };
        // Sprite 0 hit where both are drawn, in front or behind: looked for
        // only where there's a sprite, the background's then known.
        let index = if sprite != 0 && (background == 0 || sprite & 0x10 == 0) {
            if background != 0 {
                self.hit(sprite, x);
            }
            0x10 | sprite & 0x0F
        } else if background != 0 {
            self.hit(sprite, x);
            pick(self.shift_attribute_high) << 3 | pick(self.shift_attribute_low) << 2 | background
        } else {
            0
        };
        self.put(x, index);
    }

    /// Sprite 0's hit, if `sprite`'s dot at `x` is sprite 0's, over a
    /// background's not clear, and it's the first.
    #[inline(always)]
    fn hit(&mut self, sprite: usize, x: usize) {
        if sprite & 0x20 != 0 && x != 255 && !self.sprite_zero_hit {
            self.sprite_zero_hit = true;
            self.hit_at = self.dots + 1;
        }
    }

    /// The palette's colour `index` at `x` of the line; or, PAL's, the
    /// border there.
    #[inline(always)]
    fn put(&mut self, x: usize, index: usize) {
        let border = self.region == Region::Pal && (self.line == 0 || !(2..WIDTH - 2).contains(&x));
        let colour = if border {
            BORDER
        } else {
            self.colours()[index]
        };
        self.frame[self.line as usize * WIDTH + x] = colour;
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
        if dot > 1 && dot < 280 {
            return;
        }
        match dot {
            280..=304 if self.line == self.pre_render() => {
                self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0)
            }
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
        if dot & 7 == 0 && matches!(dot, 1..=256 | 321..=336) {
            self.next_tile();
        }
        if dot & !1 == 256 {
            if dot == 256 {
                self.next_line();
            } else {
                self.v = (self.v & !0x041F) | (self.t & 0x041F);
            }
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
                if dot & 1 == 0 {
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
        if (dot & 63 == 0 || dot == 340) && matches!(dot, 64 | 256 | 340) {
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
            0 if !on_line => {
                self.oam_addr = if full {
                    next_sprite_too
                } else {
                    addr.wrapping_add(4) & 0xFC
                }
            }
            0..=2 => {
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
            self.sprites_laid_out = false;
        }
        let (slot, step) = ((dot - 257) as usize / 8, (dot - 257) % 8);
        if step != 4 && step != 6 {
            return;
        }
        let [y, tile, attributes, x] = self.fetched;
        let (addr, on_line) = self.sprite_address(y, tile, attributes);
        cart.sprite_fetch = true;
        let mut plane = self.read(addr + if step == 6 { 8 } else { 0 }, cart);
        cart.sprite_fetch = false;
        if !on_line {
            plane = 0;
        }
        if attributes & 0x40 != 0 {
            plane = plane.reverse_bits();
        }
        self.sprites_laid_out = false;
        let sprite = &mut self.sprites[slot];
        if step == 4 {
            *sprite = Sprite {
                x,
                low: plane,
                high: 0,
                attributes,
            };
        } else {
            sprite.high = plane;
        }
    }

    /// A sprite's pattern's first plane's address, for the line it's fetched
    /// on, and whether the line's one of its own.
    fn sprite_address(&self, y: u8, tile: u8, attributes: u8) -> (u16, bool) {
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
        (addr, on_line)
    }

    /// A dot of a line seen: the background's and the first sprite's there
    /// not clear, either in front; or neither, the backdrop. With rendering
    /// off, the backdrop, or the palette's colour v is at if it's there.
    fn pixel(&mut self) {
        let x = self.dot as usize - 1;
        if self.rendering() {
            self.drawn_pixel(x);
            return;
        }
        let addr = if self.v & 0x3F00 == 0x3F00 {
            self.v
        } else {
            0x3F00
        };
        self.put(x, addr as usize & 0x1F);
    }

    /// The palette's colours as they're drawn, `index` 0 to 31.
    #[inline(always)]
    fn colours(&mut self) -> &[u16; 32] {
        if self.colours.is_none() {
            let grey = if self.mask & 0x01 != 0 { 0x30 } else { 0x3F };
            let emphasis = (self.mask as u16 >> 5) << 6;
            self.colours = Some(std::array::from_fn(|index| {
                (self.palette[palette_index(0x3F00 | index as u16)] & grey) as u16 | emphasis
            }));
        }
        self.colours.as_ref().unwrap()
    }

    /// Each X's first sprite not clear, a byte: its colour, its palette
    /// above that, whether it's behind the background, whether it's sprite 0.
    fn lay_out_sprites(&mut self) {
        self.sprite_dots.clear();
        self.sprite_dots.resize(WIDTH, 0);
        for (slot, sprite) in self.sprites[..self.sprite_count].iter().enumerate() {
            let zero = slot == 0 && self.zero_on_line;
            let flags =
                (sprite.attributes & 3) << 2 | (sprite.attributes & 0x20) >> 1 | (zero as u8) << 5;
            for along in 0..8.min(WIDTH - sprite.x as usize) {
                let bit = 7 - along;
                let colour = ((sprite.high >> bit) & 1) << 1 | ((sprite.low >> bit) & 1);
                let dot = &mut self.sprite_dots[sprite.x as usize + along];
                if *dot == 0 && colour != 0 {
                    *dot = colour | flags;
                }
            }
        }
        self.sprites_laid_out = true;
    }
}

/// Where a palette address is in its 32 bytes: the sprites' backdrop
/// entries, $3F10, $3F14, $3F18 and $3F1C, the background's.
fn palette_index(addr: u16) -> usize {
    let index = addr as usize & 0x1F;
    if index & 0x13 == 0x10 {
        index & 0x0F
    } else {
        index
    }
}
