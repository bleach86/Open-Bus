//! Klaus Dormann's 6502 tests against the CPU. `tools/fetch-dormann` fetches
//! them (GPL-3.0, kept out of the repository) into target/dormann/; without
//! them these tests say so and pass.
//!
//! - The functional test: every documented instruction, addressing mode and
//!   flag, decimal mode, BRK and RTI. A 64K image that starts at $0400 and
//!   ends in a branch or jump to itself: at $3469 when everything passed,
//!   anywhere else at the first thing that didn't.
//! - The decimal test (Bruce Clark's): ADC and SBC in decimal mode for every
//!   pair of bytes, both carries, valid BCD or not, the result and N, V, Z
//!   and C against the NMOS 6502's. It ends at `done` with ERROR ($0B) 0 if
//!   all were right.
//!
//! The CPU runs on its own thread at the machine's 8 MHz; the test stops it
//! through the monitor once a second to see where it is.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::monitor::Link;
    use crate::ram::{Host, Memory};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/dormann")
    }

    /// The listing's lines up to `addr`: what the test was checking there.
    fn listing_at(listing: &str, addr: u16) -> String {
        let key = format!("{addr:04x} :");
        let lines: Vec<&str> = listing.lines().collect();
        match lines.iter().position(|line| line.starts_with(&key)) {
            Some(at) => lines[at.saturating_sub(15)..(at + 2).min(lines.len())].join("\n"),
            None => format!("(${addr:04X} not in the listing)"),
        }
    }

    /// What the monitor says, for the test to read.
    #[derive(Clone, Default)]
    struct Said(Arc<Mutex<String>>);

    impl std::io::Write for Said {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().push_str(&String::from_utf8_lossy(bytes));
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Said {
        fn until_prompt(&self) -> String {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                let mut said = self.0.lock().unwrap();
                if said.contains("(monitor) ") {
                    return std::mem::take(&mut *said);
                }
                drop(said);
                std::thread::sleep(Duration::from_millis(1));
            }
            panic!("the monitor never answered");
        }
    }

    /// Stop the CPU through the monitor, run `command`, let it go on.
    fn ask(link: &Link, command: &str) -> String {
        let said = Said::default();
        let lines = link.attach(Box::new(said.clone()));
        said.until_prompt();
        lines.send(command.into()).unwrap();
        let answer = said.until_prompt();
        lines.send("q".into()).unwrap();
        answer
    }

    /// A machine with `ram` from $0000 and `rom` (16K, the vectors) at
    /// $C000, run from `start` until it traps: the monitor's link, and the
    /// registers line then.
    fn run_to_trap(ram: &[u8], rom: &[u8; crate::cart::ROM_SIZE], start: u16, name: &str) -> (Arc<Link>, String) {
        let link = Link::new(None);
        let mut cpu = CPU::new();
        cpu.memory = Memory::new(Host { monitor: Some(Arc::clone(&link)), ..Host::default() });
        cpu.power_on(rom);
        for (addr, &byte) in ram.iter().enumerate() {
            cpu.memory.write(addr as u16, byte);
        }
        cpu.pc = start;
        std::thread::spawn(move || cpu.execute(u32::MAX));

        // Once a second, where is it? The same registers three times: a trap.
        let started = Instant::now();
        let mut seen = Vec::new();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let answer = ask(&link, "r");
            let state = answer.lines().find(|line| line.starts_with("pc $")).unwrap_or_default().to_string();
            eprintln!("{name}: {:3.0}s {state}", started.elapsed().as_secs_f64());
            assert!(started.elapsed() < Duration::from_secs(600), "still running after 10 minutes: {state}");
            // The registers; not the video beam's line after them, which moves.
            seen.push(state.split("  line").next().unwrap_or("").to_string());
            if seen.len() >= 3 && seen[seen.len() - 3..].iter().all(|s| *s == seen[seen.len() - 1]) {
                return (link, state);
            }
        }
    }

    fn pc_of(state: &str) -> u16 {
        u16::from_str_radix(&state[4..8], 16).unwrap()
    }

    #[test]
    fn dormann_functional_test() {
        let Ok(image) = std::fs::read(dir().join("6502_functional_test.bin")) else {
            eprintln!("dormann: no functional test; run tools/fetch-dormann");
            return;
        };
        assert_eq!(image.len(), 0x10000);
        // $C000-$FFFF (its vectors) as the system ROM, $0000-$7FFF in RAM;
        // it uses nothing in between.
        let rom: &[u8; crate::cart::ROM_SIZE] = image[0xC000..].try_into().unwrap();
        let started = Instant::now();
        let (_, state) = run_to_trap(&image[..0x8000], rom, 0x0400, "functional");
        let listing = std::fs::read_to_string(dir().join("6502_functional_test.lst")).unwrap_or_default();
        assert!(pc_of(&state) == 0x3469, "trapped: {state}\n{}", listing_at(&listing, pc_of(&state)));
        eprintln!("functional: passed in {:.0}s at the machine's 8 MHz", started.elapsed().as_secs_f64());
    }

    #[test]
    fn dormann_decimal_test() {
        let (Ok(program), Ok(labels)) = (
            std::fs::read(dir().join("6502_decimal_test.bin")),
            std::fs::read_to_string(dir().join("6502_decimal_test.lbl")),
        ) else {
            eprintln!("dormann: no decimal test; run tools/fetch-dormann");
            return;
        };
        let done = labels
            .lines()
            .find(|line| line.ends_with(" .done"))
            .map(|line| u16::from_str_radix(&line[3..9], 16).unwrap())
            .expect("done in the labels");
        let mut ram = vec![0; 0x0200];
        ram.extend_from_slice(&program);
        let started = Instant::now();
        let (link, state) = run_to_trap(&ram, &[0xFF; crate::cart::ROM_SIZE], 0x0200, "decimal");
        assert_eq!(pc_of(&state), done, "stopped short of done: {state}");

        // The zero page: N1 N2 HA HNVZC DA DNVZC AR NF VF ZF CF ERROR. On an
        // error, N1 and N2 (and Y, the carry) are the case, DA/DNVZC what
        // the CPU gave, AR and NF VF ZF CF what it should have.
        let dump = ask(&link, "m 0 f");
        let bytes: Vec<u8> = dump
            .lines()
            .find(|line| line.starts_with("$0000"))
            .expect("the zero page dumped")
            .split_whitespace()
            .skip(1)
            .take(16)
            .filter_map(|b| u8::from_str_radix(b, 16).ok())
            .collect();
        assert_eq!(bytes.get(11), Some(&0), "decimal test failed:\n{dump}");
        eprintln!("decimal: passed in {:.0}s at the machine's 8 MHz", started.elapsed().as_secs_f64());
    }
}
