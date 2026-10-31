//! The 6522: its timers' counts, to the cycle, as its data sheet times
//! them; their interrupt flags, set and cleared; the IRQ line by the
//! enable mask; its ports; and RESET.

#[cfg(test)]
mod tests {
    use crate::chips::via6522::Via6522;

    const T1C_L: u8 = 0x4;
    const T1C_H: u8 = 0x5;
    const T1L_H: u8 = 0x7;
    const T2C_L: u8 = 0x8;
    const T2C_H: u8 = 0x9;
    const ACR: u8 = 0xB;
    const IFR: u8 = 0xD;
    const IER: u8 = 0xE;
    const T1: u8 = 0x40;
    const T2: u8 = 0x20;

    #[test]
    fn timer_1_loads_the_cycle_after_and_counts_down() {
        let mut via = Via6522::new();
        via.write(T1C_L, 16);
        via.write(T1C_H, 0); // N = 16
        // The cycle after, N; then down a cycle at a time, to 0 at N + 1.
        for cycle in 1..=17 {
            via.tick();
            assert_eq!(via.read(T1C_H), 0, "cycle {cycle}: the high byte");
            assert_eq!(via.read(IFR) & T1, 0, "cycle {cycle}: timed out early");
            assert_eq!(via.read(T1C_L), 16 - (cycle - 1) as u8, "cycle {cycle}: the count");
        }
        // N + 2: $FFFF, and its flag set (N + 1.5 after the write).
        via.tick();
        assert_eq!(via.read(IFR) & T1, T1, "no time out at N + 2");
        assert_eq!(via.read(T1C_H), 0xFF);
        assert_eq!(via.read(T1C_L), 0xFF);
        assert_eq!(via.read(IFR) & T1, 0, "the flag not cleared by reading the low counter");
        // N + 3: the latch again; one-shot, no more time outs.
        via.tick();
        assert_eq!(via.read(T1C_L), 16, "not reloaded");
        for _ in 0..100 {
            via.tick();
        }
        assert_eq!(via.read(IFR) & T1, 0, "one-shot, timed out again");
    }

    #[test]
    fn timer_1_free_running_times_out_every_n_plus_2() {
        let mut via = Via6522::new();
        via.write(ACR, 0x40); // free running
        via.write(T1C_L, 100);
        via.write(T1C_H, 0);
        let mut outs = Vec::new();
        for cycle in 1..=1_000 {
            via.tick();
            if via.read(IFR) & T1 != 0 {
                outs.push(cycle);
                via.write(IFR, T1); // cleared by writing it
            }
        }
        let want: Vec<u32> = (1..=9).map(|n| n * 102).collect();
        assert_eq!(outs, want, "time outs, at N + 2 apart");
    }

    #[test]
    fn writing_timer_1s_high_latch_clears_its_flag_and_doesnt_load_it() {
        let mut via = Via6522::new();
        via.write(T1C_L, 2);
        via.write(T1C_H, 0);
        for _ in 0..5 {
            via.tick(); // to the time out, and its reload: 2
        }
        assert_eq!(via.read(IFR) & T1, T1);
        via.write(T1L_H, 0x12);
        assert_eq!(via.read(IFR) & T1, 0, "the flag not cleared");
        via.tick();
        assert_eq!((via.read(T1C_H), via.read(T1C_L)), (0x00, 0x01), "the counter loaded by the latch's write");
        for _ in 0..3 {
            via.tick(); // 0, $FFFF, then the latch
        }
        assert_eq!((via.read(T1C_H), via.read(T1C_L)), (0x12, 0x02), "the new latch not taken at the reload");
    }

    #[test]
    fn timer_2_times_out_once_then_counts_on_down() {
        let mut via = Via6522::new();
        via.write(T2C_L, 10);
        via.write(T2C_H, 0);
        for cycle in 1..=11 {
            via.tick();
            assert_eq!(via.read(IFR) & T2, 0, "cycle {cycle}: timed out early");
            assert_eq!(via.read(T2C_L), 10 - (cycle - 1) as u8, "cycle {cycle}");
        }
        via.tick();
        assert_eq!(via.read(IFR) & T2, T2, "no time out at N + 2");
        assert_eq!(via.read(T2C_L), 0xFF);
        assert_eq!(via.read(IFR) & T2, 0, "the flag not cleared by reading the low counter");
        via.tick();
        assert_eq!((via.read(T2C_H), via.read(T2C_L)), (0xFF, 0xFE), "not on down from $FFFF");
        for _ in 0..0x10000 {
            via.tick();
        }
        assert_eq!(via.read(IFR) & T2, 0, "timed out again, not loaded");
    }

    #[test]
    fn the_irq_line_is_a_flag_set_and_enabled() {
        let mut via = Via6522::new();
        via.write(T1C_L, 0);
        via.write(T1C_H, 0);
        via.tick();
        via.tick();
        assert_eq!(via.read(IFR) & T1, T1);
        assert!(!via.irq(), "IRQ, the flag not enabled");
        assert_eq!(via.read(IFR) & 0x80, 0, "IFR bit 7, nothing enabled");
        via.write(IER, 0x80 | T1);
        assert!(via.irq(), "no IRQ, the flag enabled");
        assert_eq!(via.read(IFR) & 0x80, 0x80, "IFR bit 7 clear, IRQ held");
        assert_eq!(via.read(IER), 0x80 | T1, "the IER, read: bit 7 set");
        via.write(IER, T1); // bit 7 clear: disabled
        assert!(!via.irq(), "IRQ, disabled");
        assert_eq!(via.read(IFR) & T1, T1, "the flag cleared by disabling it");
        via.write(IER, 0x80 | T1);
        via.read(T1C_L);
        assert!(!via.irq(), "IRQ, the flag cleared");
    }

    #[test]
    fn the_ports_and_reset() {
        let mut via = Via6522::new();
        via.write(0x3, 0x0F); // port A: low nibble out
        via.write(0x1, 0xA5);
        via.pa_in = 0x3C;
        assert_eq!(via.read(0x1), 0x35, "port A: outputs written, inputs driven");
        assert_eq!(via.port_a(), 0x35);
        via.write(0x2, 0xFF);
        via.write(0x0, 0x5A);
        assert_eq!(via.read(0x0), 0x5A, "port B");
        via.write(T1C_L, 50);
        via.write(T1C_H, 0);
        via.write(IER, 0x80 | T1);
        via.tick();
        via.reset();
        assert_eq!((via.read(0x2), via.read(0x3), via.read(0xE)), (0, 0, 0x80), "not cleared by RESET");
        assert_eq!(via.read(T1C_L), 50, "timer 1 cleared by RESET");
    }
}
