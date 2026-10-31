//! The Mockingboard, in slot 4: its two 6522s at $C400 and $C480; an AY
//! reached through its 6522's ports, as software does it; and the CPU
//! running roms/mockingboard_test.s (`make -C roms` builds it; without it
//! those tests say so and pass): finding the card, writing a tone, taking
//! its timer's interrupts every 1,002 cycles, and the tone heard, on the
//! left; and a second card, in slot 5, its own, on the IRQ line too.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::cpu::CPU;
    use crate::host::display::{Display, Key};
    use crate::machines::apple2::mockingboard::Mockingboard;
    use crate::machines::apple2::{Apple2, CLOCK_HZ, CYCLES_PER_FRAME};
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;

    /// An Apple with the card, and a ROM of NOPs.
    fn machine() -> Apple2 {
        let mut apple = Apple2::new(vec![0xEA; 0x3000]);
        apple.mockingboards[0] = Some(Mockingboard::new());
        apple
    }

    #[test]
    fn its_6522s_answer_slot_4() {
        let mut apple = machine();
        apple.write(0xC402, 0x12); // the first's DDRB
        apple.write(0xC482, 0x34); // the second's
        assert_eq!(apple.read(0xC402), 0x12, "the first 6522");
        assert_eq!(apple.read(0xC482), 0x34, "the second 6522");
        assert_eq!(apple.read(0xC472), 0x12, "the first's registers, over again");
        assert_eq!(apple.read(0xC4F2), 0x34, "the second's");

        let mut without = Apple2::new(vec![0xEA; 0x3000]);
        without.write(0xC402, 0x12);
        // No card: the floating bus, as an empty slot's I/O is, the same cycle.
        assert_eq!(without.read(0xC402), without.read(0xC0F0), "no card: not the floating bus");
    }

    /// The first AY's register `reg` written, as software does it: the
    /// 6522's ports out, the number latched, then the byte written.
    fn ay_write(apple: &mut Apple2, reg: u8, value: u8) {
        apple.write(0xC402, 0x07); // DDRB: BC1, BDIR, /RESET out
        apple.write(0xC403, 0xFF); // DDRA: the bus out
        for (bus, control) in [(reg, 0x07), (reg, 0x04), (value, 0x06), (value, 0x04)] {
            apple.write(0xC401, bus);
            apple.write(0xC400, control);
        }
    }

    /// Read back: the number latched, the bus in, BC1 alone.
    fn ay_read(apple: &mut Apple2, reg: u8) -> u8 {
        apple.write(0xC403, 0xFF);
        apple.write(0xC401, reg);
        apple.write(0xC400, 0x07);
        apple.write(0xC400, 0x04);
        apple.write(0xC403, 0x00);
        apple.write(0xC400, 0x05);
        let value = apple.read(0xC401);
        apple.write(0xC400, 0x04);
        value
    }

    #[test]
    fn an_ay_through_its_6522s_ports() {
        let mut apple = machine();
        ay_write(&mut apple, 0, 0x23);
        ay_write(&mut apple, 1, 0xF1);
        ay_write(&mut apple, 8, 15);
        let card = apple.mockingboards[0].as_ref().unwrap();
        assert_eq!((card.ays[0].regs[0], card.ays[0].regs[1], card.ays[0].regs[8]), (0x23, 0x01, 15));
        assert_eq!(card.ays[1].regs[0], 0, "the second AY written");
        assert_eq!(ay_read(&mut apple, 0), 0x23, "register 0 read back");
        assert_eq!(ay_read(&mut apple, 1), 0x01, "register 1 read back, its 4 bits");
        // /RESET low: the AY's registers cleared.
        apple.write(0xC400, 0x00);
        assert_eq!(apple.mockingboards[0].as_ref().unwrap().ays[0].regs[0], 0, "not reset");
    }

    #[test]
    fn a_second_card_in_slot_5_is_its_own() {
        let mut apple = machine();
        apple.mockingboards[1] = Some(Mockingboard::new());
        apple.write(0xC402, 0x12);
        apple.write(0xC502, 0x34);
        assert_eq!((apple.read(0xC402), apple.read(0xC502)), (0x12, 0x34), "slots 4 and 5 not each their own");

        // Its timer's interrupt, on the IRQ line.
        apple.write(0xC504, 0);
        apple.write(0xC505, 0);
        apple.write(0xC50E, 0xC0);
        assert!(!apple.irq(), "IRQ before the time out");
        apple.tick();
        apple.tick();
        assert!(apple.irq(), "slot 5's interrupt not on the IRQ line");
        apple.read(0xC504);
        assert!(!apple.irq(), "IRQ still, its flag cleared");

        // Its second AY, at $C580, on the right: a level, steady.
        apple.write(0xC582, 0x07);
        apple.write(0xC583, 0xFF);
        for (bus, control) in [(7, 0x07), (7, 0x04), (0x3F, 0x06), (0x3F, 0x04), (8, 0x07), (8, 0x04), (15, 0x06), (15, 0x04)] {
            apple.write(0xC581, bus);
            apple.write(0xC580, control);
        }
        let card = apple.mockingboards[1].as_ref().unwrap();
        assert_eq!(card.output(), (0.0, 1.0), "the second AY's voice not on the right alone");
    }

    /// The test's program on an Apple with the card; None without it.
    fn program() -> Option<(CPU<Apple2>, String)> {
        let roms = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms");
        let (Ok(rom), Ok(labels)) = (
            std::fs::read(roms.join("mockingboard_test.bin")),
            std::fs::read_to_string(roms.join("mockingboard_test.lbl")),
        ) else {
            eprintln!("mockingboard test: no program; run make -C roms");
            return None;
        };
        let mut apple = Apple2::new(rom);
        apple.mockingboards[0] = Some(Mockingboard::new());
        let mut cpu = CPU::with_bus(apple);
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();
        Some((cpu, labels))
    }

    fn label(labels: &str, name: &str) -> u16 {
        let line = labels.lines().find(|line| line.ends_with(&format!(" .{name}"))).unwrap();
        u16::from_str_radix(&line[3..9], 16).unwrap()
    }

    #[test]
    fn the_cpu_finds_the_card_and_takes_its_interrupts() {
        let Some((mut cpu, labels)) = program() else { return };
        let looping = label(&labels, "loop");
        while cpu.pc != looping {
            cpu.execute(1);
        }
        assert_eq!(cpu.memory.ram[0x02], 6, "timer 1 not counted down 6 between two reads, 6 cycles apart");
        let ay = &cpu.memory.mockingboards[0].as_ref().unwrap().ays[0];
        assert_eq!((ay.regs[0], ay.regs[1], ay.regs[7], ay.regs[8]), (0x23, 0x01, 0x3E, 15), "the tone not written");

        // Each interrupt counted: when, an instruction at a time.
        let count = |cpu: &CPU<Apple2>| cpu.memory.ram[0x00] as u32 | (cpu.memory.ram[0x01] as u32) << 8;
        let mut at = Vec::new();
        let until = cpu.clock + CLOCK_HZ;
        while at.len() < 101 && cpu.clock < until {
            let before = count(&cpu);
            cpu.execute(1);
            if count(&cpu) != before {
                at.push(cpu.clock);
            }
        }
        assert_eq!(at.len(), 101, "not 101 interrupts in a second");
        // Every 1,002 cycles: each taken after the JMP it comes in, 0 to 2
        // cycles late, so 100 of them within 2 of 100,200.
        let span = at[100] - at[0];
        assert!(span.abs_diff(100 * 1_002) <= 2, "100 interrupts in {span} cycles, not 100,200");
        for pair in at.windows(2) {
            assert!((pair[1] - pair[0]).abs_diff(1_002) <= 2, "interrupts {} cycles apart", pair[1] - pair[0]);
        }
    }

    /// A window of the test's: the sound played, kept.
    struct Heard(Rc<RefCell<Vec<f32>>>);

    impl Display for Heard {
        fn show(&mut self, _: &[u32], _: usize, _: usize) {}

        fn key(&mut self) -> Option<Key> {
            None
        }

        fn play(&mut self, samples: &[f32]) {
            self.0.borrow_mut().extend_from_slice(samples);
        }
    }

    #[test]
    fn its_tone_is_heard() {
        let Some((mut cpu, _)) = program() else { return };
        let heard = Rc::new(RefCell::new(Vec::new()));
        cpu.memory.display = Some(Box::new(Heard(heard.clone())));
        cpu.execute(60 * CYCLES_PER_FRAME);
        // In stereo: the first AY's tone on the left, nothing on the right.
        let (left, right): (Vec<f32>, Vec<f32>) = heard.take().chunks(2).map(|frame| (frame[0], frame[1])).unzip();
        assert!(right[800..].iter().all(|sample| sample.abs() < 0.001), "the first AY heard on the right");
        // The tone, period $123: clock / (16 x 291), its level crossing the
        // middle twice a cycle (after the first frame's settling).
        let settled = &left[800..];
        let crossings = settled.windows(2).filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0)).count();
        let seconds = settled.len() as f32 / 48_000.0;
        let hz = crossings as f32 / 2.0 / seconds;
        let want = CLOCK_HZ as f32 / (16.0 * 0x123 as f32);
        assert!((hz - want).abs() < want * 0.01, "{hz} Hz, not {want}");
        assert!(settled.iter().any(|&sample| sample > 0.05), "too quiet");
    }
}
