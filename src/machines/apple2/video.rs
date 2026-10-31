//! The Apple II's picture, a line at a time, as its video circuit makes it:
//! 280 by 192, from RAM, by the soft switches when the line is drawn. Text
//! is 40 by 24 characters of 7 by 8 dots; low-res, 40 by 48 blocks of 16
//! colours; high-res, 280 by 192 dots, coloured as an NTSC set shows them.
//!
//! The IIe's has 560 dots a line, for its 80 columns, double low-res and
//! double high-res, each a byte of its auxiliary 64K's then a byte of its
//! main's, in turn; its 40 column modes' dots are each two of them. Its
//! characters are its video ROM's, with ALTCHARSET its alternate set:
//! lower case and MouseText, not flashing.

pub const WIDTH: usize = 280;
pub const HEIGHT: usize = 192;
/// The picture as it's shown: the IIe's 560 dots a line, a 40 column
/// mode's each two.
pub const PICTURE_WIDTH: usize = 560;

/// The soft switches the picture is drawn by: the II+'s, and the IIe's.
#[derive(Clone, Copy, Debug, Default)]
pub struct Switches {
    pub graphics: bool,
    pub mixed: bool,
    pub page2: bool,
    pub hires: bool,
    pub iie: bool,
    pub col80: bool,
    pub altcharset: bool,
    pub dhires: bool,
}

/// What a line is drawn from: main RAM; and a IIe's auxiliary RAM and its
/// video ROM's characters (the II+'s, none: its own are built in).
pub struct Source<'a> {
    pub main: &'a [u8],
    pub aux: &'a [u8],
    pub characters: Option<&'a [u8]>,
}

const BLACK: u32 = 0x000000;
const WHITE: u32 = 0xFFFFFF;

/// Low-res's colours, a nibble's.
pub const LORES: [u32; 16] = [
    0x000000, // black
    0xDD0033, // magenta
    0x000099, // dark blue
    0xDD22DD, // purple
    0x007722, // dark green
    0x555555, // grey
    0x2222FF, // medium blue
    0x66AAFF, // light blue
    0x885500, // brown
    0xFF6600, // orange
    0xAAAAAA, // grey
    0xFF9988, // pink
    0x11DD00, // light green
    0xFFFF00, // yellow
    0x44FF99, // aqua
    0xFFFFFF, // white
];

/// High-res's colours: a dot on its own, by its column and its byte's top bit.
const VIOLET: u32 = LORES[3];
const GREEN: u32 = LORES[12];
const BLUE: u32 = LORES[6];
const ORANGE: u32 = LORES[9];

/// The address of text or low-res row `row` (0 to 23) on its page: the rows
/// are interleaved, three of every eight to a 128-byte block.
pub fn text_address(page2: bool, row: usize) -> usize {
    let base = if page2 { 0x0800 } else { 0x0400 };
    base + (row % 8) * 0x80 + (row / 8) * 0x28
}

/// The address of high-res line `y` (0 to 191) on its page.
pub fn hires_address(page2: bool, y: usize) -> usize {
    let base = if page2 { 0x4000 } else { 0x2000 };
    base + (y % 8) * 0x400 + ((y / 8) % 8) * 0x80 + (y / 64) * 0x28
}

/// The address the video circuit reads from RAM in cycle `cycle` (0 to
/// 64) of line `line` (0 to 261), by the switches: what its counters make
/// it, as they count, seen or not (Sather's Understanding the Apple II).
/// The horizontal one is $00 then $40 to $7F, a step a cycle, the line
/// blanked to $57 and seen from $58; the vertical one $100 to $1FF, the
/// picture to $1BF, then $FA to $FF. Text and low-res read their page,
/// and on the II+, while the line's blanked, $1000 up from it; high-res
/// its page, but text's below line 160, mixed.
pub fn scanner_address(switches: Switches, line: usize, cycle: usize) -> usize {
    let h = if cycle == 0 { 0x00 } else { 0x3F + cycle };
    let v = if line < 256 { 0x100 + line } else { line - 6 };
    let bit = |n: usize, at: usize| (n >> at) & 1;
    let (h3, h4, h5) = (bit(h, 3), bit(h, 4), bit(h, 5));
    let (v3, v4) = (bit(v, 6), bit(v, 7));
    let hires = switches.graphics && switches.hires && !(switches.mixed && v4 == 1 && bit(v, 5) == 1);

    // Bits 3 to 6: a sum of the counters' bits, interleaving the rows.
    let sum = (0b1101 + (h5 << 2 | h4 << 1 | h3) + (v4 << 3 | v3 << 2 | v4 << 1 | v3)) & 0x0F;
    let mut address = (h & 7) | sum << 3 | bit(v, 3) << 7 | bit(v, 4) << 8 | bit(v, 5) << 9;
    let page = if switches.page2 { 2 } else { 1 };
    if hires {
        address |= (v & 7) << 10 | page << 13;
    } else {
        address |= page << 10;
        if !switches.iie && h5 == 0 && (h4 == 0 || h3 == 0) {
            address |= 0x1000; // blanked, on the II+
        }
    }
    address
}

/// Line `y` of the picture as it's shown, 560 dots, into `out`: from
/// `source` by `switches`, the flashing characters inverse while
/// `flash_on`. With 80COL, text is 80 columns, and with double high-res
/// on, graphics double; the 40 column modes as on the II+, each dot twice.
pub fn picture_line(source: &Source, switches: Switches, flash_on: bool, y: usize, out: &mut [u32]) {
    let text = !switches.graphics || (switches.mixed && y >= 160);
    let double = switches.col80 && (text || switches.dhires);
    if double && text {
        text80_line(source, switches, flash_on, y, out);
    } else if double && switches.hires {
        dhires_line(source, switches.page2, y, out);
    } else if double {
        dlores_line(source, switches.page2, y, out);
    } else {
        let mut dots = [0; WIDTH];
        line(source, switches, flash_on, y, &mut dots);
        for (x, &colour) in dots.iter().enumerate() {
            out[2 * x] = colour;
            out[2 * x + 1] = colour;
        }
    }
}

/// A character's row `row` of dots, the lowest bit leftmost, 1 a dot shown:
/// the II+'s own, inverse below $40, and flashing (below $80) inverse
/// while the flash is on; or from a IIe's video ROM, the alternate set by
/// the byte, the primary's $40 to $7F flashing, its inverse character or
/// its normal by turns.
fn character_row(characters: Option<&[u8]>, altcharset: bool, flash_on: bool, byte: u8, row: usize) -> u8 {
    match characters {
        None => {
            let dots = glyph_dots(byte & 0x3F, row);
            let inverse = byte < 0x40 || (byte < 0x80 && flash_on);
            if inverse { !dots & 0x7F } else { dots }
        }
        Some(rom) => {
            let flashing = !altcharset && (0x40..0x80).contains(&byte);
            let code = if flashing { byte & 0x3F | if flash_on { 0x00 } else { 0x80 } } else { byte };
            !rom[code as usize * 8 + row] & 0x7F
        }
    }
}

/// 40 columns of text, as the II+'s, its characters the IIe's if it is one.
fn text40_line(source: &Source, switches: Switches, flash_on: bool, y: usize, out: &mut [u32]) {
    let start = text_address(switches.page2, y / 8);
    for (column, &byte) in source.main[start..start + 40].iter().enumerate() {
        let dots = character_row(source.characters, switches.altcharset, flash_on, byte, y % 8);
        for dot in 0..7 {
            out[column * 7 + dot] = if (dots >> dot) & 1 != 0 { WHITE } else { BLACK };
        }
    }
}

/// 80 columns: of each of the 40 places, its auxiliary byte's character,
/// then its main's.
fn text80_line(source: &Source, switches: Switches, flash_on: bool, y: usize, out: &mut [u32]) {
    let start = text_address(switches.page2, y / 8);
    for place in 0..40 {
        for (half, memory) in [source.aux, source.main].into_iter().enumerate() {
            let dots = character_row(source.characters, switches.altcharset, flash_on, memory[start + place], y % 8);
            let x = (2 * place + half) * 7;
            for dot in 0..7 {
                out[x + dot] = if (dots >> dot) & 1 != 0 { WHITE } else { BLACK };
            }
        }
    }
}

/// Double low-res: 80 blocks, 7 dots wide, an auxiliary byte's then a
/// main's; the auxiliary's colour its nibble rotated left a bit, falling a
/// dot earlier in the colour signal's 4.
fn dlores_line(source: &Source, page2: bool, y: usize, out: &mut [u32]) {
    let start = text_address(page2, y / 8);
    for place in 0..40 {
        for (half, memory) in [source.aux, source.main].into_iter().enumerate() {
            let byte = memory[start + place];
            let mut nibble = if y % 8 < 4 { byte & 0x0F } else { byte >> 4 };
            if half == 0 {
                nibble = ((nibble << 1) | (nibble >> 3)) & 0x0F;
            }
            let x = (2 * place + half) * 7;
            out[x..x + 7].fill(LORES[nibble as usize]);
        }
    }
}

/// Double high-res: 560 dots, 7 of each byte, an auxiliary byte's then a
/// main's, the lowest bit first. Four dots are a cycle of the colour
/// signal, and a composite monitor sees the colour of the 4 around each
/// dot, the one before it, itself and the two after: the colour of
/// low-res's their bits make, each its place in the cycle's bit (dot 4n
/// bit 0). Solid colour as low-res's, edges fringed as on the screen.
fn dhires_line(source: &Source, page2: bool, y: usize, out: &mut [u32]) {
    let start = hires_address(page2, y);
    let mut dots = [0u8; PICTURE_WIDTH];
    for place in 0..40 {
        for (half, memory) in [source.aux, source.main].into_iter().enumerate() {
            let byte = memory[start + place];
            for bit in 0..7 {
                dots[(2 * place + half) * 7 + bit] = (byte >> bit) & 1;
            }
        }
    }
    for (x, colour_out) in out.iter_mut().enumerate().take(PICTURE_WIDTH) {
        let around = (x.saturating_sub(1)..(x + 3).min(PICTURE_WIDTH)).filter(|&at| dots[at] != 0);
        let colour = around.fold(0, |colour, at| colour | 1 << (at % 4));
        *colour_out = LORES[colour];
    }
}

/// Line `y` of the picture into `out`, 280 pixels: from `source` by
/// `switches`, the flashing characters inverse while `flash_on`. In mixed
/// mode the last 32 lines are text.
pub fn line(source: &Source, switches: Switches, flash_on: bool, y: usize, out: &mut [u32]) {
    let text = !switches.graphics || (switches.mixed && y >= 160);
    if text {
        text40_line(source, switches, flash_on, y, out);
    } else if switches.hires {
        hires_line(source.main, switches.page2, y, out);
    } else {
        lores_line(source.main, switches.page2, y, out);
    }
}

/// A line of low-res: 40 blocks, 7 dots wide; the top four lines of a row
/// are its byte's low nibble's colour, the bottom four its high's.
fn lores_line(ram: &[u8], page2: bool, y: usize, out: &mut [u32]) {
    let start = text_address(page2, y / 8);
    for (column, &byte) in ram[start..start + 40].iter().enumerate() {
        let nibble = if y % 8 < 4 { byte & 0x0F } else { byte >> 4 };
        out[column * 7..column * 7 + 7].fill(LORES[nibble as usize]);
    }
}

/// A line of high-res: 40 bytes of 7 dots, the lowest bit leftmost. A dot
/// next to another is white; a dot on its own is coloured, violet or green
/// by its column, or blue or orange if its byte's top bit is set; and a gap
/// between two such dots is filled with their colour, as on the TV.
fn hires_line(ram: &[u8], page2: bool, y: usize, out: &mut [u32]) {
    let start = hires_address(page2, y);
    let bytes = &ram[start..start + 40];
    let dot = |x: usize| bytes[x / 7] >> (x % 7) & 1 != 0;
    let colour = |x: usize| {
        let shifted = bytes[x / 7] & 0x80 != 0;
        match (shifted, x % 2 == 1) {
            (false, false) => VIOLET,
            (false, true) => GREEN,
            (true, false) => BLUE,
            (true, true) => ORANGE,
        }
    };
    for x in 0..WIDTH {
        let left = x > 0 && dot(x - 1);
        let right = x + 1 < WIDTH && dot(x + 1);
        out[x] = match (dot(x), left || right) {
            (true, true) => WHITE,
            (true, false) => colour(x),
            (false, _) if left && right => colour(x - 1),
            (false, _) => BLACK,
        };
    }
}

/// A character's dots on line `row` (0 to 7) of its 8: bit 0 leftmost, 7
/// dots, the glyph's 5 in the middle; the 8th line is blank.
fn glyph_dots(char: u8, row: usize) -> u8 {
    if row == 7 {
        return 0;
    }
    FONT[char as usize][row]
        .bytes()
        .enumerate()
        .filter(|&(_, b)| b == b'#')
        .fold(0, |dots, (i, _)| dots | 1 << (i + 1))
}

/// The II+'s 64 characters, $00 to $3F: @, A to Z, [ \ ] ^ _, then space,
/// the punctuation and the digits to ?. A 5 by 7 font of our own, after
/// the II+'s.
const FONT: [[&str; 7]; 64] = [
    [".###.", "#...#", "#.#.#", "#.###", "#.##.", "#....", ".####"], // @
    ["..#..", ".#.#.", "#...#", "#...#", "#####", "#...#", "#...#"], // A
    ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."], // B
    [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."], // C
    ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."], // D
    ["#####", "#....", "#....", "####.", "#....", "#....", "#####"], // E
    ["#####", "#....", "#....", "####.", "#....", "#....", "#...."], // F
    [".####", "#....", "#....", "#..##", "#...#", "#...#", ".####"], // G
    ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"], // H
    [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."], // I
    ["....#", "....#", "....#", "....#", "....#", "#...#", ".###."], // J
    ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"], // K
    ["#....", "#....", "#....", "#....", "#....", "#....", "#####"], // L
    ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"], // M
    ["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#"], // N
    [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."], // O
    ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."], // P
    [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"], // Q
    ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"], // R
    [".###.", "#...#", "#....", ".###.", "....#", "#...#", ".###."], // S
    ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."], // T
    ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."], // U
    ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."], // V
    ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#"], // W
    ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"], // X
    ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."], // Y
    ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"], // Z
    ["#####", "##...", "##...", "##...", "##...", "##...", "#####"], // [
    [".....", "#....", ".#...", "..#..", "...#.", "....#", "....."], // \
    ["#####", "...##", "...##", "...##", "...##", "...##", "#####"], // ]
    [".....", ".....", "..#..", ".#.#.", "#...#", ".....", "....."], // ^
    [".....", ".....", ".....", ".....", ".....", ".....", "#####"], // _
    [".....", ".....", ".....", ".....", ".....", ".....", "....."], // space
    ["..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#.."], // !
    [".#.#.", ".#.#.", ".#.#.", ".....", ".....", ".....", "....."], // "
    [".#.#.", ".#.#.", "#####", ".#.#.", "#####", ".#.#.", ".#.#."], // #
    ["..#..", ".####", "#.#..", ".###.", "..#.#", "####.", "..#.."], // $
    ["##...", "##..#", "...#.", "..#..", ".#...", "#..##", "...##"], // %
    [".#...", "#.#..", "#.#..", ".#...", "#.#.#", "#..#.", ".##.#"], // &
    ["..#..", "..#..", "..#..", ".....", ".....", ".....", "....."], // '
    ["..#..", ".#...", "#....", "#....", "#....", ".#...", "..#.."], // (
    ["..#..", "...#.", "....#", "....#", "....#", "...#.", "..#.."], // )
    ["..#..", "#.#.#", ".###.", "..#..", ".###.", "#.#.#", "..#.."], // *
    [".....", "..#..", "..#..", "#####", "..#..", "..#..", "....."], // +
    [".....", ".....", ".....", ".....", "..#..", "..#..", ".#..."], // ,
    [".....", ".....", ".....", "#####", ".....", ".....", "....."], // -
    [".....", ".....", ".....", ".....", ".....", ".....", "..#.."], // .
    [".....", "....#", "...#.", "..#..", ".#...", "#....", "....."], // /
    [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."], // 0
    ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."], // 1
    [".###.", "#...#", "....#", "..##.", ".#...", "#....", "#####"], // 2
    ["#####", "....#", "...#.", "..##.", "....#", "#...#", ".###."], // 3
    ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."], // 4
    ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."], // 5
    ["..###", ".#...", "#....", "####.", "#...#", "#...#", ".###."], // 6
    ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."], // 7
    [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."], // 8
    [".###.", "#...#", "#...#", ".####", "....#", "...#.", "###.."], // 9
    [".....", ".....", "..#..", ".....", "..#..", ".....", "....."], // :
    [".....", ".....", "..#..", ".....", "..#..", "..#..", ".#..."], // ;
    ["...#.", "..#..", ".#...", "#....", ".#...", "..#..", "...#."], // <
    [".....", ".....", "#####", ".....", "#####", ".....", "....."], // =
    [".#...", "..#..", "...#.", "....#", "...#.", "..#..", ".#..."], // >
    [".###.", "#...#", "...#.", "..#..", "..#..", ".....", "..#.."], // ?
];
