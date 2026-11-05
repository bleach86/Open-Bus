//! The NES's sound, as the window has it: a triangle silenced as games
//! silence it, its period 0, a note far above hearing, not heard (as it
//! would be, folded back down, sampled without being filtered first); a
//! note heard.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::host::machine::Machine;
    use crate::machines::nes::cartridge::Cartridge;
    use crate::machines::nes::{Nes, CLOCK_HZ};

    /// The triangle playing at `period` for a second, its last half second
    /// heard: its loudest, of the left channel's samples.
    fn triangle(period: u16) -> f32 {
        let mut image = b"NES\x1A\x01\x01".to_vec();
        image.extend([0; 10]);
        image.extend([0xEA; 0x4000 + 0x2000]);
        let mut nes = Nes::new(Cartridge::new(&image).unwrap());
        nes.sound_on();
        nes.write(0x4015, 0x04); // the triangle enabled
        nes.write(0x4008, 0xFF); // its linear counter held
        nes.write(0x400A, period as u8);
        nes.write(0x400B, 0x08 | (period >> 8) as u8); // its length loaded
        for _ in 0..CLOCK_HZ {
            nes.tick();
        }
        let heard = nes.sound();
        heard[heard.len() / 2..].iter().step_by(2).fold(0.0, |loudest: f32, sample| loudest.max(sample.abs()))
    }

    #[test]
    fn a_triangle_silenced_as_games_do_is_not_heard() {
        // 440 Hz: the CPU's clock over 32 steps of (period + 1) cycles.
        let note = triangle((CLOCK_HZ / (32 * 440) - 1) as u16);
        let ultrasonic = triangle(0);
        assert!(note > 0.02, "the note not heard: {note}");
        assert!(ultrasonic < note / 100.0, "the ultrasonic triangle heard: {ultrasonic}, the note {note}");
    }
}
