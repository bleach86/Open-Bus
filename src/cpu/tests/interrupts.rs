//! The CPU's interrupts: IRQ and NMI raised on its pins (`cpu.irq`,
//! `cpu.nmi`), as a chip tester would, while it runs roms/interrupt_test.s
//! an instruction at a time (`execute(1)`: one instruction, and the
//! interrupt, if one is taken after it). `make -C roms` builds the program;
//! without it each test says so and passes.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::cpu::CPU;
    use std::path::Path;

    const IRQS: u16 = 0x00;
    const NMIS: u16 = 0x01;

    /// The CPU, the program loaded and reset; and the program's labels.
    fn machine() -> Option<(CPU, String)> {
        let roms = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms");
        let (Ok(image), Ok(labels)) = (
            std::fs::read(roms.join("interrupt_test.bin")),
            std::fs::read_to_string(roms.join("interrupt_test.lbl")),
        ) else {
            eprintln!("interrupt test: no program; run make -C roms");
            return None;
        };
        let mut cpu = CPU::new();
        cpu.memory.data.copy_from_slice(&image);
        cpu.reset();
        Some((cpu, labels))
    }

    fn label(labels: &str, name: &str) -> u16 {
        let line = labels.lines().find(|line| line.ends_with(&format!(" .{name}"))).unwrap();
        u16::from_str_radix(&line[3..9], 16).unwrap()
    }

    /// What the interrupt pushed: P, and the address it goes back to.
    fn pushed(cpu: &mut CPU) -> (u8, u16) {
        let sp = cpu.sp as u16;
        let p = cpu.memory.read(0x0101 + sp);
        let low = cpu.memory.read(0x0102 + sp) as u16;
        let high = cpu.memory.read(0x0103 + sp) as u16;
        (p, (high << 8) | low)
    }

    #[test]
    fn reset_is_the_6502s_seven_cycles() {
        let mut cpu = CPU::new();
        cpu.memory.data[0xFFFC..0xFFFE].copy_from_slice(&[0x34, 0x12]);
        cpu.memory.bus_log = Some(Vec::new());
        cpu.reset();

        assert_eq!(cpu.clock, 7, "cycles");
        assert_eq!(cpu.pc, 0x1234, "PC from the reset vector");
        assert_eq!(cpu.sp, 0xFD, "S from power-on's 0, down 3");
        assert!(cpu.status.interrupt_disable, "I not set");
        // As an interrupt, its pushes reads: PC twice, the stack three times
        // going down, then the vector.
        let reads: Vec<(u16, bool)> = cpu.memory.bus_log.unwrap().iter().map(|&(a, _, w)| (a, w)).collect();
        let want = [0x0000, 0x0000, 0x0100, 0x01FF, 0x01FE, 0xFFFC, 0xFFFD];
        assert_eq!(reads, want.map(|a| (a, false)).to_vec(), "bus cycles: address, write");
    }

    #[test]
    fn irq_waits_while_reset_has_i_set() {
        let Some((mut cpu, labels)) = machine() else { return };
        cpu.pc = label(&labels, "loop");
        cpu.irq = true;
        cpu.execute(100);

        assert_eq!(cpu.memory.read(IRQS), 0, "IRQ taken with I set");
    }

    #[test]
    fn cli_lets_irq_in_after_the_next_instruction() {
        let Some((mut cpu, labels)) = machine() else { return };
        let start = label(&labels, "cli_then");
        cpu.pc = start;
        cpu.irq = true;
        let sp = cpu.sp;

        cpu.execute(1); // CLI
        assert_eq!(cpu.pc, start + 1, "IRQ taken straight after CLI");
        cpu.execute(1); // NOP, then the IRQ
        assert_eq!(cpu.pc, label(&labels, "irq"), "IRQ not taken");

        assert!(cpu.status.interrupt_disable, "I not set by the IRQ");
        assert_eq!(cpu.sp, sp.wrapping_sub(3), "pushed: PC and P");
        let (p, back) = pushed(&mut cpu);
        assert_eq!(back, start + 2, "pushed PC");
        assert_eq!(p & 0x30, 0x20, "pushed P: B set, or bit 5 clear");
        assert_eq!(p & 0x04, 0, "pushed P: I set");
    }

    #[test]
    fn rti_goes_back_to_the_interrupted_instruction() {
        let Some((mut cpu, labels)) = machine() else { return };
        let start = label(&labels, "cli_then");
        cpu.pc = start;
        cpu.irq = true;
        let sp = cpu.sp;
        cpu.execute(1); // CLI
        cpu.execute(1); // NOP, then the IRQ
        cpu.irq = false;

        cpu.execute(1); // INC irqs
        cpu.execute(1); // RTI
        assert_eq!(cpu.pc, start + 2);
        assert!(!cpu.status.interrupt_disable, "I not back as it was");
        assert_eq!(cpu.sp, sp, "S not back as it was");
        assert_eq!(cpu.memory.read(IRQS), 1);
    }

    #[test]
    fn irq_comes_again_while_it_is_held() {
        let Some((mut cpu, labels)) = machine() else { return };
        cpu.pc = label(&labels, "cli_then");
        cpu.irq = true;
        cpu.execute(1); // CLI
        cpu.execute(1); // NOP, then the IRQ

        cpu.execute(1); // INC irqs
        cpu.execute(1); // RTI, then the IRQ again
        assert_eq!(cpu.pc, label(&labels, "irq"), "held IRQ not taken again");
        assert_eq!(cpu.memory.read(IRQS), 1);
    }

    #[test]
    fn sei_still_lets_a_raised_irq_in() {
        let Some((mut cpu, labels)) = machine() else { return };
        let start = label(&labels, "sei_then");
        cpu.pc = start;
        cpu.execute(1); // CLI
        cpu.execute(1); // NOP
        cpu.irq = true;

        cpu.execute(1); // SEI, then the IRQ
        assert_eq!(cpu.pc, label(&labels, "irq"), "IRQ shut out by SEI at once");
        let (p, back) = pushed(&mut cpu);
        assert_eq!(back, start + 3, "pushed PC");
        assert_eq!(p & 0x04, 0x04, "pushed P: I not as SEI left it");
    }

    #[test]
    fn irq_is_taken_after_a_taken_branch() {
        let Some((mut cpu, labels)) = machine() else { return };
        cpu.pc = label(&labels, "branch_then");
        cpu.execute(1); // CLI
        cpu.execute(1); // NOP
        cpu.execute(1); // CLC
        cpu.irq = true;

        cpu.execute(1); // BCC, then the IRQ
        assert_eq!(cpu.pc, label(&labels, "irq"), "IRQ not taken after the branch");
        let (_, back) = pushed(&mut cpu);
        assert_eq!(back, label(&labels, "taken"), "pushed PC: not the branch's target");
    }

    #[test]
    fn nmi_is_taken_with_i_set_and_once() {
        let Some((mut cpu, labels)) = machine() else { return };
        let start = label(&labels, "loop");
        cpu.pc = start;
        cpu.nmi = true;

        cpu.execute(1); // NOP, then the NMI
        assert_eq!(cpu.pc, label(&labels, "nmi"), "NMI not taken");
        let (p, back) = pushed(&mut cpu);
        assert_eq!(back, start + 1, "pushed PC");
        assert_eq!(p & 0x30, 0x20, "pushed P: B set, or bit 5 clear");

        cpu.execute(100);
        assert_eq!(cpu.memory.read(NMIS), 1, "NMI not taken once");
    }

    #[test]
    fn nmi_comes_before_irq() {
        let Some((mut cpu, labels)) = machine() else { return };
        cpu.pc = label(&labels, "cli_then");
        cpu.execute(1); // CLI
        cpu.execute(1); // NOP
        cpu.irq = true;
        cpu.nmi = true;

        cpu.execute(1); // NOP, then the NMI
        assert_eq!(cpu.pc, label(&labels, "nmi"), "IRQ before NMI");
        cpu.execute(1); // INC nmis
        cpu.execute(1); // RTI, then the IRQ, waiting with I clear again
        assert_eq!(cpu.pc, label(&labels, "irq"), "IRQ lost");
    }
}
