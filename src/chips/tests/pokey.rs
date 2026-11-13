//! POKEY: held in reset by SKCTL; a pure tone's pitch at each of its clocks,
//! 8 bits and 16 joined; volume only, and as a state loaded has it; its
//! timer's IRQ; RANDOM; STIMER.

#[cfg(test)]
mod tests {
    use crate::chips::pokey::Pokey;

    const AUDF1: u8 = 0x0;
    const AUDC1: u8 = 0x1;
    const AUDF2: u8 = 0x2;
    const AUDC2: u8 = 0x3;
    const AUDCTL: u8 = 0x8;
    const STIMER: u8 = 0x9;
    const RANDOM: u8 = 0xA;
    const IRQEN: u8 = 0xE;
    const IRQST: u8 = 0xE;
    const SKCTL: u8 = 0xF;
    /// AUDC: a pure tone, at volume 8.
    const PURE_TONE: u8 = 0xA8;

    /// POKEY out of reset, `audctl` its AUDCTL, channel 1 a pure tone of
    /// AUDF `audf`.
    fn pokey(audctl: u8, audf: u8) -> Pokey {
        let mut pokey = Pokey::new();
        pokey.write(SKCTL, 0x03);
        pokey.write(AUDCTL, audctl);
        pokey.write(AUDF1, audf);
        pokey.write(AUDC1, PURE_TONE);
        pokey.write(STIMER, 0);
        pokey
    }

    /// The clocks between its output's changes, over `clocks` of them,
    /// once it's settled.
    fn half_period(pokey: &mut Pokey, clocks: u32) -> f64 {
        let mut last = pokey.output();
        let mut changes = Vec::new();
        for clock in 0..clocks {
            pokey.clock();
            let output = pokey.output();
            if output != last {
                changes.push(clock);
                last = output;
            }
        }
        let changes = &changes[2..];
        (changes[changes.len() - 1] - changes[0]) as f64 / (changes.len() - 1) as f64
    }

    #[test]
    fn held_in_reset_nothing_counts_and_random_stands_still() {
        let mut pokey = Pokey::new();
        pokey.write(AUDF1, 0);
        pokey.write(AUDC1, PURE_TONE);
        let (output, random) = (pokey.output(), pokey.read(RANDOM));
        for _ in 0..1000 {
            pokey.clock();
        }
        assert_eq!((pokey.output(), pokey.read(RANDOM)), (output, random));
        // Out of reset, the polynomial shifting: all 1s at first.
        pokey.write(SKCTL, 0x03);
        for _ in 0..20 {
            pokey.clock();
        }
        assert_ne!(pokey.read(RANDOM), random, "out of reset, still");
    }

    #[test]
    fn a_pure_tone_comes_round_as_audf_and_its_clock_say() {
        // 64 kHz: AUDF + 1 of its ticks, each 28 clocks.
        assert_eq!(half_period(&mut pokey(0x00, 9), 5000), 10.0 * 28.0);
        // 15 kHz: 114 clocks a tick.
        assert_eq!(half_period(&mut pokey(0x01, 9), 20000), 10.0 * 114.0);
        // 1.79 MHz: AUDF + 4 clocks.
        assert_eq!(half_period(&mut pokey(0x40, 9), 1000), 13.0);
    }

    #[test]
    fn two_channels_joined_count_16_bits() {
        // Channels 1 and 2 at 1.79 MHz, joined: AUDF1 + 256 * AUDF2 + 7
        // clocks, channel 2 sounding.
        let mut pokey = pokey(0x50, 0x34);
        pokey.write(AUDC1, 0x00);
        pokey.write(AUDF2, 0x02);
        pokey.write(AUDC2, PURE_TONE);
        pokey.write(STIMER, 0);
        assert_eq!(half_period(&mut pokey, 20000), (0x34 + 0x200 + 7) as f64);
    }

    #[test]
    fn volume_only_is_its_volume_steady() {
        let mut pokey = pokey(0x40, 0);
        pokey.write(AUDC1, 0x1B);
        for _ in 0..100 {
            pokey.clock();
            assert_eq!(pokey.output(), 11.0 / 44.0);
        }
    }

    #[test]
    fn a_state_loaded_sounds_as_it_did() {
        let mut pokey = pokey(0x40, 0);
        pokey.write(AUDC1, 0x1B);
        let state = postcard::to_stdvec(&pokey).unwrap();
        let mut loaded: Pokey = postcard::from_bytes(&state).unwrap();
        assert_eq!(loaded.output(), 11.0 / 44.0, "its volume, not yet changed");
        for _ in 0..1000 {
            pokey.clock();
            loaded.clock();
            assert_eq!(loaded.output(), pokey.output());
        }
    }

    #[test]
    fn timer_1_raises_its_irq_as_irqen_lets_it() {
        let mut pokey = pokey(0x40, 0);
        for _ in 0..10 {
            pokey.clock();
        }
        assert!(!pokey.irq(), "not let");
        pokey.write(IRQEN, 0x01);
        for _ in 0..10 {
            pokey.clock();
        }
        assert!(pokey.irq());
        assert_eq!(pokey.read(IRQST) & 0x01, 0, "IRQST's bit, low");
        pokey.write(IRQEN, 0x00);
        assert!(!pokey.irq(), "cleared, masked");
        assert_eq!(pokey.read(IRQST) & 0x01, 0x01);
    }
}
