//! A notice over a machine's picture, of any size: a disk put in, a state
//! saved. A black band across its foot, 12 lines high, the text centred in
//! it in white, in capitals, each dot as many wide as the picture has room
//! for 40 characters at; as much of the text as fits.

use crate::host::font;

/// How long a notice is shown: 3 seconds.
pub const FRAMES: u32 = 180;

const BLACK: u32 = 0x000000;
const WHITE: u32 = 0xFFFFFF;
const BAND: usize = 12;
/// A character's dots across, and the most at a dot a pixel.
const CHARACTER: usize = 7;
const COLUMNS: usize = 40;

/// `text` over `picture`, `width` by `height`.
pub fn draw(picture: &mut [u32], width: usize, height: usize, text: &str) {
    let scale = (width / (COLUMNS * CHARACTER)).max(1);
    let columns = width / (CHARACTER * scale);
    let text: Vec<u8> = text
        .to_uppercase()
        .bytes()
        .map(|b| if (0x20..0x60).contains(&b) { b } else { b'?' })
        .take(columns)
        .collect();
    let top = height - BAND;
    picture[top * width..].fill(BLACK);
    let left = (columns - text.len()) * CHARACTER * scale / 2;
    for (place, &char) in text.iter().enumerate() {
        for row in 0..8 {
            let dots = font::dots(char & 0x3F, row);
            let line = (top + 2 + row) * width;
            for dot in (0..CHARACTER).filter(|dot| (dots >> dot) & 1 != 0) {
                let x = left + (place * CHARACTER + dot) * scale;
                picture[line + x..line + x + scale].fill(WHITE);
            }
        }
    }
}
