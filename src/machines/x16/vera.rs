//! VERA, the X16's video, as its Programmer's Reference has it and the
//! X16 emulator (x16emu, BSD-2) does: 128K of video RAM of its own, reached
//! through two ports at 32 registers, `$9F20-$9F3F`; two layers, each of
//! tiles (text, 1 bit a dot in 16 colours or 256, or 2, 4 or 8 bits) or a
//! bitmap; a palette of 256 colours of 4096; a 640 by 480 picture at 60
//! frames a second, VGA's, its active part scaled and bordered as its
//! composer's set. Its interrupts: VSYNC, as line 480 begins; LINE, as the
//! line set does; and AFLOW, its audio FIFO less than a quarter full.
//!
//! A port's ADDRx (as CTRL's ADDRSEL picks) is its address in video RAM
//! and its step, a power of 2 or 40 times one, up or down; its DATAx reads
//! the byte there, fetched as the address was set or last stepped, and
//! writes it, each stepping on. The top of video RAM is its registers' too,
//! written as it's written: the PSG's (`$1F9C0`), the palette (`$1FA00`) and
//! the sprites' (`$1FC00`).
//!
//! A line's drawn as the one before it's scanned out, from its layers'
//! registers as they were then: here, as it ends, but for its vertical
//! scroll, as it is; its sprites as their attributes are then, as many as
//! its renderer has time for, their collisions into ISR as line 480 begins.
//! Its FX (`vera_fx`, DCSEL 2 to 6) steps its ports' addresses and writes
//! through them as its helpers, its cache and its multiplier have it.
//! NTSC and RGB, 15 kHz, are interlaced: two fields a frame, each of 525
//! half-lines of 794 dots, its 240 lines the picture's even ones or its
//! odd, the other field's left as they were, as a TV shows them woven;
//! VSYNC and LINE each field, LINE's line's bit 0 left out. In 240P, the
//! fields 526 half-lines, both on the even lines, the odd ones dark. A
//! TV's edge, past its title-safe part, shown dimmed in NTSC, as x16emu
//! shows it. SCANLINE, the line being sent, as the docs have it: even in
//! the first field, odd in the second.
//! Its sound's the PSG's and the PCM's (`vera_audio`), a sample each 512
//! of its clocks. Its SPI port has the SD card on it (`sd_card`), if
//! there is one.

use super::sd_card::SdCard;
use super::vera_audio::{Pcm, Psg};
use super::vera_fx::Fx;
use serde::{Deserialize, Serialize};

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;
/// A line's CPU cycles: its 800 dots at 25 MHz, the CPU at 8 MHz; a frame's
/// lines.
pub const LINE_CYCLES: u32 = 256;
pub const LINES: u16 = 525;
/// VGA's line and NTSC's half-line, in 8ths of a dot, 25 a CPU cycle; a
/// field's half-lines, interlaced and 240P; and the half-line each field's
/// line 0 begins on (240P's both the first's).
const VGA_LINE: u32 = 800 * 8;
const HALF_LINE: u32 = 794 * 8;
const FIELD: u16 = 525;
const FIELD_240P: u16 = 526;
const FIELD_TOPS: [i32; 2] = [42, 43];
/// NTSC's title-safe part, inside a TV's edge, as x16emu has it: the rest
/// dimmed.
const SAFE_X: std::ops::RangeInclusive<usize> = 43..=597;
const SAFE_Y: std::ops::RangeInclusive<usize> = 24..=456;

const VRAM_SIZE: usize = 0x20000;
const PSG: usize = 0x1F9C0;
const PALETTE: usize = 0x1FA00;
const SPRITES: usize = 0x1FC00;

/// ISR's (and IEN's) bits; ISR's top 4, the sprites' collisions.
const VSYNC: u8 = 0x01;
const LINE: u8 = 0x02;
const SPRCOL: u8 = 0x04;
const AFLOW: u8 = 0x08;

/// The sprite renderer's clocks a line, as x16emu counts them: one a
/// sprite looked at, one a 32 bits fetched, one a dot drawn.
const SPRITE_BUDGET: u32 = 800;

/// Its sound's samples, in CPU cycles: each 512 of its 25 MHz clocks, the
/// CPU at 8 MHz, counted in 25ths of a CPU cycle.
const SAMPLE_CYCLES: u32 = 512 * 8;
const CLOCKS_A_CYCLE: u32 = 25;

/// DC_VER0 to 3, read where a register's only written: "V", 47.0.2.
const VERSION: [u8; 4] = [b'V', 47, 0, 2];

/// ADDRx_H's address increment, bits 7 to 4.
const INCREMENTS: [u32; 16] = [
    0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 40, 80, 160, 320, 640,
];

/// SPI transfers' CPU cycles, a byte's 8 bits: at 12.5 MHz, and at its
/// slow clock's 390 kHz.
const SPI_FAST: u32 = 6;
const SPI_SLOW: u32 = 164;

/// The palette at reset, as x16emu has it: 0 to 15 near the C64's, 16 to
/// 31 a ramp of greys, the rest hues by saturation and brightness.
#[rustfmt::skip]
const DEFAULT_PALETTE: [u16; 256] = [
    0x000,0xfff,0x800,0xafe,0xc4c,0x0c5,0x00a,0xee7,0xd85,0x640,0xf77,0x333,0x777,0xaf6,0x08f,0xbbb,
    0x000,0x111,0x222,0x333,0x444,0x555,0x666,0x777,0x888,0x999,0xaaa,0xbbb,0xccc,0xddd,0xeee,0xfff,
    0x211,0x433,0x644,0x866,0xa88,0xc99,0xfbb,0x211,0x422,0x633,0x844,0xa55,0xc66,0xf77,0x200,0x411,
    0x611,0x822,0xa22,0xc33,0xf33,0x200,0x400,0x600,0x800,0xa00,0xc00,0xf00,0x221,0x443,0x664,0x886,
    0xaa8,0xcc9,0xfeb,0x211,0x432,0x653,0x874,0xa95,0xcb6,0xfd7,0x210,0x431,0x651,0x862,0xa82,0xca3,
    0xfc3,0x210,0x430,0x640,0x860,0xa80,0xc90,0xfb0,0x121,0x343,0x564,0x786,0x9a8,0xbc9,0xdfb,0x121,
    0x342,0x463,0x684,0x8a5,0x9c6,0xbf7,0x120,0x241,0x461,0x582,0x6a2,0x8c3,0x9f3,0x120,0x240,0x360,
    0x480,0x5a0,0x6c0,0x7f0,0x121,0x343,0x465,0x686,0x8a8,0x9ca,0xbfc,0x121,0x242,0x364,0x485,0x5a6,
    0x6c8,0x7f9,0x020,0x141,0x162,0x283,0x2a4,0x3c5,0x3f6,0x020,0x041,0x061,0x082,0x0a2,0x0c3,0x0f3,
    0x122,0x344,0x466,0x688,0x8aa,0x9cc,0xbff,0x122,0x244,0x366,0x488,0x5aa,0x6cc,0x7ff,0x022,0x144,
    0x166,0x288,0x2aa,0x3cc,0x3ff,0x022,0x044,0x066,0x088,0x0aa,0x0cc,0x0ff,0x112,0x334,0x456,0x668,
    0x88a,0x9ac,0xbcf,0x112,0x224,0x346,0x458,0x56a,0x68c,0x79f,0x002,0x114,0x126,0x238,0x24a,0x35c,
    0x36f,0x002,0x014,0x016,0x028,0x02a,0x03c,0x03f,0x112,0x334,0x546,0x768,0x98a,0xb9c,0xdbf,0x112,
    0x324,0x436,0x648,0x85a,0x96c,0xb7f,0x102,0x214,0x416,0x528,0x62a,0x83c,0x93f,0x102,0x204,0x306,
    0x408,0x50a,0x60c,0x70f,0x212,0x434,0x646,0x868,0xa8a,0xc9c,0xfbe,0x211,0x423,0x635,0x847,0xa59,
    0xc6b,0xf7d,0x201,0x413,0x615,0x826,0xa28,0xc3a,0xf3c,0x201,0x403,0x604,0x806,0xa08,0xc09,0xf0b,
];

/// Its SPI port: its chip select, slow clock and auto-TX; a byte being
/// sent, its cycles to go; the byte received, the card's answer, or $FF
/// with none.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Spi {
    select: bool,
    slow: bool,
    auto_tx: bool,
    busy: u32,
    sending: u8,
    received: u8,
    card: SdCard,
}

impl Spi {
    fn send(&mut self, byte: u8) {
        if self.select && self.busy == 0 {
            self.sending = byte;
            self.busy = if self.slow { SPI_SLOW } else { SPI_FAST };
        }
    }

    fn tick(&mut self) {
        if self.busy > 0 {
            self.busy -= 1;
            if self.busy == 0 {
                self.received = self.card.exchange(self.sending);
            }
        }
    }
}

/// A layer's registers, as they say how it's drawn.
struct Layer {
    /// Its colour depth, 0 to 3: 1, 2, 4 or 8 bits a dot.
    depth: u32,
    bitmap: bool,
    t256c: bool,
    map_base: usize,
    tile_base: usize,
    /// Its map's width and height, and its tiles', as powers of 2.
    map_width: u32,
    map_height: u32,
    tile_width: u32,
    tile_height: u32,
    hscroll: u32,
    vscroll: u32,
}

impl Layer {
    fn of(registers: &[u8; 7]) -> Layer {
        let [config, map_base, tile_base, ..] = *registers;
        Layer {
            depth: (config & 3) as u32,
            bitmap: config & 0x04 != 0,
            t256c: config & 0x08 != 0,
            map_base: (map_base as usize) << 9,
            tile_base: ((tile_base & 0xFC) as usize) << 9,
            map_width: 5 + (config >> 4 & 3) as u32,
            map_height: 5 + (config >> 6) as u32,
            tile_width: 3 + (tile_base & 1) as u32,
            tile_height: 3 + (tile_base >> 1 & 1) as u32,
            hscroll: u16::from_le_bytes([registers[3], registers[4] & 0x0F]) as u32,
            vscroll: u16::from_le_bytes([registers[5], registers[6] & 0x0F]) as u32,
        }
    }
}

/// A sprite's attributes, its 8 bytes at `$1FC00` on.
struct Sprite {
    address: usize,
    /// 8 bits a dot, not 4.
    bytes: bool,
    /// Its place, -its size up to 1023 less it.
    x: i32,
    y: i32,
    hflip: bool,
    vflip: bool,
    /// Its Z-depth, 0 (off) to 3, and its collision mask, bits 7 to 4.
    z: u8,
    mask: u8,
    /// Its width and height as powers of 2.
    width: u32,
    height: u32,
    offset: u8,
}

impl Sprite {
    fn of(a: &[u8]) -> Sprite {
        let (width, height) = (3 + (a[7] >> 4 & 3) as u32, 3 + (a[7] >> 6) as u32);
        let place = |low: u8, high: u8, size: u32| {
            let at = low as i32 | ((high & 3) as i32) << 8;
            if at >= 0x400 - (1 << size) {
                at - 0x400
            } else {
                at
            }
        };
        Sprite {
            address: (a[0] as usize) << 5 | ((a[1] & 0x0F) as usize) << 13,
            bytes: a[1] & 0x80 != 0,
            x: place(a[2], a[3], width),
            y: place(a[4], a[5], height),
            hflip: a[6] & 1 != 0,
            vflip: a[6] & 2 != 0,
            z: a[6] >> 2 & 3,
            mask: a[6] & 0xF0,
            width,
            height,
            offset: (a[7] & 0x0F) << 4,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Vera {
    vram: Vec<u8>,
    /// The palette, 256 entries of 2 bytes: green and blue, then red.
    palette: Vec<u8>,
    /// The ports: each one's address, its ADDRx_H's high bits (its step
    /// and DECR), the byte fetched for DATAx; in FX's 4-bit mode, its
    /// nibble, and whether it steps by nibbles.
    address: [u32; 2],
    stepping: [u8; 2],
    fetched: [u8; 2],
    nibble: [bool; 2],
    nibble_steps: [bool; 2],
    addrsel: usize,
    dcsel: u8,
    ien: u8,
    isr: u8,
    irq_line: u16,
    layers: [[u8; 7]; 2],
    /// DCSEL 0 and 1's registers: DC_VIDEO, its scales and border, its
    /// active part's edges.
    composer: [u8; 8],
    /// DCSEL 2 to 6's, FX's.
    fx: Fx,
    psg: Psg,
    pcm: Pcm,
    /// Its clocks to its next sample, and its sound as it is, left and
    /// right: the PSG's and the PCM's.
    sample_clock: u32,
    sound: (i32, i32),
    spi: Spi,
    /// The line being scanned out, or in NTSC or RGB the half-line, of
    /// both fields; and its time so far, in 8ths of a dot.
    line: u16,
    cycles: u32,
    /// The layers' registers and VSCALE as they were when the line before
    /// ended, for the line drawn now.
    layers_before: [[u8; 7]; 2],
    vscale_before: u8,
    /// The line of the picture shown, 16.16, as VSCALE steps it; and
    /// whether it's begun this frame (or field), at VSTART.
    picture_line: u32,
    started: bool,
    /// The sprites' collisions this frame, bits 7 to 4.
    collisions: u8,
    frame_ended: bool,
    #[serde(skip)]
    pub picture: Vec<u32>,
    /// The frame's picture left out, for time.
    #[serde(skip)]
    pub leave_out: bool,
}

impl Vera {
    /// One as it's turned on: its video RAM 0s, its registers reset.
    pub fn new() -> Vera {
        let mut vera = Vera {
            vram: vec![0; VRAM_SIZE],
            palette: vec![0; 512],
            address: [0; 2],
            stepping: [0; 2],
            fetched: [0; 2],
            nibble: [false; 2],
            nibble_steps: [false; 2],
            addrsel: 0,
            dcsel: 0,
            ien: 0,
            isr: 0,
            irq_line: 0,
            layers: [[0; 7]; 2],
            composer: [0; 8],
            fx: Fx::new(),
            psg: Psg::new(),
            pcm: Pcm::new(),
            sample_clock: 0,
            sound: (0, 0),
            spi: Spi {
                received: 0xFF,
                ..Spi::default()
            },
            line: 0,
            cycles: 0,
            layers_before: [[0; 7]; 2],
            vscale_before: 0,
            picture_line: 0,
            started: false,
            collisions: 0,
            frame_ended: false,
            picture: vec![0; WIDTH * HEIGHT],
            leave_out: false,
        };
        vera.reset();
        vera
    }

    /// CTRL's RESET: its registers as they come up, the palette its own,
    /// its scan from the top. Video RAM's as it was.
    pub fn reset(&mut self) {
        self.address = [0; 2];
        self.stepping = [0; 2];
        self.fetched = [0; 2];
        (self.nibble, self.nibble_steps) = ([false; 2], [false; 2]);
        (self.addrsel, self.dcsel) = (0, 0);
        (self.ien, self.isr, self.irq_line) = (0, 0, 0);
        self.layers = [[0; 7]; 2];
        // HSCALE and VSCALE 1, the active part all 640 by 480.
        self.composer = [
            0,
            128,
            128,
            0,
            0,
            (WIDTH >> 2) as u8,
            0,
            (HEIGHT >> 1) as u8,
        ];
        self.fx = Fx::new();
        (self.psg, self.pcm, self.sound) = (Psg::new(), Pcm::new(), (0, 0));
        for (n, colour) in DEFAULT_PALETTE.iter().enumerate() {
            self.palette[n * 2..n * 2 + 2].copy_from_slice(&colour.to_le_bytes());
        }
        (self.line, self.cycles, self.picture_line) = (0, 0, 0);
        self.started = false;
        self.collisions = 0;
    }

    /// The byte at `address` in video RAM.
    pub fn peek(&self, address: usize) -> u8 {
        self.vram[address & (VRAM_SIZE - 1)]
    }

    /// `value` written by a port at `address` (in 4-bit mode, its
    /// `nibble`), unless it's 0 and transparent writes are on; and to the
    /// register that's there, if one is, as it's written.
    fn poke(&mut self, address: usize, nibble: bool, value: u8) {
        let at = address & (VRAM_SIZE - 1);
        let (old, fx) = (self.vram[at], &self.fx);
        let (low, high) = (value & 0x0F, value & 0xF0);
        self.vram[at] = match (fx.four_bit, nibble) {
            (true, true) if !fx.transparent || low != 0 => old & 0xF0 | low,
            (true, false) if !fx.transparent || high != 0 => old & 0x0F | high,
            (false, _) if !fx.transparent || value != 0 => value,
            _ => old,
        };
        // The sprites' are read where they're written; the palette's kept
        // apart, for RESET to set it again.
        if (PSG..PALETTE).contains(&at) {
            self.psg.write(at - PSG, value);
        } else if (PALETTE..SPRITES).contains(&at) {
            self.palette[at - PALETTE] = value;
        }
    }

    /// Port `port`'s increment, as its ADDRx_H says: signed.
    fn increment(&self, port: usize) -> i32 {
        let by = INCREMENTS[self.stepping[port] as usize >> 4] as i32;
        if self.stepping[port] & 0x08 != 0 {
            -by
        } else {
            by
        }
    }

    fn add(&mut self, port: usize, by: i32) {
        let address = &mut self.address[port];
        *address = address.wrapping_add(by as u32) & (VRAM_SIZE as u32 - 1);
    }

    /// Port `port`'s address a nibble on, or back (`down`): a byte on as
    /// the high nibble's passed, or back as the low one's.
    fn nibble_step(&mut self, port: usize, down: bool) {
        if self.nibble[port] {
            if !down {
                self.add(port, 1);
            }
            self.nibble[port] = false;
        } else {
            if down {
                self.add(port, -1);
            }
            self.nibble[port] = true;
        }
    }

    /// Port `port`'s address stepped on, as its ADDRx_H says, as FX steps it
    /// (by nibbles, by the 16-bit hop), and for a read or a `write`, ADDR1
    /// as FX's helpers have it: where it was.
    fn advance(&mut self, port: usize, write: bool) -> usize {
        let address = self.address[port];
        let mut by = self.increment(port);
        if self.fx.four_bit && self.nibble_steps[port] && by == 0 {
            self.nibble_step(port, self.stepping[port] & 0x08 != 0);
        }
        if port == 1 && self.fx.hop {
            let aligned = self.fx.hop_align == address & 3;
            by = match by {
                4 => {
                    if aligned {
                        1
                    } else {
                        3
                    }
                }
                320 => {
                    if aligned {
                        1
                    } else {
                        319
                    }
                }
                by => by,
            };
        }
        self.add(port, by);
        let fx = &mut self.fx;
        match fx.mode {
            // The line: ADDR1 on by ADDR0's increment too, as X carries.
            1 if port == 1 => {
                fx.x = fx.x.wrapping_add(fx.x_increment);
                if fx.x & 0x1_0000 != 0 {
                    fx.x &= !0x1_0000;
                    if fx.four_bit && self.nibble_steps[0] {
                        self.nibble_step(1, self.stepping[0] & 0x08 != 0);
                    }
                    self.add(1, self.increment(0));
                }
            }
            // The polygon: X1 and X2 on, and ADDR1 at ADDR0 and X1.
            2 if !write => {
                fx.x = fx.x.wrapping_add(fx.x_increment);
                fx.y = fx.y.wrapping_add(fx.y_increment);
                fx.fill_length = ((fx.y as i32 >> 16) - (fx.x as i32 >> 16)) as u16;
                if port == 0 && fx.cycling && !fx.fill {
                    fx.index = (fx.index + 1) & 3;
                }
                if port == 1 {
                    let x = fx.x;
                    let (shift, nibble) = if fx.four_bit {
                        (17, x >> 16 & 1 != 0)
                    } else {
                        (16, false)
                    };
                    self.address[1] =
                        self.address[0].wrapping_add(x >> shift) & (VRAM_SIZE as u32 - 1);
                    self.nibble[1] = nibble;
                }
            }
            3 if port == 1 && !write => {
                fx.x = fx.x.wrapping_add(fx.x_increment);
                fx.y = fx.y.wrapping_add(fx.y_increment);
            }
            _ => {}
        }
        address as usize
    }

    /// The affine helper's dot at X and Y, its tile's from its map, or
    /// tile 0's past its edge if it clips, for ADDR1, fetched.
    fn affine_fetch(&mut self) {
        let fx = &self.fx;
        if fx.mode != 3 {
            return;
        }
        let (mut x, mut y) = ((fx.x >> 19) & 0xFF, (fx.y >> 19) & 0xFF);
        let (dot_x, dot_y) = ((fx.x >> 16) & 7, (fx.y >> 16) & 7);
        if !fx.clip {
            (x, y) = (x & (fx.map_size - 1), y & (fx.map_size - 1));
        }
        let four = fx.four_bit as u32;
        let within = ((dot_y << (3 - four)) + (dot_x >> four)) as usize;
        let address = if x >= fx.map_size || y >= fx.map_size {
            fx.tile_base + within
        } else {
            let tile = self.peek(fx.map_base + (y * fx.map_size + x) as usize) as usize;
            fx.tile_base + (tile << (6 - four)) + within
        };
        self.nibble[1] = fx.four_bit && dot_x & 1 != 0;
        self.address[1] = address as u32 & (VRAM_SIZE as u32 - 1);
        self.fetch(1);
    }

    /// A byte read, `value`, by a port at `nibble`, into the cache at its
    /// index: in 4-bit mode, the nibble.
    fn fill_cache(&mut self, value: u8, nibble: bool) {
        let fx = &mut self.fx;
        let at = fx.index;
        if fx.four_bit {
            let read = if nibble { value << 4 } else { value & 0xF0 };
            if fx.nibble_index {
                fx.cache[at] = fx.cache[at] & 0xF0 | read >> 4;
                (fx.nibble_index, fx.index) = (false, (at + 1) & 3);
            } else {
                fx.cache[at] = fx.cache[at] & 0x0F | read;
                fx.nibble_index = true;
            }
        } else {
            fx.cache[at] = value;
            fx.index = if fx.pairs {
                at & 2 | (at + 1) & 1
            } else {
                (at + 1) & 3
            };
        }
    }

    /// DATAx written, `value`: as it is, or through the cache, the
    /// multiplier's product, or a byte of it cycled; the port stepped on.
    fn write_data(&mut self, port: usize, value: u8) {
        // A 2-bit poke: two bits of the cache's byte, as `value`'s top two
        // say which, into what ADDR1 had.
        if self.fx.poking && self.fx.mode != 0 {
            self.fx.poking = false;
            let mask = 0xC0 >> (2 * (value >> 6));
            let at = self.address[1] as usize;
            self.vram[at] = self.fx.cache[self.fx.index] & mask | self.fetched[1] & !mask;
            return;
        }
        let nibble = self.nibble[port];
        let address = self.advance(port, true);
        let fx = &self.fx;
        let byte = if fx.cycling {
            fx.cache[fx.index]
        } else {
            value
        };
        if fx.cache_write {
            let bytes = if fx.cycling {
                [byte; 4]
            } else if fx.multiplier {
                let product = fx.product();
                let sum = if fx.subtract {
                    fx.accumulator.wrapping_sub(product)
                } else {
                    fx.accumulator.wrapping_add(product)
                };
                sum.to_le_bytes()
            } else {
                fx.cache
            };
            let at = address & 0x1_FFFC;
            for (n, byte) in bytes.into_iter().enumerate() {
                // A nibble mask's 1, its nibble left: as written, or each
                // that's 0, transparent.
                let mask = match (fx.transparent, fx.four_bit) {
                    (false, _) => value >> (2 * n) & 3,
                    (true, true) => ((byte & 0xF0 == 0) as u8) << 1 | (byte & 0x0F == 0) as u8,
                    (true, false) => {
                        if byte != 0 {
                            0
                        } else {
                            3
                        }
                    }
                };
                if fx.transparent && byte == 0 {
                    continue;
                }
                let old = self.vram[at + n];
                self.vram[at + n] = match mask {
                    0 => byte,
                    1 => old & 0x0F | byte & 0xF0,
                    2 => old & 0xF0 | byte & 0x0F,
                    _ => old,
                };
            }
        } else {
            self.poke(address, nibble, byte);
        }
        self.fetch(port);
    }

    fn fetch(&mut self, port: usize) {
        self.fetched[port] = self.peek(self.address[port] as usize);
    }

    /// Whether it's sending NTSC or RGB, interlaced or 240P, not VGA.
    fn tv(&self) -> bool {
        self.composer[0] & 2 != 0
    }

    /// NTSC's or RGB's: its field's half-lines; the field being sent; and
    /// the line of its half-line, from the field's line 0, the picture's
    /// even lines its first halves (before it, those of the last field).
    fn field(&self) -> (u16, usize, i32) {
        let progressive = self.composer[0] & 0x08 != 0;
        let size = if progressive { FIELD_240P } else { FIELD };
        let field = (self.line >= size) as usize;
        let half = self.line as i32 - size as i32 * field as i32;
        // 240P's fields alike, each a whole number of lines.
        let y = half - FIELD_TOPS[if progressive { 0 } else { field }];
        (size, field, if y < 0 { y + size as i32 } else { y })
    }

    /// The line being scanned out, as SCANLINE has it: 511 past 511; in
    /// NTSC or RGB, even in the first field, odd in the second.
    fn scanline(&self) -> u16 {
        let line = if self.tv() {
            let (_, field, y) = self.field();
            (y as u16 & !1) | field as u16
        } else {
            self.line
        };
        line.min(511)
    }

    /// A read of register `reg`, 0 to 31.
    pub fn read(&mut self, reg: u8) -> u8 {
        let port = self.addrsel;
        match reg & 0x1F {
            0x00 => self.address[port] as u8,
            0x01 => (self.address[port] >> 8) as u8,
            0x02 => {
                self.stepping[port]
                    | (self.nibble_steps[port] as u8) << 2
                    | (self.nibble[port] as u8) << 1
                    | (self.address[port] >> 16) as u8
            }
            reg @ (0x03 | 0x04) => {
                let port = (reg - 3) as usize;
                let (value, nibble) = (self.fetched[port], self.nibble[port]);
                self.advance(port, false);
                if port == 1 && self.fx.mode == 3 {
                    self.affine_fetch();
                } else {
                    self.fetch(port);
                }
                if self.fx.fill {
                    self.fill_cache(value, nibble);
                }
                value
            }
            0x05 => self.dcsel << 1 | port as u8,
            0x06 => {
                (self.irq_line >> 1 & 0x80) as u8 | (self.scanline() >> 2 & 0x40) as u8 | self.ien
            }
            0x07 => self.isr | self.aflow(),
            0x08 => self.scanline() as u8,
            reg @ 0x09..=0x0C => {
                let n = (reg - 9) as usize;
                match self.dcsel {
                    // DC_VIDEO's bit 7, the field: NTSC's or RGB's, or in
                    // VGA, the line's odd or even.
                    0 if n == 0 => {
                        let field = if self.tv() {
                            self.field().1 as u8
                        } else {
                            self.line as u8 & 1
                        };
                        self.composer[0] & 0x7F | field << 7
                    }
                    0 | 1 => self.composer[self.dcsel as usize * 4 + n],
                    // FX_CTRL; the fill length; the accumulator, reset or
                    // accumulated as it's read. The rest are written only.
                    2 if n == 0 => self.fx.regs[0],
                    5 if n == 2 => self.fx.fill_low(),
                    5 if n == 3 => self.fx.fill_high(),
                    6 if n == 0 => {
                        self.fx.accumulator = 0;
                        VERSION[n]
                    }
                    6 if n == 1 => {
                        self.fx.accumulate();
                        VERSION[n]
                    }
                    _ => VERSION[n],
                }
            }
            reg @ 0x0D..=0x13 => self.layers[0][(reg - 0x0D) as usize],
            reg @ 0x14..=0x1A => self.layers[1][(reg - 0x14) as usize],
            // AUDIO_CTRL: the FIFO empty, as it always is.
            0x1B => self.pcm.ctrl(),
            0x1C => self.pcm.rate(),
            0x1D => 0,
            0x1E => {
                let received = self.spi.received;
                if self.spi.auto_tx {
                    self.spi.send(0xFF);
                }
                received
            }
            _ => {
                let spi = &self.spi;
                ((spi.busy > 0) as u8) << 7
                    | (spi.auto_tx as u8) << 2
                    | (spi.slow as u8) << 1
                    | spi.select as u8
            }
        }
    }

    /// A write of `value` to register `reg`, 0 to 31.
    pub fn write(&mut self, reg: u8, value: u8) {
        let port = self.addrsel;
        match reg & 0x1F {
            0x00 => {
                // In the 2-bit polygon mode, ADDR1's 2 bits, for a poke.
                let fx = &mut self.fx;
                if fx.two_bit && fx.four_bit && fx.mode == 2 && port == 1 {
                    fx.poking = true;
                    self.address[1] = self.address[1] & 0x1_FFFC | (value & 3) as u32;
                } else {
                    self.address[port] = self.address[port] & 0x1FF00 | value as u32;
                    if fx.hop && port == 1 {
                        fx.hop_align = (value & 3) as u32;
                    }
                }
                self.fetch(port);
            }
            0x01 => {
                self.address[port] = self.address[port] & 0x100FF | (value as u32) << 8;
                self.fetch(port);
            }
            0x02 => {
                self.address[port] = self.address[port] & 0x0FFFF | ((value & 1) as u32) << 16;
                self.stepping[port] = value & 0xF8;
                self.nibble[port] = value & 0x02 != 0;
                self.nibble_steps[port] = value & 0x04 != 0;
                self.fetch(port);
            }
            reg @ (0x03 | 0x04) => self.write_data((reg - 3) as usize, value),
            0x05 => {
                if value & 0x80 != 0 {
                    self.reset();
                }
                self.dcsel = value >> 1 & 0x3F;
                self.addrsel = (value & 1) as usize;
            }
            0x06 => {
                self.irq_line = self.irq_line & 0xFF | ((value >> 7) as u16) << 8;
                self.ien = value & 0x0F;
            }
            // Its collisions are read only, as the docs have them (x16emu
            // clears them too).
            0x07 => self.isr &= !(value & 0x0F),
            0x08 => self.irq_line = self.irq_line & 0x100 | value as u16,
            reg @ 0x09..=0x0C => {
                let n = (reg - 9) as usize;
                match self.dcsel {
                    // DC_VIDEO's field bit is read only. 240P begun, the
                    // picture cleared, its odd lines left dark, as x16emu.
                    0 if n == 0 => {
                        let before = self.composer[0];
                        let tv_240p = |video: u8| video & 2 != 0 && video & 8 != 0;
                        if tv_240p(value) && !tv_240p(before) {
                            self.picture.fill(0);
                        }
                        self.composer[0] = before & 0x80 | value & 0x7F;
                    }
                    0 | 1 => self.composer[self.dcsel as usize * 4 + n] = value,
                    2..=6 => {
                        let moved = self.fx.write((self.dcsel as usize - 2) * 4 + n, value);
                        if moved {
                            self.affine_fetch();
                        }
                    }
                    _ => {}
                }
            }
            reg @ 0x0D..=0x13 => self.layers[0][(reg - 0x0D) as usize] = value,
            reg @ 0x14..=0x1A => self.layers[1][(reg - 0x14) as usize] = value,
            0x1B => self.pcm.write_ctrl(value),
            0x1C => self.pcm.write_rate(value),
            0x1D => self.pcm.write_data(value),
            0x1E => self.spi.send(value),
            _ => {
                if value & 1 != 0 && !self.spi.select {
                    self.spi.card.select();
                }
                self.spi.select = value & 1 != 0;
                self.spi.slow = value & 2 != 0;
                self.spi.auto_tx = value & 4 != 0;
            }
        }
    }

    /// The SD card on its SPI port.
    pub fn sd_card(&mut self) -> &mut SdCard {
        &mut self.spi.card
    }

    /// Whether it's holding the IRQ line: an interrupt it has that's on.
    pub fn irq(&self) -> bool {
        (self.isr | self.aflow()) & self.ien != 0
    }

    /// Whether a frame's ended since it was last asked.
    pub fn frame_ended(&mut self) -> bool {
        std::mem::take(&mut self.frame_ended)
    }

    /// A CPU cycle: its SPI's; and a line, as one ends: drawn, the next
    /// begun, and its interrupts.
    /// AFLOW, as the PCM's FIFO has it.
    fn aflow(&self) -> u8 {
        if self.pcm.low() { AFLOW } else { 0 }
    }

    /// Its sound as it is, left and right: the PSG's and the PCM's, signed
    /// 16 bits each.
    pub fn sound(&self) -> (i32, i32) {
        self.sound
    }

    pub fn tick(&mut self) {
        self.spi.tick();
        self.sample_clock += CLOCKS_A_CYCLE;
        if self.sample_clock >= SAMPLE_CYCLES {
            self.sample_clock -= SAMPLE_CYCLES;
            let (psg, pcm) = (self.psg.sample(), self.pcm.sample());
            self.sound = (psg.0 as i32 + pcm.0 as i32, psg.1 as i32 + pcm.1 as i32);
        }
        self.cycles += CLOCKS_A_CYCLE;
        let length = if self.tv() { HALF_LINE } else { VGA_LINE };
        if self.cycles < length {
            return;
        }
        self.cycles -= length;
        if self.tv() {
            self.half_line();
        } else {
            self.vga_line();
        }
    }

    /// A VGA line ended: drawn, and the next begun.
    fn vga_line(&mut self) {
        let y = self.line;
        self.scan(y, y, false);
        self.layers_before = self.layers;
        self.vscale_before = self.composer[2];
        self.line += 1;
        if self.line >= LINES {
            self.line = 0;
            self.frame_ended = true;
        }
        self.begun(self.line as i32, self.irq_line);
    }

    /// An NTSC or RGB half-line ended: the first half of one of its
    /// field's lines drawn, its line of the picture; and the next begun,
    /// the field ended with its last.
    fn half_line(&mut self) {
        let (size, field, y) = self.field();
        let progressive = self.composer[0] & 0x08 != 0;
        if y & 1 == 0 && y < HEIGHT as i32 {
            let shown = y as u16 | field as u16;
            self.scan(shown, if progressive { y as u16 } else { shown }, true);
            self.layers_before = self.layers;
            self.vscale_before = self.composer[2];
        }
        self.line += 1;
        if self.line == size || self.line >= size * 2 {
            self.frame_ended = true;
            if self.line >= size * 2 {
                self.line = 0;
            }
        }
        let (_, _, y) = self.field();
        self.begun(y, self.irq_line & !1);
    }

    /// Line `y` of the picture begun (one of a field's): VSYNC as 480
    /// does, the sprites' collisions with it; LINE as `irq_line` does.
    fn begun(&mut self, y: i32, irq_line: u16) {
        if y == HEIGHT as i32 {
            let collisions = std::mem::take(&mut self.collisions);
            self.isr = self.isr & 0x0F | collisions | VSYNC;
            if collisions != 0 {
                self.isr |= SPRCOL;
            }
        }
        if y == irq_line as i32 {
            self.isr |= LINE;
        }
    }

    /// Line `y`, as it ends: the picture's line it shows, stepped on as
    /// VSCALE says, and drawn, as line `row`, if it's one of the picture's.
    /// As VERA's composer has it (its Verilog's, not x16emu's since R49):
    /// the picture's line 0 on the first line at or past VSTART; then a
    /// step each line after one in VSTART to VSTOP, to the picture's line
    /// 480. `interlaced`, a field's, every other line: two lines' steps a
    /// line, and its first half a line on, a field and VSTART odd and even.
    fn scan(&mut self, y: u16, row: u16, interlaced: bool) {
        let [_, _, _, _, _, _, vstart, vstop] = self.composer;
        let (vstart, vstop) = ((vstart as u16) << 1, (vstop as u16) << 1);
        let vscale = self.vscale_before as u32;
        let (first, before, step) = if interlaced {
            (y < 2, y.wrapping_sub(2), vscale << 10)
        } else {
            (y == 0, y.wrapping_sub(1), vscale << 9)
        };
        if first {
            self.started = false;
        }
        if !self.started && y >= vstart {
            self.started = true;
            let odd = interlaced && (y ^ vstart) & 1 != 0;
            self.picture_line = if odd { vscale << 9 } else { 0 };
        } else if self.started
            && (vstart..vstop).contains(&before)
            && self.picture_line >> 16 < HEIGHT as u32
        {
            self.picture_line += step;
        }
        if y as usize >= HEIGHT {
            return;
        }
        if self.picture.len() != WIDTH * HEIGHT {
            self.picture = vec![0; WIDTH * HEIGHT];
        }
        let mut picture_y = self.picture_line >> 16;
        if picture_y >= HEIGHT as u32 {
            picture_y = HEIGHT as u32 - (y as u32 & 1);
        }
        // Its sprites, drawn as it's left out too, for their collisions.
        let mut sprites = ([0u8; WIDTH], [0u8; WIDTH]);
        if self.composer[0] & 0x40 != 0 {
            self.sprite_line(picture_y, &mut sprites);
        }
        if !self.leave_out {
            self.draw(
                row as usize,
                picture_y,
                &sprites,
                vstart as usize,
                vstop as usize,
            );
        }
    }

    /// The picture's line `y`, `picture_y` of what's shown, drawn: its
    /// sprites (their colours and Z-depths) and layers composed, scaled,
    /// inside the border.
    fn draw(
        &mut self,
        y: usize,
        picture_y: u32,
        (sprites, depths): &([u8; WIDTH], [u8; WIDTH]),
        vstart: usize,
        vstop: usize,
    ) {
        let [video, hscale, _, border, hstart, hstop, ..] = self.composer;
        let mut layers = [[0u8; WIDTH]; 2];
        for (n, line) in layers.iter_mut().enumerate() {
            if video & (0x10 << n) != 0 {
                self.layer_line(n, picture_y, line);
            }
        }
        let mut indexes = [border; WIDTH];
        if (vstart..vstop).contains(&y) {
            let (hstart, hstop) = (
                ((hstart as usize) << 2).min(WIDTH),
                ((hstop as usize) << 2).min(WIDTH),
            );
            let mut x_fp = 0u32;
            for index in indexes.iter_mut().take(hstop).skip(hstart) {
                let x = (x_fp >> 16) as usize;
                *index = if x < WIDTH {
                    let (sprite, l0, l1) = (sprites[x], layers[0][x], layers[1][x]);
                    // Bottom to top: the sprite at Z 1, layer 0, the
                    // sprite at Z 2, layer 1, the sprite at Z 3.
                    let order = match depths[x] {
                        3 => [sprite, l1, l0],
                        2 => [l1, sprite, l0],
                        1 => [l1, l0, sprite],
                        _ => [l1, l0, 0],
                    };
                    order.into_iter().find(|&c| c != 0).unwrap_or(0)
                } else {
                    0
                };
                x_fp += (hscale as u32) << 9;
            }
        }
        let colours = self.colours(video);
        let line = &mut self.picture[y * WIDTH..(y + 1) * WIDTH];
        for (pixel, &index) in line.iter_mut().zip(indexes.iter()) {
            *pixel = colours[index as usize];
        }
        // NTSC: a TV's edge, dimmed to a quarter.
        if video & 3 == 2 {
            for (x, pixel) in line.iter_mut().enumerate() {
                if !SAFE_X.contains(&x) || !SAFE_Y.contains(&y) {
                    *pixel = (*pixel & 0xFCFCFC) >> 2;
                }
            }
        }
    }

    /// The sprites' line `y` of the picture: each dot's colour, 0 where
    /// it's clear, and Z-depth. Lower sprites first, a dot drawn over only
    /// by one deeper; the collisions found, those whose masks share a bit
    /// at a dot. Its renderer stops as its clocks run out.
    fn sprite_line(&mut self, y: u32, (colours, depths): &mut ([u8; WIDTH], [u8; WIDTH])) {
        let mut masks = [0u8; WIDTH];
        let mut budget = SPRITE_BUDGET;
        let y = y as i32;
        'sprites: for n in 0..128 {
            budget -= 1;
            if budget == 0 {
                break;
            }
            let at = SPRITES + n * 8;
            let sprite = Sprite::of(&self.vram[at..at + 8]);
            let (width, height) = (1i32 << sprite.width, 1i32 << sprite.height);
            if sprite.z == 0 || y < sprite.y || y >= sprite.y + height {
                continue;
            }
            let mut row = (y - sprite.y) as usize;
            if sprite.vflip {
                row = height as usize - 1 - row;
            }
            let row_shift = sprite.width - !sprite.bytes as u32;
            let base = sprite.address + (row << row_shift);
            // A fetch is 32 bits: 8 dots of 4 bits, 4 of 8.
            let fetch = if sprite.bytes { 3 } else { 7 };
            for dx in 0..width {
                let x = sprite.x + dx;
                if !(0..WIDTH as i32).contains(&x) {
                    continue;
                }
                if dx & fetch == 0 {
                    budget -= 1;
                    if budget == 0 {
                        break 'sprites;
                    }
                }
                budget -= 1;
                if budget == 0 {
                    break 'sprites;
                }
                let sx = if sprite.hflip { width - 1 - dx } else { dx } as usize;
                let mut colour = if sprite.bytes {
                    self.peek(base + sx)
                } else {
                    self.peek(base + sx / 2) >> (4 - (sx & 1) * 4) & 0x0F
                };
                if colour == 0 {
                    continue;
                }
                let x = x as usize;
                self.collisions |= masks[x] & sprite.mask;
                masks[x] |= sprite.mask;
                if sprite.z > depths[x] {
                    if colour < 16 {
                        colour += sprite.offset;
                    }
                    colours[x] = colour;
                    depths[x] = sprite.z;
                }
            }
        }
    }

    /// The palette's colours as 0xRRGGBB, as DC_VIDEO has them shown: none
    /// with its output off; grey with NTSC's chroma off.
    fn colours(&self, video: u8) -> [u32; 256] {
        let mut colours = [0u32; 256];
        if video & 3 == 0 {
            return colours;
        }
        let grey = video & 7 == 6;
        for (n, colour) in colours.iter_mut().enumerate() {
            let entry = u16::from_le_bytes([self.palette[n * 2], self.palette[n * 2 + 1]]);
            let [mut r, mut g, mut b] =
                [entry >> 8 & 0xF, entry >> 4 & 0xF, entry & 0xF].map(|c| c as u32 * 17);
            if grey {
                (r, g, b) = ((r + g + b) / 3, (r + g + b) / 3, (r + g + b) / 3);
            }
            *colour = r << 16 | g << 8 | b;
        }
        colours
    }

    /// Layer `n`'s line `y` of the picture, its colour indexes, 0 where
    /// it's clear: its registers as they were as the line before ended,
    /// but its vertical scroll as it is (and a bitmap's palette offset).
    fn layer_line(&self, n: usize, y: u32, line: &mut [u8; WIDTH]) {
        let layer = Layer::of(&self.layers_before[n]);
        if layer.bitmap {
            self.bitmap_line(&layer, self.layers[n][4] & 0x0F, y, line);
            return;
        }
        let now = Layer::of(&self.layers[n]);
        let width_max = (1 << (layer.map_width + layer.tile_width)) - 1;
        let height_max = (1 << (layer.map_height + layer.tile_height)) - 1;
        let at_y = (y + now.vscroll) & height_max;
        let row = (at_y >> layer.tile_height) << layer.map_width;
        let (tile_width_max, tile_height_max) =
            ((1 << layer.tile_width) - 1, (1 << layer.tile_height) - 1);
        let tile_size = layer.tile_width + layer.tile_height + layer.depth - 3;
        let bits = 1 << layer.depth;
        let in_byte = (8 >> layer.depth) - 1;
        for (x, colour) in line.iter_mut().enumerate() {
            let at_x = (x as u32 + layer.hscroll) & width_max;
            let map = layer.map_base + ((row + (at_x >> layer.tile_width)) << 1) as usize;
            let (entry, attributes) = (self.peek(map), self.peek(map + 1));
            let (mut xx, mut yy) = (at_x & tile_width_max, at_y & tile_height_max);
            if layer.depth == 0 {
                let tile = (entry as usize) << tile_size;
                let row_bytes = (yy << layer.tile_width) >> 3;
                let dots = self.peek(layer.tile_base + tile + (row_bytes + (xx >> 3)) as usize);
                let on = dots >> (7 - (xx & 7)) & 1 != 0;
                let (fg, bg) = if layer.t256c {
                    (attributes, 0)
                } else {
                    (attributes & 0x0F, attributes >> 4)
                };
                *colour = if on { fg } else { bg };
                continue;
            }
            if attributes & 0x04 != 0 {
                xx ^= tile_width_max;
            }
            if attributes & 0x08 != 0 {
                yy ^= tile_height_max;
            }
            let tile = ((entry as usize) | ((attributes & 3) as usize) << 8) << tile_size;
            let row_bytes = yy << (layer.tile_width + layer.depth - 3);
            let byte = self
                .peek(layer.tile_base + tile + (row_bytes + ((xx << layer.depth) >> 3)) as usize);
            let shift = 8 - bits - (xx & in_byte) * bits;
            let mut index = byte >> shift & ((1u16 << bits) - 1) as u8;
            if (1..16).contains(&index) {
                index += attributes & 0xF0;
                if layer.t256c {
                    index |= 0x80;
                }
            }
            *colour = index;
        }
    }

    /// A bitmap layer's line `y`: 320 dots across or 640, `offset` its
    /// palette offset.
    fn bitmap_line(&self, layer: &Layer, offset: u8, y: u32, line: &mut [u8; WIDTH]) {
        let width = if layer.tile_width == 4 { 640 } else { 320 };
        let bits = 1 << layer.depth;
        let in_byte = (8 >> layer.depth) - 1;
        let row = (y % HEIGHT as u32 * width * bits) >> 3;
        for (x, colour) in line.iter_mut().enumerate() {
            let xx = x as u32 % width;
            let byte = self.peek(layer.tile_base + (row + ((xx * bits) >> 3)) as usize);
            let shift = 8 - bits - ((xx & in_byte) << layer.depth);
            let mut index = byte >> shift & ((1u16 << bits) - 1) as u8;
            if (1..16).contains(&index) {
                index += offset << 4;
                if layer.t256c {
                    index |= 0x80;
                }
            }
            *colour = index;
        }
    }

    /// Layer `n`'s text map, if it's a text layer: its base, its width in
    /// cells, and the cells shown across and down.
    pub fn text_map(&self, n: usize) -> Option<(usize, usize, usize, usize)> {
        let layer = Layer::of(&self.layers[n]);
        if layer.bitmap || layer.depth != 0 || self.composer[0] & (0x10 << n) == 0 {
            return None;
        }
        let [_, hscale, vscale, ..] = self.composer;
        let across = (WIDTH * hscale.max(1) as usize / 128) >> layer.tile_width;
        let down = (HEIGHT * vscale.max(1) as usize / 128) >> layer.tile_height;
        Some((layer.map_base, 1 << layer.map_width, across, down))
    }
}

impl Default for Vera {
    fn default() -> Vera {
        Vera::new()
    }
}
