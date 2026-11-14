//! VERA's sound: a PSG voice's pitch, its waveforms, its volume and sides;
//! noise; the PCM's FIFO, its formats, its rate, its flags and AFLOW; and
//! both through VERA's registers, a sample each 163.84 CPU cycles.

#[cfg(test)]
mod tests {
    use crate::machines::x16::vera::Vera;
    use crate::machines::x16::vera_audio::{Pcm, Psg};

    /// Voice 0 set: its frequency word, its volume and sides (byte 2), its
    /// waveform and width (byte 3).
    fn voice(psg: &mut Psg, frequency: u16, sides_volume: u8, waveform_width: u8) {
        let [low, high] = frequency.to_le_bytes();
        for (reg, value) in [low, high, sides_volume, waveform_width]
            .into_iter()
            .enumerate()
        {
            psg.write(reg, value);
        }
    }

    #[test]
    fn a_voices_pitch_is_its_frequency_word_in_the_sample_rates_2_to_the_17th() {
        let mut psg = Psg::new();
        // 440 Hz, a square wave (pulse, width 63), loudest, both sides.
        voice(&mut psg, 1181, 0xC0 | 63, 63);
        let samples: Vec<(i16, i16)> = (0..48828).map(|_| psg.sample()).collect();
        let rises = samples
            .windows(2)
            .filter(|w| w[0].0 < 0 && w[1].0 > 0)
            .count();
        assert!((439..=441).contains(&rises), "{rises} cycles in a second");
        let (high, low) = (
            samples.iter().map(|s| s.0).max(),
            samples.iter().map(|s| s.0).min(),
        );
        assert_eq!((high, low), (Some((31 * 511) >> 3), Some((-32 * 511) >> 3)));
        assert!(samples.iter().all(|s| s.0 == s.1), "left as right");
    }

    #[test]
    fn its_waveforms_volume_and_sides_are_as_its_registers_say() {
        let mut psg = Psg::new();
        // A sawtooth, its XOR 0 (width 63): rising, 64 steps a cycle.
        voice(&mut psg, 0x800, 0x40 | 63, 0x40 | 63);
        let saw: Vec<i16> = (0..64).map(|_| psg.sample().0).collect();
        assert!(
            saw.windows(2).take(30).all(|w| w[1] > w[0]),
            "rising: {saw:?}"
        );
        assert_eq!(psg.sample().1, 0, "left alone");
        // Quieter: volume 32 (85 of 511).
        voice(&mut psg, 0x800, 0x80 | 32, 0x40 | 63);
        let quiet = (0..64).map(|_| psg.sample().1.abs()).max().unwrap();
        assert_eq!(quiet, (32 * 85) >> 3);
        // Neither side: silent, its phase held.
        voice(&mut psg, 0x800, 63, 0x40 | 63);
        assert!((0..64).all(|_| psg.sample() == (0, 0)));
        // Noise: changing, as its phase wraps.
        voice(&mut psg, 0xFFFF, 0xC0 | 63, 0xC0);
        let noise: Vec<i16> = (0..100).map(|_| psg.sample().0).collect();
        let values = noise
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        assert!(values > 10, "{values} values of noise");
    }

    #[test]
    fn the_pcm_plays_its_fifo_as_its_format_and_rate_say() {
        let mut pcm = Pcm::new();
        assert_eq!(pcm.ctrl(), 0x40, "empty");
        assert!(pcm.low());
        pcm.write_ctrl(0x0F); // 8 bits, mono, loudest
        for byte in [0x40, 0xC0, 0x7F] {
            pcm.write_data(byte);
        }
        assert_eq!(pcm.sample(), (0, 0), "rate 0: stopped");
        pcm.write_rate(128);
        assert_eq!(pcm.sample(), (0x4000, 0x4000));
        assert_eq!(pcm.sample(), (-0x4000, -0x4000));
        pcm.write_rate(64);
        let half = [pcm.sample(), pcm.sample(), pcm.sample()];
        assert_eq!(
            half,
            [(-0x4000, -0x4000), (0x7F00, 0x7F00), (0x7F00, 0x7F00)],
            "half speed: each twice"
        );
        // 16 bits, stereo, volume 7: 8 64ths.
        pcm.write_ctrl(0x30 | 7);
        pcm.write_rate(128);
        for byte in [0x00, 0x40, 0x00, 0xC0] {
            pcm.write_data(byte);
        }
        assert_eq!(pcm.sample(), (0x4000 / 8, -0x4000 / 8));
        assert_eq!(pcm.sample(), (0, 0), "empty: silent");
        // Not all of a sample there: the FIFO emptied.
        pcm.write_data(0x12);
        pcm.sample();
        assert_eq!(pcm.ctrl() & 0x40, 0x40);
        // Full at 4,095; AFLOW under 1,024; reset, empty.
        for _ in 0..1024 {
            pcm.write_data(1);
        }
        assert!(!pcm.low());
        for _ in 0..4000 {
            pcm.write_data(1);
        }
        assert_eq!(pcm.ctrl() & 0xC0, 0x80, "full");
        pcm.write_ctrl(0x80);
        assert_eq!(pcm.ctrl(), 0x40, "reset: empty, its format 0");
    }

    #[test]
    fn veras_registers_reach_them_a_sample_each_163_84_cycles() {
        const ADDR_L: u8 = 0x00;
        const ADDR_M: u8 = 0x01;
        const ADDR_H: u8 = 0x02;
        const DATA0: u8 = 0x03;
        const IEN: u8 = 0x06;
        const ISR: u8 = 0x07;
        const AUDIO_CTRL: u8 = 0x1B;
        const AUDIO_RATE: u8 = 0x1C;
        const AUDIO_DATA: u8 = 0x1D;
        let mut vera = Vera::new();
        assert_eq!(vera.read(ISR) & 0x08, 0x08, "AFLOW: empty");
        vera.write(IEN, 0x08);
        assert!(vera.irq());
        vera.write(AUDIO_CTRL, 0x0F);
        for _ in 0..1024 {
            vera.write(AUDIO_DATA, 0x40);
        }
        assert_eq!(vera.read(ISR) & 0x08, 0, "a quarter full");
        assert!(!vera.irq());
        vera.write(AUDIO_RATE, 128);
        // 100 samples: 16,384 cycles; and one more's 163 or 164 later.
        for _ in 0..16_384 {
            vera.tick();
        }
        assert_eq!(vera.sound(), (0x4000, 0x4000));
        assert!(vera.irq(), "AFLOW again, 100 read");
        // The PSG's voice 1, written in video RAM at $1F9C4: a square wave.
        vera.write(AUDIO_CTRL, 0x80);
        vera.write(ADDR_L, 0xC4);
        vera.write(ADDR_M, 0xF9);
        vera.write(ADDR_H, 0x11); // $1F9C4, stepping by 1
        for byte in [0x00, 0x10, 0xC0 | 63, 63] {
            vera.write(DATA0, byte);
        }
        for _ in 0..1000 {
            vera.tick();
        }
        let mut levels = std::collections::BTreeSet::new();
        for _ in 0..200_000 {
            vera.tick();
            levels.insert(vera.sound().0);
        }
        assert_eq!(levels.len(), 2, "high and low: {levels:?}");
        vera.write(0x05, 0x80);
        for _ in 0..200 {
            vera.tick();
        }
        assert_eq!(vera.sound(), (0, 0), "RESET: silent");
    }
}
