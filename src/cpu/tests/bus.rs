//! The CPU on another machine's bus: a test board, 64K of RAM with a
//! register at $D000 that counts the reads and writes it sees, a count of
//! the cycles it is ticked, and a RESET line. It runs roms/bus_test.s,
//! whose comments say what the 6502 does to the register. `make -C roms` builds the program;
//! without it each test says so and passes.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::cpu::CPU;
    use std::path::Path;

    const PORT: u16 = 0xD000;

    struct Board {
        ram: Vec<u8>,
        reads: u32,
        writes: u32,
        ticks: u32,
        reset: bool,
    }

    impl Bus for Board {
        fn read(&mut self, addr: u16) -> u8 {
            if addr == PORT {
                self.reads += 1;
            }
            self.ram[addr as usize]
        }

        fn write(&mut self, addr: u16, value: u8) {
            if addr == PORT {
                self.writes += 1;
            }
            self.ram[addr as usize] = value;
        }

        fn tick(&mut self) {
            self.ticks += 1;
        }

        fn reset_held(&self) -> bool {
            self.reset
        }
    }

    /// The program run to its end, on the board: the CPU, and where the end is.
    fn run() -> Option<(CPU<Board>, u16)> {
        let roms = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms");
        let (Ok(ram), Ok(labels)) = (
            std::fs::read(roms.join("bus_test.bin")),
            std::fs::read_to_string(roms.join("bus_test.lbl")),
        ) else {
            eprintln!("bus test: no program; run make -C roms");
            return None;
        };
        let line = labels.lines().find(|line| line.ends_with(" .done")).unwrap();
        let done = u16::from_str_radix(&line[3..9], 16).unwrap();

        let mut cpu = CPU::with_bus(Board { ram, reads: 0, writes: 0, ticks: 0, reset: false });
        cpu.reset();
        // LDA abs 4, LDX # 2, LDA abs,X crossing 5, INC abs 6, LDX # 2,
        // STA abs,X 5: to the end.
        cpu.execute(24);
        Some((cpu, done))
    }

    #[test]
    fn it_runs_on_another_machines_bus() {
        let Some((cpu, done)) = run() else { return };
        assert_eq!(cpu.pc, done, "didn't get to the end");
    }

    #[test]
    fn the_bus_is_ticked_every_cycle() {
        let Some((cpu, _)) = run() else { return };
        assert_eq!(cpu.memory.ticks, 7 + 24, "ticks: reset's 7 and the program's 24");
        assert_eq!(cpu.memory.ticks, cpu.clock, "ticks and the CPU's clock");
    }

    #[test]
    fn the_register_sees_the_6502s_reads_and_writes() {
        let Some((cpu, _)) = run() else { return };
        assert_eq!(cpu.memory.reads, 4, "reads: LDA, LDA abs,X, INC, STA abs,X's before the carry");
        assert_eq!(cpu.memory.writes, 2, "writes: INC's old value back, then its new");
    }

    #[test]
    fn reset_held_stops_the_cpu_and_let_go_resets_it() {
        let Some((mut cpu, done)) = run() else { return };
        let start = u16::from_le_bytes([cpu.memory.ram[0xFFFC], cpu.memory.ram[0xFFFD]]);
        let (a, x, sp) = (cpu.a, cpu.x, cpu.sp);
        cpu.status.interrupt_disable = false;

        cpu.memory.reset = true;
        let (clock, writes) = (cpu.clock, cpu.memory.writes);
        cpu.execute(100);
        assert_eq!(cpu.pc, done, "it ran, held");
        assert_eq!(cpu.clock - clock, 100, "its cycles, held");
        assert_eq!(cpu.memory.ticks, cpu.clock, "the bus not ticked, held");
        assert_eq!(cpu.memory.writes, writes, "the register written, held");

        cpu.memory.reset = false;
        cpu.execute(7);
        assert_eq!(cpu.clock - clock, 107, "reset's 7 cycles");
        assert_eq!(cpu.pc, start, "PC not from the reset vector");
        assert_eq!(cpu.sp, sp.wrapping_sub(3), "S not down 3");
        assert!(cpu.status.interrupt_disable, "I not set");
        assert_eq!((cpu.a, cpu.x), (a, x), "A and X not as they were");
    }

    #[test]
    fn a_jam_stops_the_cpu_until_reset() {
        let Some((mut cpu, done)) = run() else { return };
        let start = u16::from_le_bytes([cpu.memory.ram[0xFFFC], cpu.memory.ram[0xFFFD]]);
        cpu.memory.ram[done as usize] = 0x02; // a JAM where the program ends
        cpu.irq = true;
        cpu.nmi = true;
        cpu.execute(1_000);
        assert_eq!(cpu.pc, done.wrapping_add(1), "past the JAM");
        assert_eq!(cpu.memory.ticks, cpu.clock, "the bus not ticked, jammed");
        assert!(cpu.nmi, "an NMI taken, jammed");

        cpu.irq = false;
        cpu.nmi = false;
        cpu.memory.reset = true;
        cpu.execute(10);
        cpu.memory.reset = false;
        cpu.execute(7);
        assert_eq!(cpu.pc, start, "not reset out of the JAM");
        cpu.execute(4); // the program's first instruction, LDA PORT
        assert_eq!(cpu.pc, start.wrapping_add(3), "not running again");
    }
}
