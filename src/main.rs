mod cpu;

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
/// ROM [DISK]` for an Apple II+, ROM a dump of its ROM (its last 12K, $D000
/// up), DISK a .dsk or .nib in a Disk II (its ROMs, disk2-p5-341-0027.bin
/// and disk2-p6-341-0028.bin, beside ROM); in a window, or with `--text`
/// its text screen in the terminal.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "cpu_emu apple2 ROM [DISK] [--text]";
    match args.get(1).map(String::as_str) {
        Some("apple2") => {
            let files: Vec<&String> = args[2..].iter().filter(|arg| !arg.starts_with("--")).collect();
            let rom = files.first().expect(usage);
            let text = args.iter().any(|arg| arg == "--text");
            machines::apple2::run(rom, files.get(1).map(|disk| disk.as_str()), text);
        }
        _ => machines::basic::run(),
    }
}
