//! The window, kept on the screen: placed where it's asked, but moved and
//! made smaller to be inside the part of the screen a taskbar's not over,
//! its title bar too; or, opened at its own size, made smaller, its shape
//! kept, to fit.

#[cfg(test)]
mod tests {
    use crate::host::window::{placed_inside, sized_to_fit};

    /// A laptop's 1366 by 768, a dock 66 wide at its left.
    const LAPTOP: (i32, i32, u32, u32) = (66, 0, 1300, 768);

    #[test]
    fn a_place_that_fits_is_kept() {
        let place = [100, 60, 800, 600];
        assert_eq!(placed_inside(place, (0, 0, 1920, 1080)), place);
    }

    #[test]
    fn a_place_too_big_or_off_the_screen_is_moved_and_made_smaller() {
        assert_eq!(
            placed_inside([66, 32, 1274, 768], LAPTOP),
            [66, 40, 1274, 728],
            "its title bar left room, no taller than that leaves"
        );
        assert_eq!(
            placed_inside([0, 500, 400, 400], LAPTOP),
            [66, 368, 400, 400],
            "moved in"
        );
    }

    #[test]
    fn a_size_too_big_is_made_smaller_its_shape_kept() {
        assert_eq!(sized_to_fit((1120, 768), LAPTOP), (1061, 728), "the IIe's");
        assert_eq!(sized_to_fit((1024, 720), LAPTOP), (1024, 720), "fits");
    }
}
