//! The host's sound on its way: stretched and squeezed.

#[cfg(test)]
mod tests {
    use crate::host::sound::Stretch;

    /// A ramp of `frames` frames of 2 channels, from `from`, the right
    /// channel the left's negative.
    fn ramp(from: usize, frames: usize) -> Vec<f32> {
        (from..from + frames)
            .flat_map(|n| [n as f32, -(n as f32)])
            .collect()
    }

    #[test]
    fn at_one_its_as_it_was_a_frame_later() {
        let mut stretch = Stretch::<2>::default();
        let mut played = Vec::new();
        stretch.run(&ramp(1, 100), 1.0, &mut played);
        stretch.run(&ramp(101, 100), 1.0, &mut played);
        assert_eq!(played, ramp(0, 200));
    }

    #[test]
    fn squeezed_or_stretched_its_that_many_fewer_or_more() {
        for (step, frames) in [(1.01, 9901), (0.99, 10101)] {
            let mut stretch = Stretch::<2>::default();
            let mut played = Vec::new();
            for batch in 0..100 {
                stretch.run(&ramp(1 + batch * 100, 100), step, &mut played);
            }
            let played_frames = played.len() / 2;
            assert!(
                played_frames.abs_diff(frames) <= 1,
                "{step}: {played_frames}"
            );
            // A ramp played anywhere between its frames is still one.
            for pair in played.chunks(2) {
                assert_eq!(pair[0], -pair[1]);
            }
            let rising = played.chunks(4).all(|two| two.len() < 4 || two[2] > two[0]);
            assert!(rising, "{step}");
        }
    }
}
