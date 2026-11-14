//! VERA's FX, as its reference (the Programmer's Reference's chapter 10)
//! has it and x16emu (BSD-2) does: ways of reaching video RAM, by its
//! registers at DCSEL 2 to 6, for the CPU's drawing; not a renderer.
//!
//! ADDR1 by a helper, as Addr1 Mode says: the line draw helper, ADDR1
//! stepping by its increment each access and by ADDR0's as the X position
//! (a 16-bit fraction) carries; the polygon filler, X1 and X2 stepped each
//! read, ADDR1 then ADDR0 and X1, the fill length X2 less X1; the affine
//! helper, ADDR1 the X and Y positions' dot in a map of 8 by 8 tiles,
//! stepped each read. In 4-bit mode, an address a nibble at a time.
//!
//! The 32-bit cache: filled as DATA0 or DATA1 is read, a byte or a nibble
//! at its index; written, all of it or a byte four times, to the 4 bytes
//! at the address, as a write's nibble mask says. Transparent writes leave
//! what's 0 unwritten. The multiplier: the cache's two signed 16-bit
//! halves, multiplied, with the accumulator, written in the cache's place.
//! The 16-bit hop: ADDR1 stepping +4 (or +320) by +1 then +3.
//!
//! Its registers, DCSEL 2 to 6's: those read back are FX_CTRL, the fill
//! length, and, reading them, the accumulator reset and accumulated; the
//! rest read as DC_VER does.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fx {
    /// Its registers as written, DCSEL 2 to 6's, 4 each.
    pub regs: [u8; 20],
    /// FX_CTRL's: ADDR1's helper, 4-bit mode, the 16-bit hop, one-byte
    /// cache cycling, cache filling and writing, transparent writes.
    pub mode: u8,
    pub four_bit: bool,
    pub hop: bool,
    pub cycling: bool,
    pub fill: bool,
    pub cache_write: bool,
    pub transparent: bool,
    /// FX_TILEBASE's and FX_MAPBASE's: the affine helper's tiles, whether
    /// it clips, its map and its size; and the 2-bit polygon mode.
    pub tile_base: usize,
    pub clip: bool,
    pub two_bit: bool,
    pub map_base: usize,
    pub map_size: u32,
    /// The next DATA write, ADDR_L written in the 2-bit polygon mode, to
    /// be a 2-bit poke.
    pub poking: bool,
    /// FX_MULT's: the cache's index, a byte's and a nibble's, and whether
    /// it cycles in a pair; the multiplier, and whether it subtracts.
    pub index: usize,
    pub nibble_index: bool,
    pub pairs: bool,
    pub multiplier: bool,
    pub subtract: bool,
    /// The X and Y increments and positions, the X1 and X2 of the polygon
    /// filler: 16.16 positions, the increments 9 bits of fraction under 6
    /// of dots, and 32 times that.
    pub x_increment: u32,
    pub y_increment: u32,
    pub x: u32,
    pub y: u32,
    pub fill_length: u16,
    /// The 16-bit hop's alignment, ADDR1's low 2 bits as ADDR1_L was set.
    pub hop_align: u32,
    pub cache: [u8; 4],
    pub accumulator: i32,
}

impl Fx {
    pub fn new() -> Fx {
        Fx {
            regs: [0; 20],
            mode: 0,
            four_bit: false,
            hop: false,
            cycling: false,
            fill: false,
            cache_write: false,
            transparent: false,
            tile_base: 0,
            clip: false,
            two_bit: false,
            map_base: 0,
            map_size: 2,
            poking: false,
            index: 0,
            nibble_index: false,
            pairs: false,
            multiplier: false,
            subtract: false,
            x_increment: 0,
            y_increment: 0,
            x: 0x8000,
            y: 0x8000,
            fill_length: 0,
            hop_align: 0,
            cache: [0; 4],
            accumulator: 0,
        }
    }

    /// An increment, from its registers, low and high: 15 bits, 6.9 and
    /// signed; times 32, if the high's bit 7 says.
    fn increment(low: u8, high: u8) -> u32 {
        let mut value = ((high as u32 & 0x7F) << 15) + ((low as u32) << 7);
        if high & 0x40 != 0 {
            value |= 0xFFC0_0000;
        }
        if high & 0x80 != 0 { value << 5 } else { value }
    }

    /// The cache's halves multiplied, signed 16 bits each.
    pub fn product(&self) -> i32 {
        let half = |low: u8, high: u8| i16::from_le_bytes([low, high]) as i32;
        half(self.cache[0], self.cache[1]) * half(self.cache[2], self.cache[3])
    }

    /// The product added to the accumulator, or taken from it.
    pub fn accumulate(&mut self) {
        let product = self.product();
        self.accumulator = if self.subtract {
            self.accumulator.wrapping_sub(product)
        } else {
            self.accumulator.wrapping_add(product)
        };
    }

    /// Register `reg`, 0 to 19 (DCSEL 2's first to DCSEL 6's last),
    /// written: whether the affine helper's to fetch again, the positions
    /// changed.
    pub fn write(&mut self, reg: usize, value: u8) -> bool {
        self.regs[reg] = value;
        let regs = self.regs;
        let line_or_polygon = self.mode == 1 || self.mode == 2;
        match reg {
            0 => {
                self.mode = value & 3;
                self.four_bit = value & 0x04 != 0;
                self.hop = value & 0x08 != 0;
                self.cycling = value & 0x10 != 0;
                self.fill = value & 0x20 != 0;
                self.cache_write = value & 0x40 != 0;
                self.transparent = value & 0x80 != 0;
            }
            1 => {
                self.tile_base = ((value & 0xFC) as usize) << 9;
                self.clip = value & 0x02 != 0;
                self.two_bit = value & 0x01 != 0;
            }
            2 => {
                self.map_base = ((value & 0xFC) as usize) << 9;
                self.map_size = 2 << ((value & 3) << 1);
            }
            3 => {
                self.pairs = value & 0x01 != 0;
                self.nibble_index = value & 0x02 != 0;
                self.index = (value as usize >> 2) & 3;
                self.multiplier = value & 0x10 != 0;
                self.subtract = value & 0x20 != 0;
                if value & 0x40 != 0 {
                    self.accumulate();
                }
                if value & 0x80 != 0 {
                    self.accumulator = 0;
                }
            }
            // The increments; their high bytes setting the line's or the
            // polygon's position to half a dot.
            4 | 5 => {
                self.x_increment = Fx::increment(regs[4], regs[5]);
                if reg == 5 && line_or_polygon {
                    self.x = (self.x & 0x07FF_0000) | 0x8000;
                }
            }
            6 | 7 => {
                self.y_increment = Fx::increment(regs[6], regs[7]);
                if reg == 7 && line_or_polygon {
                    self.y = (self.y & 0x07FF_0000) | 0x8000;
                }
            }
            8 => self.x = (self.x & 0x0700_FF80) | (value as u32) << 16,
            9 => {
                self.x = (self.x & 0x00FF_FF00) | ((value as u32 & 7) << 24) | (value as u32 & 0x80)
            }
            10 => self.y = (self.y & 0x0700_FF80) | (value as u32) << 16,
            11 => {
                self.y = (self.y & 0x00FF_FF00) | ((value as u32 & 7) << 24) | (value as u32 & 0x80)
            }
            12 => self.x = (self.x & 0x07FF_0080) | (value as u32) << 8,
            13 => self.y = (self.y & 0x07FF_0080) | (value as u32) << 8,
            16..=19 => self.cache[reg - 16] = value,
            _ => {}
        }
        (8..=11).contains(&reg)
    }

    /// FX_POLY_FILL_L, DCSEL 5's third: the fill length's low bits, or in
    /// 4-bit mode its nibble and X1's; whether it's long, past 15 (or 7);
    /// all 0 but bit 7 for a negative one, as its top two bits say.
    pub fn fill_low(&self) -> u8 {
        let (x, y, length) = (self.x, self.y, self.fill_length as u32);
        let two_bit = self.two_bit && self.mode == 2;
        if length >= 768 {
            return if two_bit { 0x00 } else { 0x80 };
        }
        let value = if self.four_bit && two_bit {
            ((y & 0x8000) >> 8)
                | ((x >> 11) & 0x60)
                | ((x >> 14) & 0x10)
                | ((length & 7) << 1)
                | ((x & 0x8000) >> 15)
        } else if self.four_bit {
            ((length & 0xFFF8 != 0) as u32) << 7
                | ((x >> 11) & 0x60)
                | ((x >> 14) & 0x10)
                | ((length & 7) << 1)
        } else {
            ((length & 0xFFF0 != 0) as u32) << 7 | ((x >> 11) & 0x60) | ((length & 0x0F) << 1)
        };
        value as u8
    }

    /// FX_POLY_FILL_H, DCSEL 5's last: the fill length's bits 9 to 3.
    pub fn fill_high(&self) -> u8 {
        ((self.fill_length & 0x03F8) >> 2) as u8
    }
}

impl Default for Fx {
    fn default() -> Fx {
        Fx::new()
    }
}
