//! Klaus Dormann's 6502 tests, unmodified, run as they are meant to be: the
//! binary loaded into memory, reset, PC set to the test's start (as a
//! monitor would; the tests have no start-up of their own), execute. Each
//! test ends in a jump or branch to itself, where PC stays: at the success
//! trap if all passed. `tools/fetch-dormann` puts the tests (GPL-3.0, not
//! part of the repository) in target/dormann/; without them each test says
//! so and passes. execute runs at 8 MHz, so these take as long as the 6502
//! would.
//!
//! - The functional test: every documented instruction, addressing mode and
//!   flag, decimal mode, BRK and RTI. A 64K image; starts at $0400 and
//!   succeeds at $3469 in 96,241,364 cycles (as on the 6502).
//! - The decimal test (Bruce Clark's): ADC and SBC in decimal mode for every
//!   pair of bytes, both carries: the result and the carry. Assembled at
//!   $0200, its start; ends at `done` in 46,089,505 cycles, with ERROR ($0B)
//!   0 if all were right.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use std::path::{Path, PathBuf};

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

    #[test]
    fn dormann_functional_test() {
        let Ok(image) = std::fs::read(dir().join("6502_functional_test.bin")) else {
            eprintln!("dormann: no functional test; run tools/fetch-dormann");
            return;
        };
        let mut cpu = CPU::new();
        cpu.memory.data.copy_from_slice(&image);
        cpu.reset();
        cpu.pc = 0x0400;
        cpu.execute(96_241_364 + 1_000);

        let listing = std::fs::read_to_string(dir().join("6502_functional_test.lst")).unwrap_or_default();
        assert!(
            cpu.pc == 0x3469,
            "trapped at ${:04X} (A ${:02X} X ${:02X} Y ${:02X} SP ${:02X} P ${:02X}):\n{}",
            cpu.pc,
            cpu.a,
            cpu.x,
            cpu.y,
            cpu.sp,
            cpu.status.to_byte(),
            listing_at(&listing, cpu.pc)
        );
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
        let mut cpu = CPU::new();
        cpu.memory.data[0x0200..0x0200 + program.len()].copy_from_slice(&program);
        cpu.reset();
        cpu.pc = 0x0200;
        cpu.execute(46_089_505 + 1_000);

        let done = labels
            .lines()
            .find(|line| line.ends_with(" .done"))
            .map(|line| u16::from_str_radix(&line[3..9], 16).unwrap())
            .expect("done in the labels");
        assert_eq!(cpu.pc, done, "stopped short of done");
        // The zero page: N1 N2 HA HNVZC DA DNVZC AR NF VF ZF CF ERROR.
        assert_eq!(cpu.memory.data[0x0B], 0, "decimal test failed: {:02X?}", &cpu.memory.data[..12]);
    }
}
