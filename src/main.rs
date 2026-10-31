mod cpu;

/// The chips the machines are made of, besides the CPU.
mod chips {
    pub mod ay38910;
    pub mod via6522;

    #[cfg(test)]
    mod tests {
        mod ay38910;
        mod via6522;
    }
}

/// The host's side: its window, sound and controls, and its terminal.
mod host {
    pub mod display;
    pub mod terminal;
    pub mod window;
}

/// The machines the CPU runs in.
mod machines {
    pub mod apple2;
    pub mod basic;
}

/// The machine to run: `cpu_emu` for this one, with BASIC; `cpu_emu apple2
/// ROM [DISK...]` for an Apple II+, ROM a dump of its ROM (its last 12K,
/// $D000 up), the DISKs .dsk, .do, .po, .nib or .woz images for a Disk II
/// (its ROMs, disk2-p5-341-0027.bin and disk2-p6-341-0028.bin, beside ROM),
/// the first in drive 1, F9 and F10 changing drive 1's and drive 2's; a
/// Mockingboard in slot 4, or with `--mockingboards=N` none, or two (4 and
/// 5); in a window, or with `--text` its text screen in the terminal.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "cpu_emu apple2 ROM [DISK...] [--mockingboards=0|1|2] [--text]";
    match args.get(1).map(String::as_str) {
        Some("apple2") => {
            let files: Vec<&String> = args[2..].iter().filter(|arg| !arg.starts_with("--")).collect();
            let rom = files.first().expect(usage);
            let text = args.iter().any(|arg| arg == "--text");
            let mockingboards = match args.iter().find_map(|arg| arg.strip_prefix("--mockingboards=")) {
                Some(count) => count.parse().ok().filter(|&count| count <= 2).expect(usage),
                None => 1,
            };
            let disks: Vec<&str> = files[1..].iter().map(|disk| disk.as_str()).collect();
            machines::apple2::run(rom, &disks, mockingboards, text);
        }
        _ => machines::basic::run(),
    }
}
