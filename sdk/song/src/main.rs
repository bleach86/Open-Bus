//! `song NAME.song OUT.s [SEGMENT]`: a song as ca65 source for the SDK's
//! music driver (see the library). cart.mk runs it on a cart's music/.

use std::path::Path;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (input, output, segment) = match &args[..] {
        [input, output] => (input, output, "RODATA"),
        [input, output, segment] => (input, output, segment.as_str()),
        _ => {
            eprintln!("usage: song NAME.song OUT.s [SEGMENT]");
            exit(2);
        }
    };
    let name = Path::new(input).file_stem().unwrap_or_default().to_string_lossy();
    let result = std::fs::read_to_string(input)
        .map_err(|error| error.to_string())
        .and_then(|text| song::compile(&text))
        .map(|song| song::asm(&name, &song, segment))
        .and_then(|source| std::fs::write(output, source).map_err(|error| error.to_string()));
    if let Err(error) = result {
        eprintln!("song: {input}: {error}");
        exit(1);
    }
}
