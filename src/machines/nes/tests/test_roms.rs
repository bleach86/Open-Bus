//! The NES's test ROMs, run on it: nestest, its every instruction's state
//! and cycle, and its PPU's dot, as its log has them; and blargg's tests
//! of the CPU and PPU, which say how they did: the newer at $6000 (its
//! status, $80 while running, $81 to be reset, else its result, 0 passed;
//! then DE B0 61; then its text), the 2005 ones at $F8, or the PPU's at
//! $F0 (1 passed, else the number of the check that failed). They're in roms/nes/tests/ (not part
//! of the repository, from github.com/christopherpow/nes-test-roms);
//! without them, the tests say so and pass.

#[cfg(test)]
mod tests {
    use crate::host::machine::Machine;
    use crate::cpu::{Variant, CPU};
    use crate::machines::nes::cartridge::Cartridge;
    use crate::machines::nes::region::Region;
    use crate::machines::nes::{self, Nes, CLOCK_HZ};
    use std::path::{Path, PathBuf};

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/nes/tests/nes-test-roms")
    }

    /// The NES with the ROM at `path`, reset; none if it isn't there.
    fn nes(path: &Path) -> Option<CPU<Nes>> {
        nes_of(path, Region::Ntsc)
    }

    fn nes_of(path: &Path, region: Region) -> Option<CPU<Nes>> {
        let image = nes::image(path).ok()?;
        let mut cpu = CPU::with_bus(Nes::of(Cartridge::new(&image).unwrap(), region));
        cpu.variant = Variant::Ricoh2A03;
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();
        Some(cpu)
    }

    #[test]
    fn nestest_runs_as_its_log_does() {
        let (Some(mut cpu), Ok(log)) = (nes(&dir().join("other/nestest.nes")), std::fs::read_to_string(dir().join("other/nestest.log"))) else {
            eprintln!("nes test: no nestest in roms/nes/tests/");
            return;
        };
        cpu.pc = 0xC000; // its automatic start, no PPU needed
        for (n, line) in log.lines().enumerate() {
            let field = |name: &str| line.split(name).nth(1).unwrap().split_whitespace().next().unwrap();
            let hex = |name: &str| u8::from_str_radix(field(name), 16).unwrap();
            let want = (u16::from_str_radix(&line[..4], 16).unwrap(), hex("A:"), hex("X:"), hex("Y:"), hex("P:"), hex("SP:"));
            let ppu = line.split("PPU:").nth(1).unwrap();
            let (ppu_line, ppu_dot) = ppu[..7].split_once(',').unwrap();
            let want_ppu = (ppu_line.trim().parse::<u16>().unwrap(), ppu_dot.trim().parse::<u16>().unwrap());
            let cycles: u32 = field("CYC:").parse().unwrap();
            let p = (cpu.status.to_byte() & !0x10) | 0x20;
            let have = (cpu.pc, cpu.a, cpu.x, cpu.y, p, cpu.sp);
            assert_eq!(have, want, "line {}: {line}", n + 1);
            assert_eq!(cpu.clock, cycles, "line {}, its cycles: {line}", n + 1);
            assert_eq!((cpu.memory.ppu.line, cpu.memory.ppu.dot), want_ppu, "line {}, the PPU's dot: {line}", n + 1);
            cpu.execute(1);
        }
        assert_eq!((cpu.memory.ram[2], cpu.memory.ram[3]), (0, 0), "nestest's own results, $02 and $03");
    }

    /// A blargg test of the newer kind run: its result, and its text.
    fn newer(cpu: &mut CPU<Nes>) -> (u8, String) {
        let ram = |cpu: &mut CPU<Nes>, addr: u16| cpu.memory.cartridge.cpu_read(addr).unwrap_or(0);
        // Its Reset button: when it was asked for, when it was pressed, and
        // whether it's been let go, the status not yet written again.
        let (mut asked_at, mut reset_at, mut let_go) = (None, None, false);
        while cpu.clock < 60 * CLOCK_HZ {
            cpu.execute(CLOCK_HZ / 100);
            let signed = [0x6001, 0x6002, 0x6003].map(|addr| ram(cpu, addr)) == [0xDE, 0xB0, 0x61];
            let status = ram(cpu, 0x6000);
            if !signed {
                continue;
            }
            // To be reset: a couple of frames on, as the tests ask, its
            // button held a little, then let go.
            if status != 0x81 {
                (asked_at, let_go) = (None, false);
            }
            if status == 0x81 && asked_at.is_none() && !let_go {
                asked_at = Some(cpu.clock);
            }
            if asked_at.is_some_and(|at| cpu.clock - at > CLOCK_HZ / 20) && reset_at.is_none() && !let_go {
                reset_at = Some(cpu.clock);
                cpu.memory.reset_button(true);
            }
            if reset_at.is_some() {
                cpu.memory.reset_button(true);
            }
            if reset_at.is_some_and(|at| cpu.clock - at > CLOCK_HZ / 10) {
                cpu.memory.reset_button(false);
                (reset_at, let_go) = (None, true);
            }
            if status < 0x80 {
                let text: Vec<u8> = (0x6004..0x7000).map(|addr| ram(cpu, addr)).take_while(|&byte| byte != 0).collect();
                return (status, String::from_utf8_lossy(&text).trim().to_string());
            }
        }
        let text: Vec<u8> = (0x6004..0x7000).map(|addr| ram(cpu, addr)).take_while(|&byte| byte != 0).collect();
        (0xFF, format!("never finished: {}", String::from_utf8_lossy(&text).trim()))
    }

    /// A blargg test of 2005's run for `seconds`: its result, at `at`.
    fn old(cpu: &mut CPU<Nes>, seconds: u32, at: usize) -> u8 {
        cpu.execute(seconds * CLOCK_HZ);
        cpu.memory.ram[at]
    }

    /// Each of `roms` run as `run` has it, `passed` its result if it
    /// passed: those that didn't, and how.
    fn run_all(roms: &[&str], run: impl Fn(&mut CPU<Nes>) -> (bool, String)) {
        run_all_of(Region::Ntsc, roms, run)
    }

    /// As `run_all`, on a NES of `region`.
    fn run_all_of(region: Region, roms: &[&str], run: impl Fn(&mut CPU<Nes>) -> (bool, String)) {
        let mut failed = Vec::new();
        let mut missing = 0;
        for rom in roms {
            let Some(mut cpu) = nes_of(&dir().join(rom), region) else {
                missing += 1;
                continue;
            };
            let (passed, how) = run(&mut cpu);
            if !passed {
                failed.push(format!("{rom}: {how}"));
            }
        }
        if missing == roms.len() {
            eprintln!("nes test: no test ROMs in roms/nes/tests/");
        }
        assert!(failed.is_empty(), "{} of {} failed:\n{}", failed.len(), roms.len(), failed.join("\n"));
    }

    fn newer_passes(cpu: &mut CPU<Nes>) -> (bool, String) {
        let (status, text) = newer(cpu);
        (status == 0, format!("{status}: {text}"))
    }

    fn old_passes(seconds: u32, at: usize) -> impl Fn(&mut CPU<Nes>) -> (bool, String) {
        move |cpu| {
            let result = old(cpu, seconds, at);
            (result == 1, format!("failed check {result}"))
        }
    }

    #[test]
    fn blargg_cpu_instructions() {
        let roms: Vec<String> = [
            "01-basics", "02-implied", "03-immediate", "04-zero_page", "05-zp_xy", "06-absolute", "07-abs_xy",
            "08-ind_x", "09-ind_y", "10-branches", "11-stack", "12-jmp_jsr", "13-rts", "14-rti", "15-brk", "16-special",
        ]
        .iter()
        .map(|name| format!("instr_test-v5/rom_singles/{name}.nes"))
        .collect();
        run_all(&roms.iter().map(String::as_str).collect::<Vec<_>>(), newer_passes);
    }

    #[test]
    fn blargg_cpu_bus() {
        run_all(
            &["cpu_dummy_writes/cpu_dummy_writes_oam.nes", "cpu_dummy_writes/cpu_dummy_writes_ppumem.nes", "cpu_exec_space/test_cpu_exec_space_ppuio.nes"],
            newer_passes,
        );
    }

    #[test]
    fn blargg_branch_timing() {
        run_all(&["branch_timing_tests/1.Branch_Basics.nes", "branch_timing_tests/2.Backward_Branch.nes", "branch_timing_tests/3.Forward_Branch.nes"], old_passes(4, 0xF8));
    }

    #[test]
    fn blargg_ppu_vbl_nmi() {
        let roms: Vec<String> = [
            "01-vbl_basics", "02-vbl_set_time", "03-vbl_clear_time", "04-nmi_control", "05-nmi_timing", "06-suppression",
            "07-nmi_on_timing", "08-nmi_off_timing", "09-even_odd_frames", "10-even_odd_timing",
        ]
        .iter()
        .map(|name| format!("ppu_vbl_nmi/rom_singles/{name}.nes"))
        .collect();
        run_all(&roms.iter().map(String::as_str).collect::<Vec<_>>(), newer_passes);
    }

    #[test]
    fn blargg_ppu_memory() {
        run_all(&["oam_read/oam_read.nes", "oam_stress/oam_stress.nes", "ppu_open_bus/ppu_open_bus.nes"], newer_passes);
        run_all(
            &[
                "blargg_ppu_tests_2005.09.15b/palette_ram.nes",
                "blargg_ppu_tests_2005.09.15b/sprite_ram.nes",
                "blargg_ppu_tests_2005.09.15b/vbl_clear_time.nes",
                "blargg_ppu_tests_2005.09.15b/vram_access.nes",
            ],
            old_passes(3, 0xF0),
        );
    }

    #[test]
    fn blargg_sprites() {
        let hits: Vec<String> = [
            "01.basics", "02.alignment", "03.corners", "04.flip", "05.left_clip", "06.right_edge", "07.screen_bottom",
            "08.double_height", "09.timing_basics", "10.timing_order", "11.edge_timing",
        ]
        .iter()
        .map(|name| format!("sprite_hit_tests_2005.10.05/{name}.nes"))
        .collect();
        run_all(&hits.iter().map(String::as_str).collect::<Vec<_>>(), old_passes(4, 0xF8));
        run_all(
            &["sprite_overflow_tests/1.Basics.nes", "sprite_overflow_tests/2.Details.nes", "sprite_overflow_tests/3.Timing.nes", "sprite_overflow_tests/4.Obscure.nes", "sprite_overflow_tests/5.Emulator.nes"],
            old_passes(4, 0xF8),
        );
    }

    #[test]
    fn blargg_vbl_nmi_timing() {
        let roms: Vec<String> = ["1.frame_basics", "2.vbl_timing", "3.even_odd_frames", "4.vbl_clear_timing", "5.nmi_suppression", "6.nmi_disable", "7.nmi_timing"]
            .iter()
            .map(|name| format!("vbl_nmi_timing/{name}.nes"))
            .collect();
        run_all(&roms.iter().map(String::as_str).collect::<Vec<_>>(), old_passes(8, 0xF8));
    }

    /// The ROMs named `names` in `folder`, as `.nes` files.
    fn named(folder: &str, names: &[&str]) -> Vec<String> {
        names.iter().map(|name| format!("{folder}/{name}.nes")).collect()
    }

    fn strs(roms: &[String]) -> Vec<&str> {
        roms.iter().map(String::as_str).collect()
    }

    #[test]
    fn blargg_apu() {
        let roms = named(
            "apu_test/rom_singles",
            &["1-len_ctr", "2-len_table", "3-irq_flag", "4-jitter", "5-len_timing", "6-irq_flag_timing", "7-dmc_basics", "8-dmc_rates"],
        );
        run_all(&strs(&roms), newer_passes);
    }

    #[test]
    fn blargg_apu_pal() {
        let roms = named(
            "pal_apu_tests",
            &[
                "01.len_ctr", "02.len_table", "03.irq_flag", "04.clock_jitter", "05.len_timing_mode0", "06.len_timing_mode1",
                "07.irq_flag_timing", "08.irq_timing", "10.len_halt_timing", "11.len_reload_timing",
            ],
        );
        run_all_of(Region::Pal, &strs(&roms), old_passes(30, 0xF8));
    }

    #[test]
    fn blargg_apu_2005() {
        let roms = named(
            "blargg_apu_2005.07.30",
            &[
                "01.len_ctr", "02.len_table", "03.irq_flag", "04.clock_jitter", "05.len_timing_mode0", "06.len_timing_mode1",
                "07.irq_flag_timing", "08.irq_timing", "09.reset_timing", "10.len_halt_timing", "11.len_reload_timing",
            ],
        );
        run_all(&strs(&roms), old_passes(10, 0xF0));
    }

    #[test]
    fn blargg_apu_reset() {
        let roms = named(
            "apu_reset",
            &["4015_cleared", "4017_timing", "4017_written", "irq_flag_cleared", "len_ctrs_enabled", "works_immediately"],
        );
        run_all(&strs(&roms), newer_passes);
    }

    #[test]
    fn blargg_cpu_interrupts_and_timing() {
        let mut roms = named(
            "cpu_interrupts_v2/rom_singles",
            &["1-cli_latency", "2-nmi_and_brk", "3-nmi_and_irq", "4-irq_and_dma", "5-branch_delays_irq"],
        );
        roms.extend(named("instr_timing/rom_singles", &["1-instr_timing", "2-branch_timing"]));
        roms.extend(named("sprdma_and_dmc_dma", &["sprdma_and_dmc_dma", "sprdma_and_dmc_dma_512"]));
        run_all(&strs(&roms), newer_passes);
    }

    #[test]
    fn blargg_on_mmc1() {
        run_all(
            &[
                "instr_test-v5/all_instrs.nes",
                "instr_test-v5/official_only.nes",
                "cpu_interrupts_v2/cpu_interrupts.nes",
                "ppu_vbl_nmi/ppu_vbl_nmi.nes",
                "apu_test/apu_test.nes",
                "instr_misc/instr_misc.nes",
                "instr_timing/instr_timing.nes",
            ],
            newer_passes,
        );
    }

    #[test]
    fn blargg_misc() {
        let mut roms = named("instr_misc/rom_singles", &["01-abs_x_wrap", "02-branch_wrap", "03-dummy_reads", "04-dummy_reads_apu"]);
        roms.extend(named("cpu_reset", &["ram_after_reset", "registers"]));
        roms.push("ppu_read_buffer/test_ppu_read_buffer.nes".to_string());
        run_all(&strs(&roms), newer_passes);
    }

    #[test]
    fn blargg_mmc3() {
        // The fifth for the MMC3 most boards have; the sixth, the older's
        // IRQ, isn't.
        let roms = named("mmc3_test_2/rom_singles", &["1-clocking", "2-details", "3-A12_clocking", "4-scanline_timing", "5-MMC3"]);
        run_all(&strs(&roms), newer_passes);
    }
}
