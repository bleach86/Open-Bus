mod backlight;
mod cart;
mod cpu;
mod dormann;
mod drive;
mod gamepad;
mod harte;
mod input;
mod instructions;
mod monitor;
mod ram;
mod render;
mod rom;
mod sound;
mod status;
mod vdp;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const USAGE: &str = "usage: cpu_emu [--drive DIR] [--launcher] [--fullscreen] [--blank-after SECONDS] [--monitor PORT]";

/// `cpu_emu [--drive DIR] [--launcher] [--fullscreen] [--blank-after SECONDS] [--monitor PORT]`
///
/// - `--drive`: the drive's disk, a directory (default: `drive` beside the
///   executable). The flash cart's save memory is kept there too, in `.save`.
/// - `--launcher`: a launcher started the emulator and acts on the status it
///   powers off with (on the RG351MP: games, reboot, power off).
/// - `--fullscreen`: start full screen; under a kiosk compositor (cage) this
///   also keeps the window from leaving room for a title bar.
/// - `--monitor`: the machine-language monitor (monitor.rs) on
///   127.0.0.1:PORT.
/// - `--blank-after`: on battery, turn the host's backlight off after SECONDS
///   without input.
fn main() {
    let mut drive = None;
    let mut launcher = false;
    let mut fullscreen = false;
    let mut monitor_port = None;
    let mut blank_after = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--drive" => drive = args.next().map(PathBuf::from),
            "--launcher" => launcher = true,
            "--fullscreen" => fullscreen = true,
            "--monitor" => {
                monitor_port = Some(
                    args.next()
                        .and_then(|s| s.parse::<u16>().ok())
                        .unwrap_or_else(|| {
                            eprintln!("{USAGE}");
                            std::process::exit(2);
                        }),
                );
            }
            "--blank-after" => {
                let seconds = args.next().and_then(|s| s.parse().ok());
                blank_after = Some(Duration::from_secs(seconds.unwrap_or_else(|| {
                    eprintln!("{USAGE}");
                    std::process::exit(2);
                })));
            }
            _ => {
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let drive = drive.unwrap_or_else(|| {
        let exe = std::env::current_exe().unwrap_or_default();
        exe.with_file_name("drive")
    });

    let monitor = monitor_port.map(|port| {
        let link = monitor::Link::new(Some(drive.clone()));
        if let Err(error) = link.listen(port) {
            eprintln!("monitor: 127.0.0.1:{port}: {error}");
            std::process::exit(1);
        }
        link
    });
    let host = ram::Host {
        status: status::Status::start(),
        display: Some(Arc::new(render::FrameSlot::default())),
        save_file: Some(drive.join(".save")),
        drive: Some(drive),
        launcher,
        monitor,
        ..ram::Host::default()
    };
    let input = Arc::clone(&host.input);
    let status = Arc::clone(&host.status);
    let frames = Arc::clone(host.display.as_ref().unwrap());

    std::thread::spawn(move || {
        let mut cpu = cpu::CPU::new();
        cpu.memory = ram::Memory::new(host);
        loop {
            cpu.power_on(rom::SYSTEM);
            if !cpu.execute(u32::MAX) {
                break; // the CPU stopped on its own; leave the last picture up
            }
        }
    });

    let blanker = blank_after.map(|after| Arc::new(Mutex::new(backlight::Blanker::new(after))));
    gamepad::Gamepads::start(Arc::clone(&input), blanker.clone());
    render::run(input, frames, status, fullscreen, blanker);
}
