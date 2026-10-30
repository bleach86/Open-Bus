//! PNG images to what the machine's video chip takes (docs/machine.md):
//! 8x8 tiles of 4-bit pixels, and palettes of 16 colours, each any of the
//! chip's 4096 (`$0RGB`).
//!
//! An image has at most 16 colours, colour 0 transparent:
//!
//! - an indexed PNG: a pixel's index is its colour number, and the PNG's
//!   palette the colours;
//! - any other PNG: transparent pixels are colour 0, and the other colours,
//!   at most 15 once cut to 4 bits a channel, are numbered in the order
//!   they first appear.
//!
//! Tiles run left to right, then down, 32 bytes each. `asm` writes an
//! image as ca65 source, so a build links its art like any other object.
//!
//! A cart icon (`icon`) is 32x32 in up to 30 colours, from any PNG: two
//! palettes of 15, each of its 16 tiles in one of them, worked out here.

use std::fs::File;
use std::path::Path;

/// An image of colour numbers, 0-15, and the `$0RGB` colours they stand for.
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    pub colours: Vec<u16>,
}

fn rgb12(r: u8, g: u8, b: u8) -> u16 {
    (r as u16 >> 4) << 8 | (g as u16 >> 4) << 4 | b as u16 >> 4
}

pub fn read(path: &Path) -> Result<Image, String> {
    let fail = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
    let file = File::open(path).map_err(|e| fail(&e))?;
    let indexed = {
        let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path).map_err(|e| fail(&e))?));
        decoder.read_info().map_err(|e| fail(&e))?.info().color_type == png::ColorType::Indexed
    };
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    if !indexed {
        decoder.set_transformations(png::Transformations::normalize_to_color8());
    }
    let mut reader = decoder.read_info().map_err(|e| fail(&e))?;
    let info = reader.info();
    let (width, height, depth) = (info.width as usize, info.height as usize, info.bit_depth as usize);
    let png_palette: Vec<u16> = info
        .palette
        .as_ref()
        .map(|p| p.chunks(3).map(|c| rgb12(c[0], c[1], c[2])).collect())
        .unwrap_or_default();
    let mut buffer = vec![0; reader.output_buffer_size().ok_or_else(|| fail(&"too large"))?];
    let frame = reader.next_frame(&mut buffer).map_err(|e| fail(&e))?;
    let rows = buffer[..frame.buffer_size()].chunks(frame.line_size);

    let mut pixels = Vec::with_capacity(width * height);
    let colours = if indexed {
        if png_palette.len() > 16 {
            return Err(fail(&format!("{} colours, at most 16", png_palette.len())));
        }
        for row in rows {
            for x in 0..width {
                let bit = x * depth;
                pixels.push(row[bit / 8] >> (8 - depth - bit % 8) & ((1 << depth) - 1) as u8);
            }
        }
        png_palette
    } else {
        let channels = frame.color_type.samples();
        let mut colours = vec![0];
        for row in rows {
            for pixel in row[..width * channels].chunks(channels) {
                let [r, g, b, a] = match channels {
                    1 => [pixel[0], pixel[0], pixel[0], 255],
                    2 => [pixel[0], pixel[0], pixel[0], pixel[1]],
                    3 => [pixel[0], pixel[1], pixel[2], 255],
                    _ => [pixel[0], pixel[1], pixel[2], pixel[3]],
                };
                if a < 128 {
                    pixels.push(0);
                    continue;
                }
                let colour = rgb12(r, g, b);
                let number = match colours[1..].iter().position(|&c| c == colour) {
                    Some(i) => i + 1,
                    None => {
                        colours.push(colour);
                        colours.len() - 1
                    }
                };
                if number > 15 {
                    return Err(fail(&"more than 15 colours besides transparent"));
                }
                pixels.push(number as u8);
            }
        }
        colours
    };
    Ok(Image { width, height, pixels, colours })
}

/// The image as tiles: left to right, then down, 32 bytes each, two pixels
/// a byte with the left one in the high nibble.
pub fn tiles(image: &Image) -> Result<Vec<u8>, String> {
    tiles_in_blocks(image, 8)
}

/// The image as tiles a `block`-pixel square at a time (left to right, then
/// down), and within each block left to right, then down. With 16, each
/// 16x16 block comes out as a 16x16 sprite takes its four tiles: top left,
/// top right, bottom left, bottom right.
pub fn tiles_in_blocks(image: &Image, block: usize) -> Result<Vec<u8>, String> {
    if image.width % block != 0 || image.height % block != 0 {
        return Err(format!("{}x{}: not a whole number of {block}x{block} blocks", image.width, image.height));
    }
    let mut tiles = Vec::with_capacity(image.width * image.height / 2);
    for block_y in (0..image.height).step_by(block) {
        for block_x in (0..image.width).step_by(block) {
            for tile_y in (block_y..block_y + block).step_by(8) {
                for tile_x in (block_x..block_x + block).step_by(8) {
                    for y in tile_y..tile_y + 8 {
                        for x in (tile_x..tile_x + 8).step_by(2) {
                            let at = y * image.width + x;
                            tiles.push(image.pixels[at] << 4 | image.pixels[at + 1] & 15);
                        }
                    }
                }
            }
        }
    }
    Ok(tiles)
}

/// The image's colours as a palette: 16 of `$0RGB`, little-endian.
pub fn palette(image: &Image) -> Vec<u8> {
    (0..16).flat_map(|i| image.colours.get(i).copied().unwrap_or(0).to_le_bytes()).collect()
}

/// The image as ca65 source exporting `_art_NAME`, its tiles (in `block`
/// squares, as `tiles_in_blocks`), and `_art_NAME_palette`, in `segment`; C
/// declares them `extern const unsigned char art_NAME[],
/// art_NAME_palette[];`. `name` is cut to letters, digits and `_`.
pub fn asm(name: &str, image: &Image, segment: &str, block: usize) -> Result<String, String> {
    let name: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let tiles = tiles_in_blocks(image, block)?;
    let mut source = format!(
        "; {name}: {}x{}, {} tiles. Made by gfx; edit the PNG instead.\n\n\
         \x20       .export _art_{name}, _art_{name}_palette\n\n\
         \x20       .segment \"{segment}\"\n",
        image.width,
        image.height,
        tiles.len() / 32
    );
    for (label, data) in [(format!("_art_{name}"), tiles), (format!("_art_{name}_palette"), palette(image))] {
        source += &format!("\n{label}:\n");
        for line in data.chunks(16) {
            let bytes: Vec<String> = line.iter().map(|b| format!("${b:02X}")).collect();
            source += &format!("        .byte   {}\n", bytes.join(", "));
        }
    }
    Ok(source)
}

/// The size of a cart file's icon field: 16 tiles, two palettes, and
/// which palette each tile takes.
pub const ICON_SIZE: usize = 16 * 32 + 2 * 32 + 16;

/// Any PNG as 12-bit colours, `None` where it is transparent.
pub fn read_colours(path: &Path) -> Result<(usize, usize, Vec<Option<u16>>), String> {
    let fail = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
    let file = File::open(path).map_err(|e| fail(&e))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| fail(&e))?;
    let (width, height) = (reader.info().width as usize, reader.info().height as usize);
    let mut buffer = vec![0; reader.output_buffer_size().ok_or_else(|| fail(&"too large"))?];
    let frame = reader.next_frame(&mut buffer).map_err(|e| fail(&e))?;
    let channels = frame.color_type.samples();
    let mut pixels = Vec::with_capacity(width * height);
    for row in buffer[..frame.buffer_size()].chunks(frame.line_size) {
        for pixel in row[..width * channels].chunks(channels) {
            let [r, g, b, a] = match channels {
                1 => [pixel[0], pixel[0], pixel[0], 255],
                2 => [pixel[0], pixel[0], pixel[0], pixel[1]],
                3 => [pixel[0], pixel[1], pixel[2], 255],
                _ => [pixel[0], pixel[1], pixel[2], pixel[3]],
            };
            pixels.push((a >= 128).then(|| rgb12(r, g, b)));
        }
    }
    Ok((width, height, pixels))
}

/// A cart icon from a 32x32 PNG in up to 30 colours: its 16 tiles, palette
/// 0, palette 1 (15 colours each, after the see-through colour 0), and for
/// each tile the palette it takes. Each tile's colours must fit one palette;
/// every way of sharing the tiles out is tried until both fit.
pub fn icon(path: &Path) -> Result<Vec<u8>, String> {
    let (width, height, pixels) = read_colours(path)?;
    icon_field(width, height, &pixels).map_err(|error| format!("{}: {error}", path.display()))
}

fn icon_field(width: usize, height: usize, pixels: &[Option<u16>]) -> Result<Vec<u8>, String> {
    const SIDE: usize = 32;
    const TILES: usize = 16;
    if (width, height) != (SIDE, SIDE) {
        return Err(format!("{width}x{height}: an icon is {SIDE}x{SIDE}"));
    }
    let tile_of = |at: usize| at / SIDE / 8 * 4 + at % SIDE / 8;

    let mut colours: Vec<u16> = Vec::new();
    let mut sets = [0u32; TILES];
    for (at, pixel) in pixels.iter().enumerate() {
        let Some(colour) = *pixel else { continue };
        let index = colours.iter().position(|&c| c == colour).unwrap_or_else(|| {
            colours.push(colour);
            colours.len() - 1
        });
        if index < 32 {
            sets[tile_of(at)] |= 1 << index;
        }
    }
    if colours.len() > 30 {
        return Err(format!("{} colours: at most 30, two palettes of 15", colours.len()));
    }
    for (tile, set) in sets.iter().enumerate() {
        if set.count_ones() > 15 {
            let (x, y) = (tile % 4 * 8, tile / 4 * 8);
            return Err(format!("the tile at {x},{y} has {} colours: at most 15", set.count_ones()));
        }
    }
    // Tile 0 takes palette 0; bit t-1 of `choice` puts tile t in palette 1.
    let split = |choice: u32| {
        let mut palettes = [0u32; 2];
        for (tile, set) in sets.iter().enumerate() {
            palettes[(tile > 0 && choice >> (tile - 1) & 1 != 0) as usize] |= set;
        }
        palettes
    };
    let choice = (0..1u32 << (TILES - 1))
        .find(|&choice| split(choice).iter().all(|palette| palette.count_ones() <= 15))
        .ok_or("no way to share the tiles between two palettes of 15 colours")?;
    let palettes = split(choice);
    let members = |palette: u32| (0..32).filter(move |i| palette >> i & 1 != 0);
    let select: Vec<u8> = (0..TILES).map(|tile| (tile > 0 && choice >> (tile - 1) & 1 != 0) as u8).collect();

    let indexed = pixels
        .iter()
        .enumerate()
        .map(|(at, pixel)| {
            let Some(colour) = *pixel else { return 0 };
            let index = colours.iter().position(|&c| c == colour).unwrap();
            1 + members(palettes[select[tile_of(at)] as usize]).position(|i| i == index).unwrap() as u8
        })
        .collect();
    let mut field = tiles(&Image { width, height, pixels: indexed, colours: vec![] })?;
    for palette in palettes {
        let mut list = vec![0];
        list.extend(members(palette).map(|i| colours[i]));
        field.extend(self::palette(&Image { width: 0, height: 0, pixels: vec![], colours: list }));
    }
    field.extend(select);
    debug_assert_eq!(field.len(), ICON_SIZE);
    Ok(field)
}

/// A cart icon as ca65 source, in the cart file's ICON segment.
pub fn icon_asm(path: &Path) -> Result<String, String> {
    let field = icon(path)?;
    let mut source = format!(
        "; The cart's icon, from {}. Made by gfx; edit the PNG instead.\n\n        .segment \"ICON\"\n\n",
        path.display()
    );
    for line in field.chunks(16) {
        let bytes: Vec<String> = line.iter().map(|b| format!("${b:02X}")).collect();
        source += &format!("        .byte   {}\n", bytes.join(", "));
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_pack_two_pixels_a_byte_in_tile_order() {
        // 16x8: two tiles; the left one all colour 1, the right one counting.
        let pixels = (0..8).flat_map(|_| [1u8; 8].into_iter().chain(0..8)).collect();
        let image = Image { width: 16, height: 8, pixels, colours: vec![] };
        let tiles = tiles(&image).unwrap();
        assert_eq!(tiles.len(), 64);
        assert_eq!(&tiles[..4], &[0x11; 4]);
        assert_eq!(&tiles[32..36], &[0x01, 0x23, 0x45, 0x67]);
    }

    #[test]
    fn an_icon_of_many_colours_shares_its_tiles_between_two_palettes() {
        // 32x32: each tile in one colour of 16 different ones, and a
        // transparent corner.
        let pixels: Vec<Option<u16>> = (0..32 * 32)
            .map(|at: usize| {
                let (x, y) = (at % 32, at / 32);
                (at != 0).then(|| 0x100 + (y / 8 * 4 + x / 8) as u16)
            })
            .collect();
        let field = icon_field(32, 32, &pixels).unwrap();
        assert_eq!(field.len(), ICON_SIZE);
        let select = &field[576..];
        // 16 colours can't share one palette: both are used.
        assert!(select.contains(&0) && select.contains(&1));
        assert_eq!(field[0] >> 4, 0, "the transparent corner");
        // Every tile's pixel indexes a colour its palette has.
        for (tile, &palette) in select.iter().enumerate() {
            let colours = &field[512 + palette as usize * 32..][..32];
            let pixel = (field[tile * 32 + 1] >> 4) as usize;
            assert!(pixel > 0 && u16::from_le_bytes([colours[pixel * 2], colours[pixel * 2 + 1]]) != 0);
        }
    }

    #[test]
    fn blocks_of_16_come_out_as_16x16_sprites_take_them() {
        // 32x16: two 16x16 blocks; each tile filled with its own number.
        let pixels = (0..16 * 32).map(|at| ((at / 32 / 8) * 4 + at % 32 / 8) as u8).collect();
        let image = Image { width: 32, height: 16, pixels, colours: vec![] };
        let order: Vec<u8> = tiles_in_blocks(&image, 16).unwrap().chunks(32).map(|tile| tile[0] >> 4).collect();
        // Row-major tiles 0 1 2 3 / 4 5 6 7: block one is 0 1 4 5, block two 2 3 6 7.
        assert_eq!(order, [0, 1, 4, 5, 2, 3, 6, 7]);
    }

    #[test]
    fn palettes_are_12_bit_colours() {
        assert_eq!(rgb12(0xF0, 0x80, 0x10), 0xF81);
        let image = Image { width: 0, height: 0, pixels: vec![], colours: vec![0, 0xF81] };
        let palette = palette(&image);
        assert_eq!(palette.len(), 32);
        assert_eq!(&palette[2..4], &[0x81, 0x0F]);
    }
}
