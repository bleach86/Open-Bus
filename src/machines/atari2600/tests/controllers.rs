//! The controllers: a paddle's input charging, from VBLANK's grounding it,
//! as long as its pot says; paddles found in a game's code; Stella's
//! properties; a driving controller's gray code; and the phosphor.

#[cfg(test)]
mod tests {
    use crate::machines::atari2600::controllers::{Controller, Paddle, Wheel};
    use crate::machines::atari2600::database::{self, Controller as Named};
    use crate::machines::atari2600::tia::{LINE, Tia};
    use crate::machines::region::Region;

    const VBLANK: u16 = 0x01;
    const INPT0: u16 = 0x08;

    /// The lines after VBLANK lets go of paddle 0's capacitor till INPT0
    /// reads it charged, its pot `resistance` ohms.
    fn lines_to_charge(resistance: u32) -> usize {
        let mut tia = Tia::new(Region::Ntsc);
        tia.paddle(0, Some(resistance));
        tia.write(VBLANK, 0x80);
        for _ in 0..LINE as usize * 10 {
            tia.clock();
        }
        assert_eq!(tia.read(INPT0, 0) & 0x80, 0, "grounded");
        tia.write(VBLANK, 0x00);
        (0..600)
            .find(|_| {
                for _ in 0..LINE {
                    tia.clock();
                }
                tia.read(INPT0, 0) & 0x80 != 0
            })
            .unwrap_or(600)
    }

    #[test]
    fn a_paddle_charges_as_long_as_its_pot_says() {
        assert!(lines_to_charge(0) < 2, "turned all the way right");
        let half = lines_to_charge(500_000);
        assert!((185..=195).contains(&half), "{half} lines, turned halfway");
        let most = lines_to_charge(1_000_000);
        assert!(
            (377..=380).contains(&most),
            "{most} lines, all the way left"
        );
        let mut tia = Tia::new(Region::Ntsc);
        for _ in 0..LINE as usize * 600 {
            tia.clock();
        }
        assert_eq!(tia.read(INPT0, 0) & 0x80, 0, "no paddle there, never");
    }

    #[test]
    fn a_paddle_turns_by_the_stick_or_where_the_pointer_is() {
        let mut paddle = Paddle::new();
        assert_eq!(paddle.resistance(false), 500_000);
        for _ in 0..200 {
            paddle.turn(1.0, None);
        }
        assert_eq!(paddle.resistance(false), 0, "right, the least");
        assert_eq!(paddle.resistance(true), 1_000_000, "the other way round");
        paddle.turn(0.0, Some(0.25));
        assert_eq!(paddle.resistance(false), 750_000);
        paddle.turn(1.0, Some(0.25));
        assert!(
            paddle.resistance(false) < 750_000,
            "the stick, the pointer not moved"
        );
    }

    #[test]
    fn paddles_are_found_in_code_that_reads_them_as_a_paddle_game() {
        // LDA INPT0; BPL: only a paddle game does that.
        let mut image = vec![0xEA; 0x1000];
        image[0x100..0x103].copy_from_slice(&[0xA5, 0x08, 0x10]);
        assert!(matches!(
            Controller::found(&image, false),
            Controller::Paddles { .. }
        ));
        assert_eq!(Controller::found(&image, true), Controller::Joystick);
        assert_eq!(
            Controller::found(&[0xEA; 0x1000], false),
            Controller::Joystick
        );
    }

    #[test]
    fn stellas_properties_by_a_roms_md5() {
        let known = database::find("3051b6071cb26377cd428af155e1bfc4").unwrap();
        assert_eq!(known.controllers[0], Some(Named::PaddlesOtherAxisReversed));
        assert!(known.swap_paddles);
        let known = database::find("1343de49c2a50d99176255f99f0d0234").unwrap();
        assert!(known.phosphor);
        assert_eq!(known.blend, Some(55));
        assert!(database::find("00000000000000000000000000000000").is_none());
    }

    #[test]
    fn a_wheel_turns_through_its_gray_code() {
        let mut wheel = Wheel::default();
        let mut codes = vec![wheel.pins()];
        for _ in 0..8 {
            wheel.turn(false, true);
            codes.push(wheel.pins());
        }
        codes.dedup();
        assert_eq!(codes, [0b11, 0b01, 0b00, 0b10, 0b11]);
    }
}
