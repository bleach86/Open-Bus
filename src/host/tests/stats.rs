//! What's measured of a machine running, and how it's shown.

#[cfg(test)]
mod tests {
    use crate::host::notice;
    use crate::host::stats::Stats;

    /// `seconds` of frames at 60 a second, of `cycles`, `busy` seconds each.
    fn run(stats: &mut Stats, seconds: f64, cycles: u64, busy: f64) {
        let mut now = 0.0;
        while now < seconds {
            stats.frame(now, cycles, busy);
            now += 1.0 / 60.0;
        }
    }

    #[test]
    fn it_measures_a_machine_at_full_speed() {
        let mut stats = Stats::new();
        run(&mut stats, 1.0, 29830, 0.004);
        assert!(
            (stats.frames_per_second - 60.0).abs() < 0.5,
            "{}",
            stats.frames_per_second
        );
        assert!(
            (stats.cycles_per_second - 1_789_800.0).abs() < 20_000.0,
            "{}",
            stats.cycles_per_second
        );
        assert_eq!(stats.cycles_per_frame, 29830);
        assert!((stats.load - 0.24).abs() < 0.02, "{}", stats.load);
    }

    #[test]
    fn it_says_so_until_its_measured_something() {
        let mut stats = Stats::new();
        assert_eq!(stats.lines(1_789_773), ["MEASURING"]);
        stats.frame(0.0, 100, 0.0);
        stats.frame(0.1, 100, 0.0);
        assert_eq!(
            stats.lines(1_789_773),
            ["MEASURING"],
            "under its half second"
        );
    }

    #[test]
    fn its_lines_say_how_it_compares() {
        let mut stats = Stats::new();
        (
            stats.frames_per_second,
            stats.cycles_per_second,
            stats.cycles_per_frame,
            stats.load,
        ) = (30.0, 894_886.0, 29830, 1.5);
        let lines = stats.lines(1_789_773);
        assert_eq!(lines[0], "FPS 30.0 OF 60.0", "half speed");
        assert_eq!(lines[1], "CPU 0.895 MHZ 50%");
        assert_eq!(lines[2], "HOST LOAD 150%");
        assert_eq!(lines[3], "CYCLES A FRAME 29830");
    }

    #[test]
    fn a_panel_dims_what_it_covers_and_leaves_the_rest() {
        let (width, height) = (256, 240);
        let mut picture = vec![0x808080; width * height];
        notice::panel(
            &mut picture,
            width,
            height,
            &["HI".to_string(), "THERE".to_string()],
        );
        assert_eq!(picture[width - 1], 0x808080, "right of it, untouched");
        assert_eq!(picture[100 * width], 0x808080, "under it, untouched");
        assert_eq!(picture[0], 0x202020, "not dimmed");
        let lit = picture[..18 * width]
            .iter()
            .filter(|&&dot| dot == 0xFFFFFF)
            .count();
        assert!(lit > 30, "its text not drawn: {lit}");
    }

    #[test]
    fn a_panel_of_too_much_fits_in_a_small_picture() {
        let (width, height) = (64, 40);
        let mut picture = vec![0; width * height];
        let lines: Vec<String> = (0..50)
            .map(|n| format!("A VERY LONG LINE INDEED, NUMBER {n}"))
            .collect();
        notice::panel(&mut picture, width, height, &lines);
    }
}
