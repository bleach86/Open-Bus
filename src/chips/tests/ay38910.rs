//! The AY-3-8910, by its bus as a CPU reaches it: its registers' bits; a
//! tone's frequency, clock / (16 x period); its DAC's levels, 3 dB a step;
//! its noise, a 17-bit shift register's longest sequence; and each of its
//! envelope's 16 shapes, as its data sheet draws them.

#[cfg(test)]
mod tests {
    use crate::chips::ay38910::{Ay38910, LEVELS};

    /// Register `reg` written, as a CPU does it: its number latched, then
    /// its byte.
    fn set(ay: &mut Ay38910, reg: u8, value: u8) {
        ay.bus(true, true, reg);
        ay.bus(true, false, value);
        ay.bus(false, false, 0);
    }

    fn get(ay: &mut Ay38910, reg: u8) -> Option<u8> {
        ay.bus(true, true, reg);
        let value = ay.bus(false, true, 0);
        ay.bus(false, false, 0);
        value
    }

    #[test]
    fn its_registers_hold_their_bits() {
        let mut ay = Ay38910::new();
        let bits = [
            0xFF, 0x0F, 0xFF, 0x0F, 0xFF, 0x0F, 0x1F, 0xFF, 0x1F, 0x1F, 0x1F, 0xFF, 0xFF, 0x0F,
            0xFF, 0xFF,
        ];
        for (reg, &mask) in bits.iter().enumerate() {
            set(&mut ay, reg as u8, 0xFF);
            assert_eq!(get(&mut ay, reg as u8), Some(mask), "register {reg}");
        }
        // A number past 15: none of its registers, written or read.
        set(&mut ay, 0x10, 0x00);
        assert_eq!(get(&mut ay, 0x10), None, "register $10 read");
        assert_eq!(get(&mut ay, 0), Some(0xFF), "register 0 written by $10");
    }

    #[test]
    fn a_tone_is_clock_over_16_times_its_period() {
        for period in [1u16, 100, 0xFFF] {
            let mut ay = Ay38910::new();
            set(&mut ay, 0, period as u8);
            set(&mut ay, 1, (period >> 8) as u8);
            set(&mut ay, 7, 0x3E); // tone A alone
            set(&mut ay, 8, 15);
            let mut rises = 0;
            let mut last = ay.output(0);
            for _ in 0..16 * period as u32 * 10 {
                ay.tick();
                let now = ay.output(0);
                if now > last {
                    rises += 1;
                }
                last = now;
            }
            assert_eq!(rises, 10, "period {period}: 10 cycles in 160 x the period");
        }
    }

    #[test]
    fn its_levels_are_3_db_apart() {
        assert_eq!(LEVELS[0], 0.0);
        assert_eq!(LEVELS[15], 1.0);
        for level in 1..15 {
            let ratio = LEVELS[level + 1] / LEVELS[level];
            assert!(
                (ratio - 2f32.sqrt()).abs() < 1e-4,
                "levels {level} to {}: {ratio}",
                level + 1
            );
        }
        // Tone and noise both off: the channel its level, steady.
        let mut ay = Ay38910::new();
        set(&mut ay, 7, 0x3F);
        for level in 0..16 {
            set(&mut ay, 9, level);
            assert_eq!(ay.output(1), LEVELS[level as usize], "channel B at {level}");
        }
    }

    #[test]
    fn its_noise_is_a_17_bit_shift_registers_longest_sequence() {
        let mut ay = Ay38910::new();
        set(&mut ay, 6, 1); // shifted every 16 cycles
        set(&mut ay, 7, 0x37); // noise A alone
        set(&mut ay, 8, 15);
        // A bit a shift, two turns of 2^17 - 1.
        const TURN: usize = (1 << 17) - 1;
        let bits: Vec<bool> = (0..2 * TURN)
            .map(|_| {
                for _ in 0..16 {
                    ay.tick();
                }
                ay.output(0) > 0.0
            })
            .collect();
        assert_eq!(bits[..TURN], bits[TURN..], "not repeating after 2^17 - 1");
        for shorter in [TURN / 7, TURN / 31, TURN / 127] {
            assert_ne!(
                bits[..shorter],
                bits[shorter..2 * shorter],
                "repeating after {shorter}"
            );
        }
        let ones = bits[..TURN].iter().filter(|&&bit| bit).count();
        assert_eq!(
            ones,
            1 << 16,
            "its 1s: half and a half, as the longest sequence's"
        );
    }

    #[test]
    fn each_envelope_shape_is_its_data_sheets() {
        let down: Vec<u8> = (0..16).rev().collect();
        let up: Vec<u8> = (0..16).collect();
        let then = |first: &[u8], rest: &[u8]| -> Vec<u8> {
            first
                .iter()
                .chain(rest.iter().cycle())
                .copied()
                .take(48)
                .collect()
        };
        for shape in 0..16u8 {
            let want = match shape {
                0x0..=0x3 | 0x9 => then(&down, &[0]),
                0x4..=0x7 | 0xF => then(&up, &[0]),
                0x8 => then(&down, &down),
                0xA => then(&down, &[up.clone(), down.clone()].concat()),
                0xB => then(&down, &[15]),
                0xC => then(&up, &up),
                0xD => then(&up, &[15]),
                _ => then(&up, &[down.clone(), up.clone()].concat()), // $E
            };
            let mut ay = Ay38910::new();
            set(&mut ay, 11, 1); // a step every 16 cycles
            set(&mut ay, 13, shape);
            // Step k comes 16k - 7 to 16k cycles after the shape's written,
            // the clock's eighths as they fall: so at 16k + 4, k steps.
            let mut got = Vec::new();
            let mut cycle = 0;
            for k in 0..48 {
                while cycle < 16 * k + 4 {
                    ay.tick();
                    cycle += 1;
                }
                got.push(ay.envelope());
            }
            assert_eq!(got, want, "shape ${shape:X}");
        }
        // A channel at the envelope's volume: register 8's bit 4.
        let mut ay = Ay38910::new();
        set(&mut ay, 7, 0x3F);
        set(&mut ay, 13, 0x0D); // up, then held at 15
        set(&mut ay, 8, 0x10);
        assert_eq!(ay.output(0), LEVELS[0], "the envelope's start");
        for _ in 0..16 * 20 {
            ay.tick();
        }
        assert_eq!(ay.output(0), LEVELS[15], "the envelope held at 15");
    }
}
