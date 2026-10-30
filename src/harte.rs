//! Tom Harte's 6502 SingleStepTests against the CPU: for each opcode, 10,000
//! cases of the registers and memory before and after one instruction, and
//! every bus cycle it takes. `tools/fetch-harte` fetches them (MIT, about
//! 640 MB, kept out of the repository) into target/harte/; without them
//! this test says so and passes. Faster with `--release`.
//!
//! Each case is checked three ways: the state after (registers, memory),
//! the number of cycles, and the bus cycles themselves, every read and
//! write in order, dummy ones included, as a real 6502 makes them.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::ram::FlatRam;
    use serde_json::Value;
    use std::fmt::Write;
    use std::path::Path;

    #[derive(Default)]
    struct Tally {
        cases: usize,
        state: usize,
        cycles: usize,
        bus: usize,
        first_state: Option<String>,
        first_cycles: Option<String>,
        first_bus: Option<String>,
    }

    fn number(value: &Value) -> u64 {
        value.as_u64().unwrap()
    }

    fn run_case(cpu: &mut CPU, case: &Value, tally: &mut Tally) {
        let (initial, expected) = (&case["initial"], &case["final"]);
        let flat = cpu.memory.flat.as_mut().unwrap();
        for cell in initial["ram"].as_array().unwrap() {
            flat.ram[number(&cell[0]) as usize] = number(&cell[1]) as u8;
        }
        flat.bus.clear();
        cpu.pc = number(&initial["pc"]) as u16;
        cpu.sp = number(&initial["s"]) as u8;
        cpu.a = number(&initial["a"]) as u8;
        cpu.x = number(&initial["x"]) as u8;
        cpu.y = number(&initial["y"]) as u8;
        cpu.status.from_byte(number(&initial["p"]) as u8);

        let budget = 1000u32;
        let mut cycles = budget;
        let opcode = cpu.fetch_byte(&mut cycles);
        cpu.exec_opcode(opcode, &mut cycles);
        let used = (budget - cycles) as usize;

        let name = case["name"].as_str().unwrap();
        // P: bits 4 (B) and 5 aren't flags a 6502 has; what they are when
        // pushed is checked in the memory written.
        let p = |byte: u64| byte as u8 | 0x30;
        let got = (cpu.pc as u64, cpu.sp as u64, cpu.a as u64, cpu.x as u64, cpu.y as u64, p(cpu.status.to_byte() as u64));
        let want = (
            number(&expected["pc"]),
            number(&expected["s"]),
            number(&expected["a"]),
            number(&expected["x"]),
            number(&expected["y"]),
            p(number(&expected["p"])),
        );
        let flat = cpu.memory.flat.as_ref().unwrap();
        let mut wrong = String::new();
        if got != want {
            let _ = write!(wrong, "pc s a x y p {got:02X?}, want {want:02X?}; ");
        }
        for cell in expected["ram"].as_array().unwrap() {
            let (addr, value) = (number(&cell[0]) as usize, number(&cell[1]) as u8);
            if flat.ram[addr] != value {
                let _ = write!(wrong, "${addr:04X} ${:02X}, want ${value:02X}; ", flat.ram[addr]);
            }
        }
        tally.cases += 1;
        if !wrong.is_empty() {
            tally.state += 1;
            tally.first_state.get_or_insert_with(|| format!("{name}: {wrong}"));
        }

        let bus: Vec<(u16, u8, bool)> = case["cycles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (number(&c[0]) as u16, number(&c[1]) as u8, c[2].as_str() == Some("write")))
            .collect();
        if used != bus.len() {
            tally.cycles += 1;
            tally.first_cycles.get_or_insert_with(|| format!("{name}: {used} cycles, want {}", bus.len()));
        }
        if flat.bus != bus {
            tally.bus += 1;
            let show = |b: &[(u16, u8, bool)]| {
                b.iter().map(|(a, v, w)| format!("{}{a:04X}={v:02X}", if *w { "W" } else { "R" })).collect::<Vec<_>>().join(" ")
            };
            tally.first_bus.get_or_insert_with(|| format!("{name}:\n      got  {}\n      want {}", show(&flat.bus), show(&bus)));
        }

        // Back to zeros, for the next case.
        let touched: Vec<u16> = flat.bus.iter().map(|&(a, _, _)| a).collect();
        let flat = cpu.memory.flat.as_mut().unwrap();
        for addr in touched {
            flat.ram[addr as usize] = 0;
        }
        for cell in initial["ram"].as_array().unwrap() {
            flat.ram[number(&cell[0]) as usize] = 0;
        }
    }

    #[test]
    fn harte_single_step_tests() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/harte");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("harte: no tests; run tools/fetch-harte");
            return;
        };
        let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
        files.sort();

        let mut cpu = CPU::new();
        cpu.memory.flat = Some(FlatRam { ram: Box::new([0; 0x10000]), bus: Vec::new() });
        let mut report = String::new();
        let (mut opcodes, mut state_bad, mut cycles_bad, mut bus_bad) = (0, 0, 0, 0);
        for file in &files {
            let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
            let mut tally = Tally::default();
            for case in &cases {
                run_case(&mut cpu, case, &mut tally);
            }
            opcodes += 1;
            let op = file.file_stem().unwrap().to_string_lossy().to_string();
            if tally.state + tally.cycles + tally.bus > 0 {
                let _ = writeln!(report, "${op}: state {}, cycles {}, bus {} of {}", tally.state, tally.cycles, tally.bus, tally.cases);
                for first in [&tally.first_state, &tally.first_cycles, &tally.first_bus].into_iter().flatten() {
                    let _ = writeln!(report, "    {first}");
                }
            }
            state_bad += usize::from(tally.state > 0);
            cycles_bad += usize::from(tally.cycles > 0);
            bus_bad += usize::from(tally.bus > 0);
        }
        eprintln!(
            "harte: {opcodes} opcodes; wrong state in {state_bad}, wrong cycle counts in {cycles_bad}, bus cycles differing in {bus_bad}\n{report}"
        );
        assert!(state_bad == 0 && cycles_bad == 0 && bus_bad == 0, "see above");
    }
}
