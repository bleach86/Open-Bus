//! A notice over a machine's picture, of any size: a disk put in, a state
//! saved. A black band across its foot, 12 lines high, the text centred in
//! it in white, in capitals, each dot as many wide as the picture has room
//! for 40 characters at; as much of the text as fits. And a panel at its
//! top left, some lines of text on the picture dimmed.

use crate::host::font;

/// How long a notice is shown: 3 seconds.
pub const FRAMES: u32 = 180;

const BLACK: u32 = 0x000000;
const WHITE: u32 = 0xFFFFFF;
const BAND: usize = 12;
/// A character's dots across, and the most at a dot a pixel.
const CHARACTER: usize = 7;
const COLUMNS: usize = 40;
/// A panel's lines; and how its picture's dimmed, each colour shifted down
/// and the bits shifted in from the next left out.
const LINE: usize = 8;
const DIM: u32 = 2;
const DIMMED: u32 = 0x3F3F3F;

/// `text` over `picture`, `width` by `height`.
pub fn draw(picture: &mut [u32], width: usize, height: usize, text: &str) {
    let (scale, columns) = fit(width);
    let text = characters(text, columns);
    let top = height - BAND;
    picture[top * width..].fill(BLACK);
    let left = (columns - text.len()) * CHARACTER * scale / 2;
    write(picture, width, left, top + 2, scale, &text);
}

/// `lines` at the top left of `picture`, `width` by `height`, what's under
/// them dimmed; as many of them as fit, and as much of each.
pub fn panel(picture: &mut [u32], width: usize, height: usize, lines: &[String]) {
    let (scale, columns) = fit(width);
    let lines: Vec<Vec<u8>> = lines.iter().take((height - 2) / LINE).map(|line| characters(line, columns.saturating_sub(1))).collect();
    let longest = lines.iter().map(Vec::len).max().unwrap_or(0);
    let across = ((longest * CHARACTER + 2) * scale).min(width);
    for row in picture.chunks_mut(width).take(lines.len() * LINE + 2) {
        for dot in &mut row[..across] {
            *dot = (*dot >> DIM) & DIMMED;
        }
    }
    for (place, line) in lines.iter().enumerate() {
        write(picture, width, scale, 1 + place * LINE, scale, line);
    }
}

/// How many pixels wide a dot is, and how many characters fit, in `width`.
fn fit(width: usize) -> (usize, usize) {
    let scale = (width / (COLUMNS * CHARACTER)).max(1);
    (scale, width / (CHARACTER * scale))
}

/// `text` as the font has it, as much as `columns` holds.
fn characters(text: &str, columns: usize) -> Vec<u8> {
    text.to_uppercase()
        .bytes()
        .map(|b| if (0x20..0x60).contains(&b) { b } else { b'?' })
        .take(columns)
        .collect()
}

/// `text` written at `left`, `top`, its dots `scale` wide.
fn write(picture: &mut [u32], width: usize, left: usize, top: usize, scale: usize, text: &[u8]) {
    for (place, &char) in text.iter().enumerate() {
        for row in 0..8 {
            let dots = font::dots(char & 0x3F, row);
            let line = (top + row) * width;
            for dot in (0..CHARACTER).filter(|dot| (dots >> dot) & 1 != 0) {
                let x = left + (place * CHARACTER + dot) * scale;
                picture[line + x..line + x + scale].fill(WHITE);
            }
        }
    }
}
