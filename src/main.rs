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
    pub mod clock;
    pub mod display;
    pub mod terminal;
    pub mod window;
    pub mod zip;

    #[cfg(test)]
    mod tests {
        mod clock;
        mod zip;
    }
}

/// The machines the CPU runs in.
mod machines {
    pub mod apple2;
    pub mod basic;
    pub mod nes;
    pub mod save_state;
}

/// The machine to run: `cpu_emu` for this one, with BASIC; `cpu_emu apple2
/// ROM [DISK...]` for an Apple II+, ROM a dump of its ROM (its last 12K,
/// $D000 up), the Disk II's ROMs (disk2-p5-341-0027.bin and
/// disk2-p6-341-0028.bin) beside it; `cpu_emu apple2e ROMS [DISK...]` for
/// an enhanced Apple IIe, or with `--unenhanced` the first, ROMS the folder
/// its ROMs are in, by their names, and the Disk II's. The DISKs are .dsk,
/// .do, .po, .nib or .woz images, or folders of them (in their names'
/// order) or .m3u playlists, the first in drive 1, F9 and F10
/// changing drive 1's and drive 2's; hard disk volumes among them (.hdv,
/// .2mg, and .po bigger than a floppy), up to two, the hard disk card's,
/// in slot 7, booted first; a Mockingboard in slot 4, or with
/// `--mockingboards=N` none, or two (4 and 5); its CPU its own (the 6502,
/// the enhanced IIe's 65C02), or with `--cpu=` a 6502 or a 65C02
/// (`65c02`, the base one, as the enhanced IIe's; `r65c02`, Rockwell's;
/// `w65c02`, WDC's); in a window, with `--fullscreen` fullscreen (F11 or a
/// double click switching), its picture in colour or with
/// `--monitor=white`, `green` or `amber` monochrome (F8 switching); or
/// with `--text` its text screen in the terminal. F2 saves its state, F4
/// loads it back: beside the first DISK, or the ROM, its name and .state.
/// `cpu_emu nes ROM [--fullscreen] [--overscan=N]` for the NES, ROM an
/// iNES image or a zip of one, its picture's edges hidden as a TV hides
/// them, N dots or lines each (8, or 0 to see it all).
use machines::apple2::video::Monitor;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "cpu_emu apple2 ROM | apple2e ROMS [--unenhanced] [DISK...] [--mockingboards=0|1|2] [--cpu=6502|65c02|r65c02|w65c02] [--monitor=colour|white|green|amber] [--fullscreen | --text] | nes ROM [--fullscreen] [--overscan=0..32]";
    match args.get(1).map(String::as_str) {
        Some("nes") => {
            let rom = args[2..]
                .iter()
                .find(|arg| !arg.starts_with("--"))
                .expect(usage);
            let overscan = match args.iter().find_map(|arg| arg.strip_prefix("--overscan=")) {
                Some(dots) => dots.parse().ok().filter(|&dots| dots <= 32).expect(usage),
                None => 8,
            };
            machines::nes::run(rom, args.iter().any(|arg| arg == "--fullscreen"), overscan);
        }
        Some(machine @ ("apple2" | "apple2e")) => {
            let files: Vec<&String> = args[2..]
                .iter()
                .filter(|arg| !arg.starts_with("--"))
                .collect();
            let rom = files.first().expect(usage);
            let text = args.iter().any(|arg| arg == "--text");
            let mockingboards = match args
                .iter()
                .find_map(|arg| arg.strip_prefix("--mockingboards="))
            {
                Some(count) => count.parse().ok().filter(|&count| count <= 2).expect(usage),
                None => 1,
            };
            let variant = match args.iter().find_map(|arg| arg.strip_prefix("--cpu=")) {
                None => None,
                Some("6502") => Some(cpu::Variant::Nmos6502),
                Some("65c02") => Some(cpu::Variant::Synertek65C02),
                Some("r65c02") => Some(cpu::Variant::Rockwell65C02),
                Some("w65c02") => Some(cpu::Variant::Wdc65C02),
                Some(_) => panic!("{usage}"),
            };
            let model = match (machine, args.iter().any(|arg| arg == "--unenhanced")) {
                ("apple2", _) => machines::apple2::Model::IIPlus,
                (_, true) => machines::apple2::Model::IIe,
                (_, false) => machines::apple2::Model::IIeEnhanced,
            };
            let disks: Vec<&str> = files[1..].iter().map(|disk| disk.as_str()).collect();
            let fullscreen = args.iter().any(|arg| arg == "--fullscreen");
            let monitor = match args.iter().find_map(|arg| arg.strip_prefix("--monitor=")) {
                Some(name) => *Monitor::ALL
                    .iter()
                    .find(|monitor| monitor.name() == name)
                    .expect(usage),
                None => Monitor::Colour,
            };
            machines::apple2::run(
                model,
                rom,
                &disks,
                mockingboards,
                variant,
                monitor,
                text,
                fullscreen,
            );
        }
        _ => machines::basic::run(),
    }
}
