//! The PPU in performance mode, for what can't keep up with a dot at a time:
//! a line at a time, the whole of it drawn as it starts, from what the
//! registers say then, its sprites found and fetched at the end of the line
//! before. The dots are only counted, so the vertical blank, its flags and
//! its NMI are as they are otherwise, and a sprite 0 hit's seen at the dot of
//! it; and a mapper still sees the fetches, in their order, a line at a time.
//! What's lost is anything done to the picture partway along a line, which
//! shows on the next.

use super::{BORDER, DOTS, Ppu, Sprite, VBLANK, WIDTH, palette_index};
use crate::machines::nes::cartridge::Cartridge;
use crate::machines::nes::region::Region;

/// Tiles fetched a line: 32, and one more for fine X to go into.
const TILES: usize = 33;

/// Each byte's bits, one to a byte, the leftmost lowest: the 8 dots of a plane.
const SPREAD: [u64; 256] = {
    let mut table = [0; 256];
    let mut byte = 0;
    while byte < 256 {
        let mut bit = 0;
        while bit < 8 {
            table[byte] |= ((byte as u64 >> (7 - bit)) & 1) << (bit * 8);
            bit += 1;
        }
        byte += 1;
    }
    table
};

impl Ppu {
    /// A dot, counted, and whatever's on it.
    pub(super) fn quick_tick(&mut self, cart: &mut Cartridge) {
        self.settle_v(cart);
        match self.dot {
            1 => self.line_starts(cart),
            257 => self.line_ends(cart),
            304 if self.line == self.pre_render() && self.rendering() => {
                self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0);
            }
            338 | 339 if self.line == self.pre_render() => self.pre_render_end(),
            _ => {}
        }
        self.next_dot();
        self.nothing_until = match self.dot {
            0..=1 => 1,
            2..=257 => 257,
            _ if self.line != self.pre_render() => DOTS - 1,
            258..=304 => 304,
            305..=338 => 338,
            339 => 339,
            _ => DOTS - 1,
        };
    }

    fn line_starts(&mut self, cart: &mut Cartridge) {
        if self.line < 240 {
            // A mapper that counts lines knows one by the same nametable
            // byte read three times, as the end of a line does.
            if self.rendering() && cart.counts_lines() {
                for _ in 0..3 {
                    self.read(0x2000 | (self.v & 0x0FFF), cart);
                }
            }
            self.draw_line(cart);
            return;
        }
        if self.line == VBLANK {
            self.vblank_starts();
        }
        let pre_render = self.line == self.pre_render();
        if pre_render {
            self.vblank_ends();
        }
        // What the lines' first fetches would be: a mapper counts them.
        if self.rendering() && (pre_render || self.line == 240) {
            self.read(self.pattern_address(), cart);
        }
    }

    /// Down a line, back to the left, and the next line's sprites.
    fn line_ends(&mut self, cart: &mut Cartridge) {
        let pre_render = self.line == self.pre_render();
        if self.rendering() && (self.line < 240 || pre_render) {
            self.next_line();
            self.v = (self.v & !0x041F) | (self.t & 0x041F);
            self.fetch_sprites(cart, pre_render);
        }
    }

    /// The sprites on the next line, 8 at most, a ninth setting the overflow
    /// flag; and their patterns fetched. A mapper counts those fetches too,
    /// so a slot with no sprite in it still makes one.
    fn fetch_sprites(&mut self, cart: &mut Cartridge, none: bool) {
        self.oam_addr = 0;
        self.sprites_laid_out = false;
        self.zero_on_line = false;
        let height = self.sprite_height();
        let mut found = 0;
        for at in (0..256).step_by(4) {
            let y = self.oam[at];
            if none || !(0..height).contains(&(self.line as i16 - y as i16)) {
                continue;
            }
            if found == 8 {
                if !self.overflow {
                    self.overflow = true;
                    self.overflowed_at = self.dots;
                }
                break;
            }
            let (tile, attributes, x) = (self.oam[at + 1], self.oam_byte(at as u8 + 2), self.oam[at + 3]);
            let (addr, _) = self.sprite_address(y, tile, attributes);
            let (mut low, mut high) = (self.sprite_plane(addr, cart), self.sprite_plane(addr + 8, cart));
            if attributes & 0x40 != 0 {
                (low, high) = (low.reverse_bits(), high.reverse_bits());
            }
            self.sprites[found] = Sprite { x, low, high, attributes };
            self.zero_on_line |= at == 0;
            found += 1;
        }
        self.sprite_count = found;
        for _ in found..8 {
            let (addr, _) = self.sprite_address(0xFF, 0xFF, 0);
            self.sprite_plane(addr, cart);
        }
    }

    fn sprite_plane(&mut self, addr: u16, cart: &mut Cartridge) -> u8 {
        cart.sprite_fetch = true;
        let plane = self.read(addr, cart);
        cart.sprite_fetch = false;
        plane
    }

    /// The tiles of the line fetched, each dot of them a byte, the colour
    /// and the palette above it, from the leftmost; v as it was.
    fn fetch_tiles(&mut self, cart: &mut Cartridge, dots: &mut [u8; TILES * 8]) {
        let v = self.v;
        let watched = cart.watches_fetches();
        let pages = [0x2000, 0x2400, 0x2800, 0x2C00].map(|addr| cart.nametable_page(addr) * 0x400);
        for (at, tile) in dots.chunks_exact_mut(8).enumerate() {
            // All of a line's fetches being on one dot, a mapper counting
            // them sees no more in the first and last than in the lot.
            let told = watched || at == 0 || at == TILES - 1;
            let v = self.v;
            let attribute = 0x23C0 | (v & 0x0C00) | ((v >> 4) & 0x38) | ((v >> 2) & 0x07);
            let (name, attribute) = if told {
                (self.read(0x2000 | (v & 0x0FFF), cart), self.read(attribute, cart))
            } else {
                let table = pages[(v as usize >> 10) & 3];
                (self.nametables[table + (v as usize & 0x3FF)], self.nametables[table + (attribute as usize & 0x3FF)])
            };
            self.tile = name;
            let palette = ((attribute >> (((v >> 4) & 4) | (v & 2))) & 3) << 2;
            let addr = self.pattern_address();
            let (low, high) = if told {
                (self.read(addr, cart), self.read(addr + 8, cart))
            } else {
                (cart.chr_read(addr & 0x1FFF), cart.chr_read((addr + 8) & 0x1FFF))
            };
            let colours = SPREAD[low as usize] | SPREAD[high as usize] << 1;
            let drawn = (colours | colours >> 1) & 0x0101_0101_0101_0101;
            tile.copy_from_slice(&(colours | drawn * palette as u64).to_le_bytes());
            self.next_tile();
        }
        self.v = v;
    }

    /// A line's fetches as a mapper counts them: its first's and last's.
    fn mention_fetches(&mut self, cart: &mut Cartridge) {
        for _ in 0..2 {
            self.tile = self.read(0x2000 | (self.v & 0x0FFF), cart);
            self.read(0x23C0 | (self.v & 0x0C00), cart);
            let addr = self.pattern_address();
            self.read(addr, cart);
            self.read(addr + 8, cart);
        }
    }

    /// The whole of a line: its background, its sprites over or under it, the
    /// first hit of sprite 0 kept for the dot it comes on.
    fn draw_line(&mut self, cart: &mut Cartridge) {
        // A picture left out: the sprite 0 hit's still seen, and a mapper
        // still sees fetches, but with no one to look there's nothing else.
        let hit_due = self.zero_on_line && !self.sprite_zero_hit && self.mask & 0x18 == 0x18;
        if self.leaving_out && !hit_due && !cart.watches_fetches() {
            if self.rendering() {
                self.mention_fetches(cart);
            }
            return;
        }
        let start = self.line as usize * WIDTH;
        let grey = if self.mask & 0x01 != 0 { 0x30 } else { 0x3F };
        let emphasis = (self.mask as u16 >> 5) << 6;
        if self.rendering() {
            let mut tiles = [0; TILES * 8];
            self.fetch_tiles(cart, &mut tiles);
            let background = &tiles[self.fine_x as usize..];
            if self.mask & 0x10 != 0 && !self.sprites_laid_out {
                self.lay_out_sprites();
            }
            let colours: [u16; 32] = std::array::from_fn(|index| {
                (self.palette[palette_index(0x3F00 | index as u16)] & grey) as u16 | emphasis
            });
            let (backgrounds_from, background_on) = (if self.mask & 0x02 != 0 { 0 } else { 8 }, self.mask & 0x08 != 0);
            let (sprites_from, sprites_on) = (if self.mask & 0x04 != 0 { 0 } else { 8 }, self.mask & 0x10 != 0);
            for x in 0..WIDTH {
                let back = if background_on && x >= backgrounds_from { background[x] } else { 0 };
                let sprite = if sprites_on && x >= sprites_from { self.sprite_dots[x] } else { 0 };
                if sprite & 0x20 != 0 && back != 0 && x != 255 && !self.sprite_zero_hit {
                    self.sprite_zero_hit = true;
                    self.hit_at = self.dots + x as u64 + 1;
                }
                let index = if sprite != 0 && (back == 0 || sprite & 0x10 == 0) { 0x10 | sprite & 0x0F } else { back };
                self.frame[start + x] = colours[index as usize];
            }
        } else {
            let addr = if self.v & 0x3F00 == 0x3F00 { self.v } else { 0x3F00 };
            let colour = (self.palette[palette_index(addr)] & grey) as u16 | emphasis;
            self.frame[start..start + WIDTH].fill(colour);
        }
        if self.region == Region::Pal {
            let left = if self.line == 0 { WIDTH } else { 2 };
            self.frame[start..start + left].fill(BORDER);
            self.frame[start + WIDTH - 2..start + WIDTH].fill(BORDER);
        }
    }
}
