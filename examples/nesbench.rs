//! A NES game run headless, as fast as it goes, for a fixed number of
//! frames with the same input each time: how fast that was, against the
//! console's own speed, and a checksum of every frame's picture and sound.
//! Two builds with the same checksums made the same frames and the same
//! sound, every sample of it: a change that's only to be faster, exact.
//!
//! The input presses Start now and then, and after a while holds right and
//! taps A, to get past a title screen and into play.
//!
//! tools/nesbench runs this on games, against another commit's build.

use clap::Parser;
use cpu_emu::host::display::Gamepad;
use cpu_emu::host::machine::{Input, Machine};
use cpu_emu::host::zip::crc32;
use cpu_emu::machines::nes;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;

#[derive(Parser)]
#[command(about = "A NES game run headless: its speed, and its frames' checksums")]
struct Args {
    /// An iNES image, or a zip of one.
    rom: String,
    /// How many frames.
    #[arg(long, default_value_t = 1800)]
    frames: usize,
    /// Performance mode, as `--fast` has it.
    #[arg(long)]
    fast: bool,
    /// Each frame's picture and sound checksums written to FILE, a line a
    /// frame: to find the first frame two builds differ at.
    #[arg(long, value_name = "FILE")]
    log: Option<String>,
}

/// The pad on frame `frame`.
fn pad(frame: usize) -> Gamepad {
    let f = frame % 600;
    Gamepad {
        start: (100..106).contains(&f) || (300..306).contains(&f),
        a: frame > 400 && frame % 40 < 10,
        right: frame > 400 && frame % 200 < 150,
        ..Default::default()
    }
}

fn main() {
    let args = Args::parse();
    let mut cpu = nes::build(&args.rom, 0, nes::Chosen::default()).unwrap_or_else(|error| {
        eprintln!("nesbench: {error}");
        std::process::exit(1);
    });
    if args.fast {
        cpu.memory.performance();
    }
    cpu.memory.sound_on();
    let mut log = args
        .log
        .map(|path| BufWriter::new(File::create(path).expect("the log can't be made")));

    let (mut picture_sum, mut sound_sum) = (0u32, 0u32);
    let mut took = 0.0;
    let mut cycles = 0u64;
    let mut bytes = Vec::new();
    for frame in 0..args.frames {
        cpu.memory.input(Input {
            gamepad: pad(frame),
            ..Default::default()
        });
        let (start, clock) = (Instant::now(), cpu.clock);
        cpu.run_frame();
        took += start.elapsed().as_secs_f64();
        cycles += cpu.clock.wrapping_sub(clock) as u64;

        bytes.clear();
        let sound = cpu.memory.sound();
        bytes.extend(sound.iter().flat_map(|sample| sample.to_le_bytes()));
        sound.clear();
        let sound = crc32(&bytes);

        bytes.clear();
        let picture = cpu.memory.picture();
        bytes.extend(picture.pixels.iter().flat_map(|pixel| pixel.to_le_bytes()));
        let picture = crc32(&bytes);

        picture_sum = picture_sum.rotate_left(1) ^ picture;
        sound_sum = sound_sum.rotate_left(1) ^ sound;
        if let Some(log) = &mut log {
            writeln!(log, "{frame} {picture:08x} {sound:08x}").expect("the log can't be written");
        }
    }
    // The console's own time for those frames, its clock's.
    let real = cycles as f64 / cpu.clock_hz as f64;
    println!(
        "{} frames in {took:.3}s: {:.0}% of real time; picture {picture_sum:08x} sound {sound_sum:08x}",
        args.frames,
        100.0 * real / took
    );
}
