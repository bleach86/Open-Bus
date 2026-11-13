//! The controllers: a paddle's input charging, from VBLANK's grounding it,
//! as long as its pot says; paddles found in a game's code; Stella's
//! properties; a driving controller's gray code; a keypad's columns, its
//! keys typed, found in a game's code, and read through the console; and
//! the phosphor.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::host::display::Key;
    use crate::host::machine::Machine;
    use crate::machines::atari2600::Atari2600;
    use crate::machines::atari2600::cartridge::Cartridge;
    use crate::machines::atari2600::controllers::{
        Column, Controller, Paddle, Wheel, columns, keypad_key,
    };
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

    #[test]
    fn a_keypads_column_is_grounded_by_a_key_in_a_row_driven_low() {
        const FIVE: u16 = 1 << 4;
        const POUND: u16 = 1 << 11;
        // 5: the second row's, the second column's.
        assert_eq!(
            columns(FIVE, 0b1101),
            [Column::Open, Column::Ground, Column::Open]
        );
        assert_eq!(
            columns(FIVE, 0b1111),
            [Column::Open, Column::Supply, Column::Open],
            "its row high"
        );
        assert_eq!(columns(0, 0), [Column::Open; 3], "no key");
        // #: the fourth row's, the third column's; with 9, above it, high.
        assert_eq!(columns(POUND | 1 << 8, 0b0111)[2], Column::Ground);
    }

    #[test]
    fn keys_typed_are_one_keypads_digits_or_two_keypads_as_stella_has_them() {
        assert_eq!(keypad_key('5', false), Some((0, 1 << 4)));
        assert_eq!(keypad_key('*', false), Some((0, 1 << 9)));
        assert_eq!(keypad_key('0', false), Some((0, 1 << 10)));
        assert_eq!(keypad_key('#', false), Some((0, 1 << 11)));
        assert_eq!(keypad_key('q', false), None);
        assert_eq!(keypad_key('W', true), Some((0, 1 << 4)), "the left's 5");
        assert_eq!(keypad_key('8', true), Some((1, 1 << 0)), "the right's 1");
        assert_eq!(keypad_key('/', true), Some((1, 1 << 11)), "the right's #");
        assert_eq!(keypad_key('5', true), None);
    }

    #[test]
    fn a_keypad_is_found_where_its_columns_and_button_are_read_and_no_joystick() {
        let mut image = vec![0xEA; 0x1000];
        image[0x100..0x103].copy_from_slice(&[0xA5, 0x3C, 0x30]); // lda INPT4; bmi
        image[0x110..0x113].copy_from_slice(&[0x24, 0x38, 0x30]); // bit INPT0; bmi
        image[0x120..0x123].copy_from_slice(&[0x24, 0x39, 0x30]); // bit INPT1; bmi
        assert_eq!(Controller::found(&image, false), Controller::Keypad);
        image[0x130..0x133].copy_from_slice(&[0xAD, 0x80, 0x02]); // lda SWCHA
        assert_ne!(
            Controller::found(&image, false),
            Controller::Keypad,
            "the joystick's read too"
        );
    }

    #[test]
    fn a_keypads_keys_are_read_through_its_rows_and_columns() {
        const SWCHA: u16 = 0x280;
        const SWACNT: u16 = 0x281;
        const INPT1: u16 = 0x09;
        const INPT4: u16 = 0x0C;
        let cartridge = Cartridge::new(&[0xEA; 0x1000], None, 1_193_182).unwrap();
        let mut atari = Atari2600::new(cartridge, Region::Ntsc, Region::Ntsc);
        atari.controllers = [Controller::Keypad, Controller::Joystick];
        let lines = |atari: &mut Atari2600, n: usize| {
            for _ in 0..n * 76 {
                atari.tick();
            }
        };
        atari.write(SWACNT, 0xF0); // the left port's rows, outputs
        atari.write(SWCHA, 0xF0); // all high
        lines(&mut atari, 5);
        assert_eq!(atari.read(INPT1) & 0x80, 0x80, "no key: pulled up");

        atari.key(Key::Char(b'5'));
        atari.write(SWCHA, 0xD0); // 5's row low
        lines(&mut atari, 5);
        assert_eq!(atari.read(INPT1) & 0x80, 0, "5, grounding its column");
        assert_eq!(atari.read(INPT4) & 0x80, 0x80, "not the third column");
        atari.write(SWCHA, 0xB0); // the next row low
        lines(&mut atari, 5);
        assert_eq!(atari.read(INPT1) & 0x80, 0x80, "5's row high");

        atari.key(Key::Char(b'#'));
        atari.write(SWCHA, 0x70); // the bottom row low
        assert_eq!(atari.read(INPT4) & 0x80, 0, "#, on the fire button's");
    }
}
