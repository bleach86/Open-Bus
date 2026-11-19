//! The emulator, as a program: its command line, `cli`'s.

fn main() {
    cpu_emu::cli::run(std::env::args().collect());
}
