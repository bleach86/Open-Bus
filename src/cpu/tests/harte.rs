//! Tom Harte's 6502 tests (SingleStepTests): for each documented opcode,
//! 10,000 cases of one instruction each, all run, the state before and after and its
//! cycles. Each runs as the Dormann tests do: memory loaded, reset, the
//! registers set as the case has them, execute for the instruction's
//! cycles; then the registers and memory are compared with the case's.
//!
//! The cycles are checked by the byte after the instruction, whose value it
//! doesn't use: it is made a JAM ($02, an opcode the CPU doesn't run), so
//! that execute stops on it, one past it, if any cycles are left over. Run
//! for its cycles, the instruction must leave none; run for one more, it
//! must leave that one. Where one is wrong, the cycles it does take are the
//! most it can be given and still leave none. Then its bus cycles, from the
//! memory's bus log, are compared with the case's: each read or write, its
//! address and value, in order.
//!
//! `tools/fetch-harte` puts the tests (MIT, not part of the repository) in
//! target/harte/; without them the test says so and passes.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use std::path::{Path, PathBuf};

    const JAM: u8 = 0x02;

    struct State {
        pc: u16,
        s: u8,
        a: u8,
        x: u8,
        y: u8,
        p: u8,
        ram: Vec<(u16, u8)>,
    }

    struct Case {
        initial: State,
        last: State,
        /// Each cycle's bus access: address, value, whether a write.
        cycles: Vec<(u16, u8, bool)>,
    }

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/harte")
    }

    /// A case's line: see tools/fetch-harte.
    fn parse(line: &str) -> Case {
        let sections: Vec<&str> = line.split('|').collect();
        let parts: Vec<Vec<u32>> = sections[..4]
            .iter()
            .map(|part| part.split_whitespace().map(|n| n.parse().unwrap()).collect())
            .collect();
        let words: Vec<&str> = sections[4].split_whitespace().collect();
        let state = |regs: &[u32], ram: &[u32]| State {
            pc: regs[0] as u16,
            s: regs[1] as u8,
            a: regs[2] as u8,
            x: regs[3] as u8,
            y: regs[4] as u8,
            p: regs[5] as u8,
            ram: ram.chunks(2).map(|pair| (pair[0] as u16, pair[1] as u8)).collect(),
        };
        Case {
            initial: state(&parts[0], &parts[1]),
            last: state(&parts[2], &parts[3]),
            cycles: words
                .chunks(3)
                .map(|cycle| (cycle[0].parse().unwrap(), cycle[1].parse().unwrap(), cycle[2] == "w"))
                .collect(),
        }
    }

    /// Whether the byte after the instruction can be made a JAM: whether
    /// the instruction doesn't use it. A one-byte instruction reads it on
    /// its second cycle and throws it away; other than that, it mustn't be
    /// touched.
    fn can_jam(case: &Case) -> bool {
        let opcode = case.initial.ram.iter().find(|&&(addr, _)| addr == case.initial.pc).unwrap().1;
        let one_byte = matches!(opcode & 0x0F, 0x08 | 0x0A) || matches!(opcode, 0x00 | 0x40 | 0x60);
        case.cycles
            .iter()
            .enumerate()
            .all(|(i, &(addr, _, _))| addr != case.last.pc || (one_byte && i == 1))
    }

    /// The case run for `cycles` cycles, the bus logged from after reset;
    /// whether the byte after the instruction was made a JAM.
    fn run(case: &Case, cycles: u32) -> (CPU, bool) {
        let mut cpu = CPU::new();
        for &(addr, value) in &case.initial.ram {
            cpu.memory.data[addr as usize] = value;
        }
        let jam = can_jam(case);
        if jam {
            cpu.memory.data[case.last.pc as usize] = JAM;
        }
        cpu.reset();
        cpu.pc = case.initial.pc;
        cpu.sp = case.initial.s;
        cpu.a = case.initial.a;
        cpu.x = case.initial.x;
        cpu.y = case.initial.y;
        cpu.status.from_byte(case.initial.p);
        cpu.memory.bus_log = Some(Vec::new());
        cpu.execute(cycles);
        (cpu, jam)
    }

    /// PC S A X Y P. Bits 4 and 5 of P are not in the register, only in
    /// what's pushed.
    type Regs = (u16, u8, u8, u8, u8, u8);

    fn regs(cpu: &CPU) -> Regs {
        (cpu.pc, cpu.sp, cpu.a, cpu.x, cpu.y, cpu.status.to_byte() & !0x30)
    }

    fn want(state: &State) -> Regs {
        (state.pc, state.s, state.a, state.x, state.y, state.p & !0x30)
    }

    fn show((pc, s, a, x, y, p): Regs) -> String {
        format!("{pc:04X} {s:02X} {a:02X} {x:02X} {y:02X} {p:02X}")
    }

    /// The cycles the instruction takes: the most it can be given and leave
    /// none over, with the right result. None if no count up to a few more
    /// than it should take does that: it takes fewer than it won't start
    /// without.
    fn cycles_taken(case: &Case) -> Option<u32> {
        let cycles = case.cycles.len() as u32;
        (1..=cycles + 4).rev().find(|&given| regs(&run(case, given).0) == want(&case.last))
    }

    /// What's wrong with the CPU's run of the case, if anything.
    fn check(case: &Case) -> Option<String> {
        let cycles = case.cycles.len() as u32;
        let last = &case.last;
        let (cpu, jam) = run(case, cycles);
        let wrong_cycles = || match cycles_taken(case) {
            Some(taken) => format!("takes {taken} cycles, not {cycles}"),
            None => format!("takes fewer than {cycles} cycles, but won't start with fewer"),
        };

        if regs(&cpu) != want(last) {
            if jam && cpu.pc == last.pc.wrapping_add(1) {
                return Some(wrong_cycles());
            }
            if regs(&cpu) == want(&case.initial) {
                return Some(format!("didn't run in {cycles} cycles"));
            }
            return Some(format!("PC S A X Y P: got {}, want {}", show(regs(&cpu)), show(want(last))));
        }
        for &(addr, value) in &last.ram {
            let at = cpu.memory.data[addr as usize];
            if at != value && !(jam && addr == last.pc) {
                return Some(format!("${addr:04X}: got {at:02X}, want {value:02X}"));
            }
        }

        if jam && run(case, cycles + 1).0.pc == last.pc {
            return Some(wrong_cycles());
        }

        let log = cpu.memory.bus_log.as_ref().unwrap();
        let access = |(addr, value, write): (u16, u8, bool)| {
            format!("{} ${addr:04X} {value:02X}", if write { "write" } else { "read" })
        };
        for (i, &(addr, value, write)) in case.cycles.iter().enumerate() {
            // A JAM put in the byte after reads as one.
            let value = if jam && addr == last.pc && !write { JAM } else { value };
            match log.get(i) {
                Some(&got) if got == (addr, value, write) => {}
                got => {
                    let got = got.map_or("nothing".into(), |&got| access(got));
                    return Some(format!("cycle {}: {got}, want {}", i + 1, access((addr, value, write))));
                }
            }
        }
        if log.len() > case.cycles.len() {
            return Some(format!("cycle {}: {}, want nothing", case.cycles.len() + 1, access(log[case.cycles.len()])));
        }
        None
    }

    /// What's wrong with an opcode's cases, if anything.
    fn check_file(file: &Path) -> Option<String> {
        let text = std::fs::read_to_string(file).unwrap();
        let mut count = 0;
        let mut total = 0;
        let mut first = None;
        for line in text.lines() {
            total += 1;
            let case = parse(line);
            if let Some(wrong) = check(&case) {
                count += 1;
                let bytes: Vec<String> = (0..3)
                    .map(|i| {
                        let addr = case.initial.pc.wrapping_add(i);
                        let value = case.initial.ram.iter().find(|&&(a, _)| a == addr);
                        value.map_or("??".into(), |&(_, v)| format!("{v:02X}"))
                    })
                    .collect();
                first.get_or_insert(format!("{}: {wrong}", bytes.join(" ")));
            }
        }
        let name = file.file_stem().unwrap().to_string_lossy().to_uppercase();
        first.map(|first| format!("${name}: {count}/{total} wrong, first {first}"))
    }

    #[test]
    fn harte() {
        let Ok(entries) = std::fs::read_dir(dir()) else {
            eprintln!("harte: no tests; run tools/fetch-harte");
            return;
        };
        let mut files: Vec<PathBuf> = entries.map(|entry| entry.unwrap().path()).collect();
        files.sort();

        // The opcodes shared out among the host's cores.
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let files = &files;
        let mut failed: Vec<String> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|t| {
                    scope.spawn(move || {
                        let mine = files.iter().skip(t).step_by(threads);
                        mine.filter_map(|file| check_file(file)).collect::<Vec<_>>()
                    })
                })
                .collect();
            workers.into_iter().flat_map(|worker| worker.join().unwrap()).collect()
        });
        failed.sort();
        assert!(failed.is_empty(), "{} opcodes wrong:\n{}", failed.len(), failed.join("\n"));
    }
}
