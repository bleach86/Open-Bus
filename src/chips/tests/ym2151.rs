//! The YM2151: its sound sample for sample as ymfm's, through every
//! algorithm, feedback, detune, key scaling, the LFO's four waveforms on
//! AM and PM, noise and release; its timers, their status and IRQ, CSM;
//! and its busy flag, a write while it's set lost.

#[cfg(test)]
mod tests {
    use crate::chips::ym2151::{CLOCKS_A_SAMPLE, Ym2151};
    use crate::host::zip::crc32;

    /// A sample's worth of its clocks.
    fn sample(ym: &mut Ym2151) -> (i16, i16) {
        for _ in 0..CLOCKS_A_SAMPLE {
            ym.tick();
        }
        ym.output()
    }

    /// A register written, and a sample: its busy time.
    fn write(ym: &mut Ym2151, samples: &mut Vec<(i16, i16)>, reg: u8, value: u8) {
        ym.write_address(reg);
        ym.write_data(value);
        samples.push(sample(ym));
    }

    #[test]
    fn its_sound_is_ymfms_sample_for_sample() {
        let mut ym = Ym2151::new();
        let mut samples = Vec::new();
        let w = |ym: &mut Ym2151, samples: &mut Vec<(i16, i16)>, reg: usize, value: usize| {
            write(ym, samples, reg as u8, value as u8)
        };
        // Each channel an algorithm and feedback of its own, and the rest
        // of its registers varied.
        for ch in 0..8 {
            w(
                &mut ym,
                &mut samples,
                0x20 + ch,
                0xC0 | ((ch * 3) & 7) << 3 | ch,
            );
            w(&mut ym, &mut samples, 0x28 + ch, (0x3A + ch * 0x11) & 0x7F);
            w(&mut ym, &mut samples, 0x30 + ch, (ch * 37) & 0xFC);
            w(&mut ym, &mut samples, 0x38 + ch, (ch % 4) << 4 | (ch % 4));
            for (n, op) in [ch, ch + 8, ch + 16, ch + 24].into_iter().enumerate() {
                w(
                    &mut ym,
                    &mut samples,
                    0x40 + op,
                    ((n * 2 + ch) & 7) << 4 | ((ch + n) & 15),
                );
                w(
                    &mut ym,
                    &mut samples,
                    0x60 + op,
                    [0x18, 0x20, 0x10, 0x00][n] + ch,
                );
                w(
                    &mut ym,
                    &mut samples,
                    0x80 + op,
                    (n & 3) << 6 | (31 - ch - n),
                );
                w(
                    &mut ym,
                    &mut samples,
                    0xA0 + op,
                    if n == 1 { 0x80 } else { 0 } | (8 + ch),
                );
                w(&mut ym, &mut samples, 0xC0 + op, (n & 3) << 6 | (4 + n));
                w(
                    &mut ym,
                    &mut samples,
                    0xE0 + op,
                    ((ch + n) & 15) << 4 | (5 + ch % 8),
                );
            }
        }
        // The LFO: its rate, AM depth and PM depth (0x19 both), a sawtooth.
        for (reg, value) in [(0x18, 0xC8), (0x19, 0x40), (0x19, 0xB0), (0x1B, 0x00)] {
            w(&mut ym, &mut samples, reg, value);
        }
        for ch in 0..8 {
            w(&mut ym, &mut samples, 0x08, 0x78 | ch);
        }
        let s = |ym: &mut Ym2151, samples: &mut Vec<(i16, i16)>, count| {
            for _ in 0..count {
                samples.push(sample(ym));
            }
        };
        s(&mut ym, &mut samples, 3000);
        // Its square, triangle and noise.
        for wave in 1..4 {
            w(&mut ym, &mut samples, 0x1B, wave);
            s(&mut ym, &mut samples, 2000);
        }
        // Noise, channel 7's.
        w(&mut ym, &mut samples, 0x0F, 0x8C);
        s(&mut ym, &mut samples, 1500);
        for ch in (0..8).step_by(2) {
            w(&mut ym, &mut samples, 0x08, ch);
        }
        s(&mut ym, &mut samples, 3000);
        // The LFO held in reset a while.
        w(&mut ym, &mut samples, 0x01, 0x02);
        s(&mut ym, &mut samples, 200);
        w(&mut ym, &mut samples, 0x01, 0x00);
        // The top key code, and the bottom.
        for (reg, value) in [(0x28, 0x7F), (0x30, 0xFC), (0x08, 0x78)] {
            w(&mut ym, &mut samples, reg, value);
        }
        s(&mut ym, &mut samples, 1000);
        for (reg, value) in [(0x28, 0x00), (0x30, 0x00), (0x08, 0x00), (0x08, 0x78)] {
            w(&mut ym, &mut samples, reg, value);
        }
        s(&mut ym, &mut samples, 1000);
        // ymfm's samples (its ym2151, reset, written and generated alike):
        // their count, and the CRC-32 of them as 16-bit pairs.
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|&(l, r)| [l.to_le_bytes(), r.to_le_bytes()].concat())
            .collect();
        assert_eq!(samples.len(), 15_953);
        assert_eq!(crc32(&bytes), 0xB858_1B40);
    }

    #[test]
    fn a_timer_run_out_sets_its_status_and_irq_if_enabled_and_starts_again() {
        let mut ym = Ym2151::new();
        let mut samples = Vec::new();
        // Timer A at 1020: 4 samples. Loaded, its IRQ off.
        write(&mut ym, &mut samples, 0x10, 0xFF);
        write(&mut ym, &mut samples, 0x11, 0x00);
        write(&mut ym, &mut samples, 0x14, 0x01);
        for _ in 0..8 {
            sample(&mut ym);
        }
        assert_eq!((ym.status(), ym.irq()), (0, false), "its IRQ off");
        // On: run out 4 samples on.
        write(&mut ym, &mut samples, 0x14, 0x05);
        let mut at = 1;
        while ym.status() & 1 == 0 {
            sample(&mut ym);
            at += 1;
            assert!(at < 10, "never run out");
        }
        assert!(ym.irq());
        // Its status reset; it's run on, and runs out again.
        ym.write_address(0x14);
        ym.write_data(0x15);
        assert_eq!(ym.status() & 1, 0);
        assert!(!ym.irq());
        for _ in 0..4 {
            sample(&mut ym);
        }
        assert_eq!(ym.status() & 1, 1, "again");
        // Timer B at 255: 16 samples, as bit 1.
        write(&mut ym, &mut samples, 0x14, 0x30);
        write(&mut ym, &mut samples, 0x12, 0xFF);
        write(&mut ym, &mut samples, 0x14, 0x0A);
        for _ in 0..16 {
            sample(&mut ym);
        }
        assert_eq!(ym.status() & 3, 2);
    }

    #[test]
    fn its_busy_a_writes_64_clocks_and_a_write_then_is_lost() {
        let mut ym = Ym2151::new();
        ym.write_address(0x12);
        ym.write_data(0x80);
        assert_eq!(ym.status(), 0x80, "busy");
        for _ in 0..63 {
            ym.tick();
        }
        assert_eq!(ym.status(), 0x80);
        // Lost: timer B 0, not 0x80, run out after 256 x 16 samples.
        ym.write_address(0x12);
        ym.write_data(0x00);
        ym.tick();
        assert_eq!(ym.status(), 0, "done");
        ym.write_address(0x14);
        ym.write_data(0x0A);
        for _ in 0..(128 * 16 + 1) * 64 {
            ym.tick();
        }
        assert_eq!(ym.status() & 2, 2, "timer B of 0x80, as it was");
    }

    #[test]
    fn csm_keys_every_channel_on_as_timer_a_runs_out() {
        let mut ym = Ym2151::new();
        let mut samples = Vec::new();
        // Channel 0's carrier, loud and quick, both sides; algorithm 7.
        for (reg, value) in [(0x20, 0xC7), (0x28, 0x4A), (0x58, 0x01), (0x98, 0x1F)] {
            write(&mut ym, &mut samples, reg, value);
        }
        for _ in 0..100 {
            assert_eq!(sample(&mut ym), (0, 0), "not keyed on");
        }
        write(&mut ym, &mut samples, 0x10, 0xFF);
        write(&mut ym, &mut samples, 0x14, 0x81);
        let sounding = (0..200).any(|_| sample(&mut ym) != (0, 0));
        assert!(sounding, "keyed on by CSM");
    }
}
