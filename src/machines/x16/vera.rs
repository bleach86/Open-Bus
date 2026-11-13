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
//! Its FX's ways of writing aren't here yet (its registers, DCSEL 2 to 6,
//! hold what's written, and that's all); NTSC and RGB are shown as VGA. Its sound, the PSG and PCM, isn't
//! here either: the FIFO takes nothing and reads empty. Its SPI port has
//! the SD card on it (`sd_card`), if there is one.

use super::sd_card::SdCard;
use serde::{Deserialize, Serialize};

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;
/// A line's CPU cycles: its 800 dots at 25 MHz, the CPU at 8 MHz; a frame's
/// lines.
pub const LINE_CYCLES: u32 = 256;
pub const LINES: u16 = 525;

const VRAM_SIZE: usize = 0x20000;
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
    /// and DECR, and FX's nibble bits), the byte fetched for DATAx.
    address: [u32; 2],
    stepping: [u8; 2],
    fetched: [u8; 2],
    addrsel: usize,
    dcsel: u8,
    ien: u8,
    isr: u8,
    irq_line: u16,
    layers: [[u8; 7]; 2],
    /// DCSEL 0 and 1's registers: DC_VIDEO, its scales and border, its
    /// active part's edges.
    composer: [u8; 8],
    /// DCSEL 2 to 6's, FX's, as written.
    fx: [u8; 20],
    audio_ctrl: u8,
    audio_rate: u8,
    spi: Spi,
    /// The line being scanned out, and its cycles so far.
    line: u16,
    cycles: u32,
    /// The layers' registers and VSCALE as they were when the line before
    /// ended, for the line drawn now.
    layers_before: [[u8; 7]; 2],
    vscale_before: u8,
    /// The line of the picture shown, 16.16, as VSCALE steps it.
    picture_line: u32,
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
            addrsel: 0,
            dcsel: 0,
            ien: 0,
            isr: 0,
            irq_line: 0,
            layers: [[0; 7]; 2],
            composer: [0; 8],
            fx: [0; 20],
            audio_ctrl: 0,
            audio_rate: 0,
            spi: Spi {
                received: 0xFF,
                ..Spi::default()
            },
            line: 0,
            cycles: 0,
            layers_before: [[0; 7]; 2],
            vscale_before: 0,
            picture_line: 0,
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
        self.fx = [0; 20];
        (self.audio_ctrl, self.audio_rate) = (0, 0);
        for (n, colour) in DEFAULT_PALETTE.iter().enumerate() {
            self.palette[n * 2..n * 2 + 2].copy_from_slice(&colour.to_le_bytes());
        }
        (self.line, self.cycles, self.picture_line) = (0, 0, 0);
        self.collisions = 0;
    }

    /// The byte at `address` in video RAM.
    pub fn peek(&self, address: usize) -> u8 {
        self.vram[address & (VRAM_SIZE - 1)]
    }

    /// `value` written to video RAM at `address`, and to the register
    /// that's there, if one is.
    fn poke(&mut self, address: usize, value: u8) {
        let address = address & (VRAM_SIZE - 1);
        self.vram[address] = value;
        // The PSG's and the sprites' are read where they're written; the
        // palette's kept apart, for RESET to set it again.
        if (PALETTE..SPRITES).contains(&address) {
            self.palette[address - PALETTE] = value;
        }
    }

    /// Port `port`'s address stepped on, as its ADDRx_H says, and the byte
    /// there fetched.
    fn step(&mut self, port: usize) {
        let stepping = self.stepping[port];
        let by = INCREMENTS[stepping as usize >> 4];
        let address = &mut self.address[port];
        *address = if stepping & 0x08 != 0 {
            address.wrapping_sub(by)
        } else {
            address.wrapping_add(by)
        } & (VRAM_SIZE as u32 - 1);
        self.fetch(port);
    }

    fn fetch(&mut self, port: usize) {
        self.fetched[port] = self.peek(self.address[port] as usize);
    }

    /// The line being scanned out, as SCANLINE has it: 511 past 511.
    fn scanline(&self) -> u16 {
        self.line.min(511)
    }

    /// A read of register `reg`, 0 to 31.
    pub fn read(&mut self, reg: u8) -> u8 {
        let port = self.addrsel;
        match reg & 0x1F {
            0x00 => self.address[port] as u8,
            0x01 => (self.address[port] >> 8) as u8,
            0x02 => self.stepping[port] | (self.address[port] >> 16) as u8,
            reg @ (0x03 | 0x04) => {
                let port = (reg - 3) as usize;
                let value = self.fetched[port];
                self.step(port);
                value
            }
            0x05 => self.dcsel << 1 | port as u8,
            0x06 => {
                (self.irq_line >> 1 & 0x80) as u8 | (self.scanline() >> 2 & 0x40) as u8 | self.ien
            }
            0x07 => self.isr | AFLOW,
            0x08 => self.scanline() as u8,
            reg @ 0x09..=0x0C => {
                let n = (reg - 9) as usize;
                match self.dcsel {
                    // DC_VIDEO's bit 7, the field: the line's odd or even.
                    0 if n == 0 => self.composer[0] & 0x7F | (self.line as u8 & 1) << 7,
                    0 | 1 => self.composer[self.dcsel as usize * 4 + n],
                    _ => VERSION[n],
                }
            }
            reg @ 0x0D..=0x13 => self.layers[0][(reg - 0x0D) as usize],
            reg @ 0x14..=0x1A => self.layers[1][(reg - 0x14) as usize],
            // AUDIO_CTRL: the FIFO empty, as it always is.
            0x1B => self.audio_ctrl | 0x40,
            0x1C => self.audio_rate,
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
                self.address[port] = self.address[port] & 0x1FF00 | value as u32;
                self.fetch(port);
            }
            0x01 => {
                self.address[port] = self.address[port] & 0x100FF | (value as u32) << 8;
                self.fetch(port);
            }
            0x02 => {
                self.address[port] = self.address[port] & 0x0FFFF | ((value & 1) as u32) << 16;
                self.stepping[port] = value & 0xFE;
                self.fetch(port);
            }
            reg @ (0x03 | 0x04) => {
                let port = (reg - 3) as usize;
                self.poke(self.address[port] as usize, value);
                self.step(port);
            }
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
                    // DC_VIDEO's field bit is read only.
                    0 if n == 0 => self.composer[0] = self.composer[0] & 0x80 | value & 0x7F,
                    0 | 1 => self.composer[self.dcsel as usize * 4 + n] = value,
                    2..=6 => self.fx[(self.dcsel as usize - 2) * 4 + n] = value,
                    _ => {}
                }
            }
            reg @ 0x0D..=0x13 => self.layers[0][(reg - 0x0D) as usize] = value,
            reg @ 0x14..=0x1A => self.layers[1][(reg - 0x14) as usize] = value,
            0x1B => self.audio_ctrl = value & 0x3F,
            0x1C => self.audio_rate = value,
            0x1D => {}
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
        (self.isr | AFLOW) & self.ien != 0
    }

    /// Whether a frame's ended since it was last asked.
    pub fn frame_ended(&mut self) -> bool {
        std::mem::take(&mut self.frame_ended)
    }

    /// A CPU cycle: its SPI's; and a line, as one ends: drawn, the next
    /// begun, and its interrupts.
    pub fn tick(&mut self) {
        self.spi.tick();
        self.cycles += 1;
        if self.cycles < LINE_CYCLES {
            return;
        }
        self.cycles = 0;
        self.scan(self.line);
        self.layers_before = self.layers;
        self.vscale_before = self.composer[2];
        self.line += 1;
        if self.line == LINES {
            self.line = 0;
            self.frame_ended = true;
        }
        if self.line == HEIGHT as u16 {
            let collisions = std::mem::take(&mut self.collisions);
            self.isr = self.isr & 0x0F | collisions | VSYNC;
            if collisions != 0 {
                self.isr |= SPRCOL;
            }
        }
        if self.line == self.irq_line {
            self.isr |= LINE;
        }
    }

    /// Line `y`, as it ends: the picture's line it shows, stepped on as
    /// VSCALE says, and drawn, if it's one of the picture's.
    fn scan(&mut self, y: u16) {
        let [_, _, _, _, _, _, vstart, vstop] = self.composer;
        let (vstart, vstop) = ((vstart as u16) << 1, (vstop as u16) << 1);
        if y == 0 {
            self.picture_line = 0;
        } else if (vstart..vstop).contains(&y) {
            self.picture_line += (self.vscale_before as u32) << 9;
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
                y as usize,
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
