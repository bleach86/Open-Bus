mod apple2;
#[cfg(test)]
mod apple2_test;
mod bus;
#[cfg(test)]
mod bus_test;
#[cfg(test)]
mod clock_test;
mod cpu;
mod display;
#[cfg(test)]
mod disk2_test;
#[cfg(test)]
mod dormann;
#[cfg(test)]
mod harte;
mod instructions;
#[cfg(test)]
mod interrupt_test;
mod ram;
mod terminal;
mod window;

use crate::apple2::Apple2;
use crate::cpu::CPU;
use crate::ram::Memory;
use crate::terminal::Terminal;
use crate::window::Window;

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
            apple2(rom, files.get(1).map(|disk| disk.as_str()), args.iter().any(|arg| arg == "--text"));
        }
        _ => basic(),
    }
}

fn apple2(rom_path: &str, disk: Option<&str>, text: bool) {
    let read = |path: &std::path::Path| std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let rom_path = std::path::Path::new(rom_path);
    let rom = read(rom_path);
    assert!(rom.len() >= 0x3000, "the ROM is 12K, $D000 to $FFFF");
    let mut apple = Apple2::new(rom[rom.len() - 0x3000..].to_vec());

    if let Some(disk) = disk {
        let beside = |name: &str| read(&rom_path.with_file_name(name));
        let mut card = apple2::disk2::Disk2::new(beside("disk2-p5-341-0027.bin"), &beside("disk2-p6-341-0028.bin"))
            .unwrap_or_else(|e| panic!("the Disk II's ROMs: {e}"));
        let image = read(std::path::Path::new(disk));
        let lower = disk.to_lowercase();
        card.disk = Some(
            if lower.ends_with(".nib") {
                apple2::disk_image::Disk::from_nib(&image)
            } else if lower.ends_with(".dsk") || lower.ends_with(".do") {
                apple2::disk_image::Disk::from_dsk(&image)
            } else {
                Err("a .dsk, .do or .nib".to_string())
            }
            .unwrap_or_else(|e| panic!("{disk}: {e}")),
        );
        apple.disk2 = Some(card);
    }
    if text {
        apple.terminal = Some(Terminal::new());
        print!("\x1B[2J"); // the screen cleared, for the Apple's
    } else {
        let (width, height) = (apple2::video::WIDTH, apple2::video::HEIGHT);
        apple.display = Some(Box::new(Window::new("Apple ][", width, height, 3)));
    }

    let mut cpu = CPU::with_bus(apple);
    cpu.clock_hz = apple2::CLOCK_HZ;
    cpu.reset();
    cpu.execute_forever();
}

fn basic() {
    let mut cpu = CPU::new();

    // BASIC's ROM: the top 16K, its vectors included; RAM below it.
    let rom = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/roms/basic.bin"))
        .expect("roms/basic.bin: run tools/fetch-msbasic");
    let rom_start = 0x10000 - rom.len();
    cpu.memory.data[rom_start..].copy_from_slice(&rom);
    cpu.memory.rom_start = Some(rom_start as u16);
    cpu.memory.terminal = Some(Terminal::new());

    cpu.reset();
    cpu.execute_forever();
}
