//! The WDC 65C02's WAI and STP, which Harte's tests and Dormann's leave
//! out: WAI waiting, its clock going and nothing on its bus, until an IRQ
//! or NMI; the IRQ taken if I is clear, and if not, on after the WAI; STP
//! stopped, IRQ and NMI or not, until reset. The CPU runs roms/
//! wai_stp_test.s (`make -C roms` builds it; without it each test says so
//! and passes).

#[cfg(test)]
mod tests {
    use crate::cpu::{CPU, Variant};
    use std::path::Path;

    const IRQS: usize = 0x00;
    const NMIS: usize = 0x01;

    /// A WDC 65C02, the program loaded and reset; and the program's labels.
    fn machine() -> Option<(CPU, String)> {
        let roms = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms");
        let (Ok(image), Ok(labels)) = (
            std::fs::read(roms.join("wai_stp_test.bin")),
            std::fs::read_to_string(roms.join("wai_stp_test.lbl")),
        ) else {
            eprintln!("WAI and STP test: no program; run make -C roms");
            return None;
        };
        let mut cpu = CPU::new();
        cpu.variant = Variant::Wdc65C02;
        cpu.memory.data.copy_from_slice(&image);
        cpu.reset();
        Some((cpu, labels))
    }

    fn label(labels: &str, name: &str) -> u16 {
        let line = labels
            .lines()
            .find(|line| line.ends_with(&format!(" .{name}")))
            .unwrap();
        u16::from_str_radix(&line[3..9], 16).unwrap()
    }

    /// `cycles` cycles run, waiting or stopped: its clock going, nothing on
    /// its bus, nowhere further.
    fn still(cpu: &mut CPU, cycles: u32, at: u16) {
        cpu.memory.bus_log = Some(Vec::new());
        let clock = cpu.clock;
        cpu.execute(cycles);
        assert_eq!(cpu.clock - clock, cycles, "its clock not going");
        assert_eq!(
            cpu.memory.bus_log.take().unwrap(),
            vec![],
            "on its bus, waiting or stopped"
        );
        assert_eq!(cpu.pc, at, "it went on");
    }

    #[test]
    fn wai_waits_for_an_irq_then_takes_it() {
        let Some((mut cpu, labels)) = machine() else {
            return;
        };
        cpu.pc = label(&labels, "wai_irq");
        let clock = cpu.clock;
        cpu.execute(1); // CLI
        cpu.execute(1); // WAI
        assert_eq!(cpu.clock - clock, 2 + 3, "CLI's 2 cycles and WAI's 3");
        let after = label(&labels, "after_irq");
        still(&mut cpu, 100, after);

        cpu.irq = true;
        cpu.execute(1);
        assert_eq!(cpu.pc, label(&labels, "irq"), "the IRQ not taken");
        cpu.irq = false;
        while cpu.pc != after {
            cpu.execute(1);
        }
        assert_eq!(cpu.memory.data[IRQS], 1, "the handler not run");
    }

    #[test]
    fn wai_with_i_set_goes_on_after_an_irq_and_takes_an_nmi() {
        let Some((mut cpu, labels)) = machine() else {
            return;
        };
        cpu.pc = label(&labels, "wai_masked");
        cpu.execute(1); // SEI
        cpu.execute(1); // WAI
        let after = label(&labels, "after_masked");
        still(&mut cpu, 100, after);
        cpu.irq = true;
        cpu.execute(1); // woken, in no time, and on: the NOP after the WAI
        assert_eq!(cpu.pc, after + 1, "not on after the WAI");
        assert_eq!(cpu.memory.data[IRQS], 0, "the IRQ taken, I set");
        cpu.irq = false;

        // An NMI, I set or not.
        cpu.pc = label(&labels, "wai_masked");
        cpu.execute(1);
        cpu.execute(1);
        still(&mut cpu, 10, after);
        cpu.nmi = true;
        cpu.execute(1);
        assert_eq!(cpu.pc, label(&labels, "nmi"), "the NMI not taken");
    }

    #[test]
    fn stp_stops_until_reset() {
        let Some((mut cpu, labels)) = machine() else {
            return;
        };
        cpu.pc = label(&labels, "stop");
        cpu.execute(1); // STP
        let after = label(&labels, "after_stop");
        still(&mut cpu, 100, after);
        cpu.irq = true;
        cpu.nmi = true;
        still(&mut cpu, 100, after);
        assert_eq!(
            (cpu.memory.data[IRQS], cpu.memory.data[NMIS]),
            (0, 0),
            "an interrupt taken, stopped"
        );
        cpu.irq = false;
        cpu.nmi = false;

        cpu.reset();
        assert_eq!(cpu.pc, label(&labels, "loop"), "not from the reset vector");
        cpu.execute(10);
        assert_ne!(cpu.clock, 0);
        let pc = cpu.pc;
        cpu.execute(1);
        assert_ne!(cpu.pc, pc, "not running again after reset");
    }
}
