//! The CPU on another machine's bus: a test board, 64K of RAM with a
//! register at $D000 that counts the reads and writes it sees, and a count
//! of the cycles it is ticked. It runs roms/bus_test.s, whose comments say
//! what the 6502 does to the register. `make -C roms` builds the program;
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

        let mut cpu = CPU::with_bus(Board { ram, reads: 0, writes: 0, ticks: 0 });
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
}
