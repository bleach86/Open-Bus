//! Klaus Dormann's 6502 functional test against the CPU: every documented
//! instruction, addressing mode and flag, decimal mode, BRK and RTI. The
//! test is a 64K memory image that starts at $0400 and ends in a branch or
//! jump to itself: at $3469 when everything passed, anywhere else at the
//! first thing that didn't. `tools/fetch-dormann` fetches it (GPL-3.0, kept
//! out of the repository) into target/dormann/; without it this test says
//! so and passes.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::monitor::Link;
    use crate::ram::{Host, Memory};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    const START: u16 = 0x0400;
    const SUCCESS: u16 = 0x3469;

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

    /// Stop the CPU through the monitor and read its registers line.
    fn registers(link: &Link) -> String {
        let said = Said::default();
        let lines = link.attach(Box::new(said.clone()));
        said.until_prompt();
        lines.send("r".into()).unwrap();
        let answer = said.until_prompt();
        lines.send("q".into()).unwrap();
        answer.lines().find(|line| line.starts_with("pc $")).unwrap_or_default().to_string()
    }

    #[test]
    fn dormann_functional_test() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/dormann");
        let Ok(image) = std::fs::read(dir.join("6502_functional_test.bin")) else {
            eprintln!("dormann: no test image; run tools/fetch-dormann");
            return;
        };
        assert_eq!(image.len(), 0x10000);

        // The machine as it is, the test in it: $C000-$FFFF (its vectors) as
        // the system ROM, $0000-$7FFF in RAM; it uses nothing in between.
        // The CPU runs on its own thread, at the machine's 8 MHz, until the
        // test process ends.
        let link = Link::new(None);
        let mut cpu = CPU::new();
        cpu.memory = Memory::new(Host { monitor: Some(Arc::clone(&link)), ..Host::default() });
        let rom: &[u8; crate::cart::ROM_SIZE] = image[0xC000..].try_into().unwrap();
        cpu.power_on(rom);
        for (addr, &byte) in image[..0x8000].iter().enumerate() {
            cpu.memory.write(addr as u16, byte);
        }
        cpu.pc = START;
        std::thread::spawn(move || cpu.execute(u32::MAX));

        // Once a second, where is it? The same place three times: a trap.
        let started = Instant::now();
        let mut seen = Vec::new();
        let state = loop {
            std::thread::sleep(Duration::from_secs(1));
            let state = registers(&link);
            eprintln!("dormann: {:3.0}s {}", started.elapsed().as_secs_f64(), state.trim_end());
            assert!(started.elapsed() < Duration::from_secs(600), "still running after 10 minutes: {state}");
            // The registers; not the video beam's line after them, which moves.
            let registers = state.split("  line").next().unwrap_or("").to_string();
            seen.push(registers);
            if seen.len() >= 3 && seen[seen.len() - 3..].iter().all(|s| *s == seen[seen.len() - 1]) {
                break state;
            }
        };
        let pc = u16::from_str_radix(&state[4..8], 16).unwrap();
        let listing = std::fs::read_to_string(dir.join("6502_functional_test.lst")).unwrap_or_default();
        assert!(pc == SUCCESS, "trapped: {state}\n{}", listing_at(&listing, pc));
        eprintln!("dormann: passed in {:.0}s at the machine's 8 MHz", started.elapsed().as_secs_f64());
    }
}
