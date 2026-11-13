//! The light gun: what its photodiode sees of a picture, and when.

#[cfg(test)]
mod tests {
    use crate::host::display::Pointer;
    use crate::machines::atari7800::LINE;
    use crate::machines::atari7800::light_gun::LightGun;

    const WIDTH: usize = 320;
    const ROWS: usize = 200;
    /// Two colours: black, and white.
    const COLOURS: [u32; 2] = [0x000000, 0xFFFFFF];

    /// A gun aimed at dot `x` of row `y`.
    fn aimed(x: f32, y: f32) -> LightGun {
        let mut gun = LightGun::new(0);
        let pointer = Pointer {
            at: Some((x, y)),
            buttons: [false; 2],
        };
        gun.aim(pointer, WIDTH, ROWS);
        gun
    }

    /// Whether `gun` sees light, the beam at `clock` of `row`, `drawn` the
    /// picture.
    fn sees(gun: &LightGun, row: i32, clock: i32, drawn: &[u8]) -> bool {
        gun.sees(row, clock, LINE as i32, drawn, WIDTH, &COLOURS)
    }

    #[test]
    fn it_sees_a_bright_dot_where_its_aimed_74_clocks_after_and_48_more() {
        let white = vec![1; WIDTH * ROWS];
        let gun = aimed(100.0, 50.0);
        // It sees 16 dots either side, its radius of 8 colour clocks.
        assert!(!sees(&gun, 50, 100 - 17 + 74, &white), "not yet");
        assert!(
            sees(&gun, 50, 100 - 15 + 74, &white),
            "as the beam's 16 dots off"
        );
        assert!(sees(&gun, 50, 100 + 15 + 74 + 48, &white), "held");
        assert!(!sees(&gun, 50, 100 + 17 + 74 + 48, &white), "no longer");
        assert!(
            !sees(&gun, 50, 100 - 15 + 74, &vec![0; WIDTH * ROWS]),
            "a dark picture"
        );
    }

    #[test]
    fn it_sees_16_lines_up_and_down_as_a_colour_clocks_two_lines_wide() {
        let white = vec![1; WIDTH * ROWS];
        let gun = aimed(100.0, 50.0);
        assert!(sees(&gun, 50 - 15, 100 + 74, &white));
        assert!(!sees(&gun, 50 - 17, 100 + 74 + 48, &white));
        assert!(sees(&gun, 50 + 15, 100 + 74, &white));
        assert!(!sees(&gun, 50 + 17, 100 + 74 + 48, &white));
    }

    #[test]
    fn what_it_sees_at_a_lines_end_it_says_on_the_next() {
        let white = vec![1; WIDTH * ROWS];
        let gun = aimed(310.0, 50.0);
        // 74 clocks after the beam's at dot 310, it's the next line's.
        let clock = 310 + 74 - LINE as i32;
        assert!(sees(&gun, 51, clock, &white));
        let mut dark_above = white.clone();
        dark_above[..51 * WIDTH].fill(0);
        assert!(!sees(&gun, 51, clock, &dark_above), "the line before's dot");
    }

    #[test]
    fn its_trigger_is_its_first_button_or_its_second_aimed_away() {
        let mut gun = LightGun::new(1);
        gun.aim(
            Pointer {
                at: Some((10.0, 10.0)),
                buttons: [true, false],
            },
            WIDTH,
            ROWS,
        );
        assert_eq!((gun.at, gun.trigger), (Some((10, 10)), true));
        assert_eq!(gun.stick(), 0x0F, "up high, pulled; the rest high");
        gun.aim(
            Pointer {
                at: Some((10.0, 10.0)),
                buttons: [false, true],
            },
            WIDTH,
            ROWS,
        );
        assert_eq!((gun.at, gun.trigger), (None, true), "aimed away");
        gun.aim(Pointer::default(), WIDTH, ROWS);
        assert_eq!(gun.stick(), 0x0E, "not pulled: up low");
    }
}
