//! `gfx tiles IMAGE.png OUT`, `gfx palette IMAGE.png OUT`, `gfx asm
//! IMAGE.png OUT.s [SEGMENT]`, `gfx asm16 IMAGE.png OUT.s [SEGMENT]` (tiles
//! in 16x16 blocks, for 16x16 sprites), `gfx map IMAGE.png OUT.s [SEGMENT]`
//! (distinct tiles and a tile map), `gfx icon ICON.png OUT.s`: see the
//! library. cart.mk runs `gfx asm` on a cart's art/ PNGs, `gfx asm16` on
//! art/16/, `gfx map` on art/map/, and `gfx icon` on its icon.

use std::path::Path;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mode, input, output, segment) = match &args[..] {
        [mode, input, output] => (mode, input, output, "RODATA"),
        [mode, input, output, segment] if mode.starts_with("asm") || mode == "map" => (mode, input, output, segment.as_str()),
        _ => {
            eprintln!("usage: gfx tiles|palette IMAGE.png OUT, gfx asm|asm16|map IMAGE.png OUT.s [SEGMENT], gfx icon ICON.png OUT.s");
            exit(2);
        }
    };
    let input = Path::new(input);
    let data = match mode.as_str() {
        "tiles" => gfx::read(input).and_then(|image| gfx::tiles(&image)),
        "palette" => gfx::read(input).map(|image| gfx::palette(&image)),
        "asm" | "asm16" => gfx::read(input).and_then(|image| {
            let name = input.file_stem().unwrap_or_default().to_string_lossy();
            let block = if mode == "asm16" { 16 } else { 8 };
            gfx::asm(&name, &image, segment, block).map(String::into_bytes)
        }),
        "map" => gfx::read(input).and_then(|image| {
            let name = input.file_stem().unwrap_or_default().to_string_lossy();
            gfx::map_asm(&name, &image, segment).map(String::into_bytes)
        }),
        "icon" => gfx::icon_asm(input).map(String::into_bytes),
        _ => Err(format!("{mode}: not tiles, palette, asm, asm16, map or icon")),
    };
    match data.and_then(|data| std::fs::write(output, data).map_err(|error| format!("{output}: {error}"))) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("gfx: {error}");
            exit(1);
        }
    }
}
