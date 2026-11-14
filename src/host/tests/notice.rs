//! A notice, on a picture of any size: its band and its text, at the widest
//! fit, cut to what fits. A drive's light, at its corner.

#[cfg(test)]
mod tests {
    use crate::host::notice::{draw, light};

    /// The notice over a grey picture: the picture.
    fn drawn(width: usize, height: usize, text: &str) -> Vec<u32> {
        let mut picture = vec![0x808080; width * height];
        draw(&mut picture, width, height, text);
        picture
    }

    /// How wide the lit dots' span is, on the picture, across all its lines.
    fn span(picture: &[u32], width: usize) -> usize {
        let lit: Vec<usize> = (0..picture.len())
            .filter(|&at| picture[at] == 0xFFFFFF)
            .map(|at| at % width)
            .collect();
        lit.iter().max().unwrap() - lit.iter().min().unwrap() + 1
    }

    #[test]
    fn its_band_is_across_the_foot_of_any_picture() {
        for (width, height) in [(560, 192), (240, 224), (256, 240)] {
            let picture = drawn(width, height, "A");
            let (above, band) = picture.split_at((height - 12) * width);
            assert!(
                above.iter().all(|&dot| dot == 0x808080),
                "{width}: over what's above the band"
            );
            assert!(
                band.iter().all(|&dot| dot == 0x000000 || dot == 0xFFFFFF),
                "{width}: not its band"
            );
            assert!(band.iter().any(|&dot| dot == 0xFFFFFF), "{width}: no text");
        }
    }

    #[test]
    fn a_drives_light_is_at_the_bottom_right_corner_and_nowhere_else() {
        for (width, height) in [(560, 192), (240, 224)] {
            let mut picture = vec![0x808080; width * height];
            light(&mut picture, width, height);
            let changed: Vec<(usize, usize)> = (0..picture.len())
                .filter(|&at| picture[at] != 0x808080)
                .map(|at| (at % width, at / width))
                .collect();
            let left = changed.iter().map(|&(x, _)| x).min().unwrap();
            let top = changed.iter().map(|&(_, y)| y).min().unwrap();
            let (right, bottom) = (width - 1, height - 1);
            assert!(
                changed.iter().all(|&(x, y)| x >= left && y >= top),
                "{width}: past its corner"
            );
            assert_eq!(top, height - 7, "{width}: its lines");
            assert_eq!(
                right - left + 1,
                7 * (width / 280).max(1),
                "{width}: its width"
            );
            assert_eq!(
                (picture[top * width + left], picture[bottom * width + right]),
                (0x202020, 0x202020),
                "{width}: the picture under it dimmed"
            );
            let lit = |colour: u32| picture.iter().any(|&dot| dot == colour);
            assert!(lit(0xFF3020), "{width}: no lamp");
        }
    }

    #[test]
    fn its_dots_are_as_wide_as_40_characters_fit() {
        // 560 wide: a dot 2 pixels, so a letter's 5 dots 10; 240: 5.
        assert_eq!(span(&drawn(560, 192, "H"), 560), 10);
        assert_eq!(span(&drawn(240, 224, "H"), 240), 5);
    }

    #[test]
    fn its_text_is_cut_to_fit_and_centred() {
        let long = "H".repeat(100);
        // 34 characters fit 240 dots, of 7: the first's start to the last's end.
        assert_eq!(span(&drawn(240, 224, &long), 240), 33 * 7 + 5);
        let picture = drawn(240, 224, "HH");
        let lit: Vec<usize> = (0..picture.len())
            .filter(|&at| picture[at] == 0xFFFFFF)
            .map(|at| at % 240)
            .collect();
        let (left, right) = (lit.iter().min().unwrap(), lit.iter().max().unwrap());
        assert!(
            (left + right).abs_diff(238) <= 2,
            "not centred: {left} to {right}"
        );
    }

    #[test]
    fn a_lamps_round_on_a_picture_of_square_dots_and_on_a_tall_dots_one() {
        // The lamp's lit dots, as a box: its width and height in pixels.
        let lit = |width: usize, height: usize| {
            let mut picture = vec![0u32; width * height];
            light(&mut picture, width, height);
            let at: Vec<(usize, usize)> = (0..width * height)
                .filter(|&n| picture[n] == 0xFF3020)
                .map(|n| (n % width, n / width))
                .collect();
            let across =
                at.iter().map(|p| p.0).max().unwrap() - at.iter().map(|p| p.0).min().unwrap() + 1;
            let down =
                at.iter().map(|p| p.1).max().unwrap() - at.iter().map(|p| p.1).min().unwrap() + 1;
            (across, down)
        };
        assert_eq!(lit(640, 480), (6, 6), "the X16's: 2 by 2 a dot");
        assert_eq!(lit(560, 192), (6, 3), "the Apple's: 2 by 1, its dots tall");
    }
}
