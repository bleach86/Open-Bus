//! The CPU's speed: run at clock_hz, a program takes the wall time its
//! cycles take at that speed, and does exactly the same as at any other.
//! The program is a loop, INX then JMP back: 5 cycles a time round.
//!
//! A run may end late, by a little, when the host is busy (the Harte test
//! runs on every core alongside); it may never end early, by more than the
//! last half millisecond, which execute's pacing doesn't wait out.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use std::time::{Duration, Instant};

    const PAL_C64: u32 = 985_248;
    const NTSC_NES: u32 = 1_789_773;

    /// The loop at $0400, reset into, run for `cycles` at `hz`: the CPU, the
    /// cycles it ran (the clock counts reset's 7 too), and how long it took.
    fn run(hz: u32, cycles: u32) -> (CPU, u32, Duration) {
        let mut cpu = CPU::new();
        let data = &mut cpu.memory.data;
        data[0x0400..0x0404].copy_from_slice(&[0xE8, 0x4C, 0x00, 0x04]); // INX; JMP $0400
        data[0xFFFC..0xFFFE].copy_from_slice(&[0x00, 0x04]);
        cpu.reset();
        cpu.clock_hz = hz;
        let before = cpu.clock;
        let start = Instant::now();
        cpu.execute(cycles);
        let took = start.elapsed();
        let ran = cpu.clock.wrapping_sub(before);
        (cpu, ran, took)
    }

    /// That `cycles` at `hz` took `took`: not early, and not much late.
    fn check_time(hz: u32, cycles: u32, took: Duration) {
        let due = Duration::from_nanos((cycles as u128 * 1_000_000_000 / hz as u128) as u64);
        let early = Duration::from_micros(500);
        let late = Duration::from_millis(20);
        assert!(
            took + early >= due,
            "{cycles} cycles at {hz} Hz: {took:?}, early for {due:?}"
        );
        assert!(
            took <= due + late,
            "{cycles} cycles at {hz} Hz: {took:?}, late for {due:?}"
        );
    }

    #[test]
    fn the_speed_changes_only_the_time() {
        let (slow, slow_ran, _) = run(1_000_000, 50_000);
        let (fast, fast_ran, _) = run(8_000_000, 50_000);
        let (nes, nes_ran, _) = run(NTSC_NES, 50_000);
        assert_eq!(slow_ran, 50_000, "50,000 cycles are 10,000 times round");
        for (cpu, ran) in [(&fast, fast_ran), (&nes, nes_ran)] {
            assert_eq!(ran, slow_ran, "cycles run");
            assert_eq!(
                (cpu.pc, cpu.x),
                (slow.pc, slow.x),
                "where the program got to"
            );
        }
        assert_eq!(slow.x, (10_000 % 256) as u8);
    }

    #[test]
    fn it_runs_at_its_speed() {
        for hz in [PAL_C64, 1_000_000, NTSC_NES, 8_000_000] {
            let cycles = hz / 4 / 5 * 5; // a quarter of a second, whole times round
            let (_, ran, took) = run(hz, cycles);
            assert_eq!(ran, cycles);
            check_time(hz, cycles, took);
        }
    }

    #[test]
    fn it_does_not_drift() {
        // A whole second at the NES's speed, 894.8865 cycles a half
        // millisecond: rounding them would show as early or late by now.
        let cycles = NTSC_NES / 5 * 5;
        let (_, ran, took) = run(NTSC_NES, cycles);
        assert_eq!(ran, cycles);
        check_time(NTSC_NES, cycles, took);
    }
}
