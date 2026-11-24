//! A notice over a machine's picture, of any size: a disk put in, a state
//! saved. A black band across its foot, 12 dots high, the text centred in
//! it in white, in capitals, each dot as many wide as the picture has room
//! for 40 characters at, and as many lines tall as make it square on the
//! picture shown at 4:3; as much of the text as fits. And a panel at its
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
/// A drive's light: a lamp, 5 dots by 5 lines, lit, its edge, and where it
/// shines; and the dot dimmed around it.
const LAMP: [&str; 5] = [".ooo.", "o+##o", "o###o", "o###o", ".ooo."];
const LAMP_MARGIN: usize = 1;
const LAMP_LIT: u32 = 0xFF3020;
const LAMP_EDGE: u32 = 0xAA1010;
const LAMP_SHINE: u32 = 0xFFAA96;

/// `text` over `picture`, `width` by `height`.
pub fn draw(picture: &mut [u32], width: usize, height: usize, text: &str) {
    let fit = fit(width, height);
    let text = characters(text, fit.columns);
    let top = height.saturating_sub(BAND * fit.tall);
    picture[top * width..].fill(BLACK);
    let left = (fit.columns - text.len()) * CHARACTER * fit.scale / 2;
    write(picture, width, (left, top + 2 * fit.tall), fit, &text);
}

/// `lines` at the top left of `picture`, `width` by `height`, what's under
/// them dimmed; as many of them as fit, and as much of each.
pub fn panel(picture: &mut [u32], width: usize, height: usize, lines: &[String]) {
    let panel = Panel::of(width, height, lines);
    for row in picture.chunks_mut(width).take(panel.down) {
        for dot in &mut row[..panel.across] {
            *dot = (*dot >> DIM) & DIMMED;
        }
    }
    for (place, line) in panel.lines.iter().enumerate() {
        let at = (panel.fit.scale, panel.fit.tall + place * panel.line_height);
        write(picture, width, at, panel.fit, line);
    }
}

/// Which of `lines`, as `panel` puts them on a picture `width` by
/// `height`, is at `at` on it, in its pixels: its place, or None off the
/// panel. Its margins are its first line's and its last's.
pub fn panel_line(width: usize, height: usize, lines: &[String], at: (f32, f32)) -> Option<usize> {
    let panel = Panel::of(width, height, lines);
    let (x, y) = at;
    if x < 0.0 || y < 0.0 || x >= panel.across as f32 || y >= panel.down as f32 {
        return None;
    }
    let line = (y as usize).saturating_sub(panel.fit.tall) / panel.line_height;
    Some(line.min(panel.lines.len().saturating_sub(1)))
}

/// A panel's lines as they're put: as many as fit, and as much of each;
/// how tall each is; and how far across and down it goes.
struct Panel {
    fit: Fit,
    lines: Vec<Vec<u8>>,
    line_height: usize,
    across: usize,
    down: usize,
}

impl Panel {
    fn of(width: usize, height: usize, lines: &[String]) -> Panel {
        let fit = fit(width, height);
        let line_height = LINE * fit.tall;
        let lines: Vec<Vec<u8>> = lines
            .iter()
            .take(height.saturating_sub(2 * fit.tall) / line_height)
            .map(|line| characters(line, fit.columns.saturating_sub(1)))
            .collect();
        let longest = lines.iter().map(Vec::len).max().unwrap_or(0);
        Panel {
            across: ((longest * CHARACTER + 2) * fit.scale).min(width),
            down: lines.len() * line_height + 2 * fit.tall,
            fit,
            lines,
            line_height,
        }
    }
}

/// A drive's light, lit, at the bottom right of `picture`, `width` by
/// `height`: a small red lamp, its dots as wide as a notice's, on the
/// picture dimmed.
pub fn light(picture: &mut [u32], width: usize, height: usize) {
    let Fit { scale, tall, .. } = fit(width, height);
    let across = LAMP.len() + 2 * LAMP_MARGIN;
    let left = width.saturating_sub(across * scale);
    let top = height.saturating_sub(across * tall);
    for row in picture.chunks_mut(width).skip(top) {
        for dot in &mut row[left..] {
            *dot = (*dot >> DIM) & DIMMED;
        }
    }
    let lamp_left = left + LAMP_MARGIN * scale;
    for (row, dots) in LAMP.iter().enumerate() {
        for (dot, kind) in dots.bytes().enumerate() {
            let colour = match kind {
                b'#' => LAMP_LIT,
                b'o' => LAMP_EDGE,
                b'+' => LAMP_SHINE,
                _ => continue,
            };
            let x = lamp_left + dot * scale;
            for line in 0..tall {
                let line = (top + (LAMP_MARGIN + row) * tall + line) * width;
                picture[line + x..line + x + scale].fill(colour);
            }
        }
    }
}

/// A dot's size on a picture: how many pixels wide, for 40 characters
/// across; how many lines tall, for it to be square as the picture's shown,
/// at 4:3; and how many characters fit across.
#[derive(Clone, Copy)]
struct Fit {
    scale: usize,
    tall: usize,
    columns: usize,
}

fn fit(width: usize, height: usize) -> Fit {
    let scale = (width / (COLUMNS * CHARACTER)).max(1);
    let tall = ((scale * height * 4 + width * 3 / 2) / (width * 3)).max(1);
    Fit {
        scale,
        tall,
        columns: width / (CHARACTER * scale),
    }
}

/// `text` as the font has it, as much as `columns` holds.
fn characters(text: &str, columns: usize) -> Vec<u8> {
    text.to_uppercase()
        .bytes()
        .map(|b| if (0x20..0x60).contains(&b) { b } else { b'?' })
        .take(columns)
        .collect()
}

/// `text` written at `(left, top)`, its dots as big as `fit` has them.
fn write(picture: &mut [u32], width: usize, (left, top): (usize, usize), fit: Fit, text: &[u8]) {
    for (place, &char) in text.iter().enumerate() {
        for row in 0..8 {
            let dots = font::dots(char & 0x3F, row);
            for line in 0..fit.tall {
                let line = (top + row * fit.tall + line) * width;
                for dot in (0..CHARACTER).filter(|dot| (dots >> dot) & 1 != 0) {
                    let x = left + (place * CHARACTER + dot) * fit.scale;
                    picture[line + x..line + x + fit.scale].fill(WHITE);
                }
            }
        }
    }
}
