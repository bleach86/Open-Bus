//! The Apple II's picture, a line at a time, as its video circuit makes it:
//! 280 by 192, from RAM, by the soft switches when the line is drawn. Text
//! is 40 by 24 characters of 7 by 8 dots; low-res, 40 by 48 blocks of 16
//! colours; high-res, 280 by 192 dots, coloured as an NTSC set shows them.

pub const WIDTH: usize = 280;
pub const HEIGHT: usize = 192;

/// The soft switches the picture is drawn by.
#[derive(Clone, Copy, Debug, Default)]
pub struct Switches {
    pub graphics: bool,
    pub mixed: bool,
    pub page2: bool,
    pub hires: bool,
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
        if h5 == 0 && (h4 == 0 || h3 == 0) {
            address |= 0x1000; // blanked, on the II+
        }
    }
    address
}

/// Line `y` of the picture into `out`, 280 pixels: from `ram` by
/// `switches`, the flashing characters inverse while `flash_on`. In mixed
/// mode the last 32 lines are text.
pub fn line(ram: &[u8], switches: Switches, flash_on: bool, y: usize, out: &mut [u32]) {
    let text = !switches.graphics || (switches.mixed && y >= 160);
    if text {
        text_line(ram, switches.page2, flash_on, y, out);
    } else if switches.hires {
        hires_line(ram, switches.page2, y, out);
    } else {
        lores_line(ram, switches.page2, y, out);
    }
}

/// A line of text: 40 characters' dots, 7 a character, white on black;
/// inverse below $40, and flashing (below $80) inverse while the flash is
/// on.
fn text_line(ram: &[u8], page2: bool, flash_on: bool, y: usize, out: &mut [u32]) {
    let start = text_address(page2, y / 8);
    for (column, &byte) in ram[start..start + 40].iter().enumerate() {
        let dots = glyph_dots(byte & 0x3F, y % 8);
        let inverse = byte < 0x40 || (byte < 0x80 && flash_on);
        for dot in 0..7 {
            let on = (dots >> dot) & 1 != 0;
            out[column * 7 + dot] = if on != inverse { WHITE } else { BLACK };
        }
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
