use crate::host::fingers::{Fingers, Way};

const PICTURE: (f32, f32) = (640.0, 480.0);

/// `n` milliseconds, in seconds, as the fingers are given the time.
fn ms(n: u64) -> f64 {
    n as f64 / 1000.0
}

#[test]
fn a_finger_dragged_moves_the_pointer_by_as_far_as_it_goes() {
    let mut fingers = Fingers::new();
    let now = 100.0;
    fingers.down(Way::Trackpad, 1, (0.0, 0.0), PICTURE, now);
    assert_eq!(fingers.pointer(now).at, Some((320.0, 240.0)), "the middle");
    fingers.moved(Way::Trackpad, 1, (0.0, 0.0), (40.0, -8.0));
    fingers.up(Way::Trackpad, 1, now + ms(400));
    let pointer = fingers.pointer(now + ms(400));
    assert_eq!(pointer.at, Some((370.0, 230.0)), "a quarter as far again");
    assert_eq!(pointer.buttons, [false, false], "a drag's no click");
    // Where it landed's nothing: it's gone on from where it was.
    let later = now + ms(1000);
    fingers.down(Way::Trackpad, 2, (600.0, 10.0), PICTURE, later);
    assert_eq!(fingers.pointer(later).at, Some((370.0, 230.0)));
}

#[test]
fn a_tap_clicks_and_two_fingers_tapped_click_the_second_button() {
    let mut fingers = Fingers::new();
    let now = 100.0;
    fingers.down(Way::Trackpad, 1, (0.0, 0.0), PICTURE, now);
    assert_eq!(fingers.pointer(now).buttons, [false, false], "not yet");
    fingers.up(Way::Trackpad, 1, now + ms(100));
    assert_eq!(fingers.pointer(now + ms(110)).buttons, [true, false]);
    assert_eq!(fingers.pointer(now + ms(300)).buttons, [false, false]);

    let later = now + ms(2000);
    fingers.down(Way::Trackpad, 1, (0.0, 0.0), PICTURE, later);
    fingers.down(Way::Trackpad, 2, (0.0, 0.0), PICTURE, later + ms(20));
    fingers.moved(Way::Trackpad, 2, (0.0, 0.0), (1.0, 1.0));
    fingers.up(Way::Trackpad, 2, later + ms(120));
    fingers.up(Way::Trackpad, 1, later + ms(130));
    assert_eq!(fingers.pointer(later + ms(140)).buttons, [false, true]);
    assert_eq!(fingers.pointer(later + ms(140)).at, Some((320.0, 240.0)));
}

#[test]
fn a_tap_then_a_finger_held_drags_with_the_button_held() {
    let mut fingers = Fingers::new();
    let now = 100.0;
    fingers.down(Way::Trackpad, 1, (0.0, 0.0), PICTURE, now);
    fingers.up(Way::Trackpad, 1, now + ms(100));
    fingers.down(Way::Trackpad, 2, (0.0, 0.0), PICTURE, now + ms(300));
    fingers.moved(Way::Trackpad, 2, (0.0, 0.0), (80.0, 0.0));
    let pointer = fingers.pointer(now + ms(900));
    assert_eq!(pointer.buttons, [true, false], "held as it's dragged");
    assert_eq!(pointer.at, Some((420.0, 240.0)));
    fingers.up(Way::Trackpad, 2, now + ms(1000));
    assert_eq!(fingers.pointer(now + ms(1000)).buttons, [false, false]);
    // Too long after the tap: a move, not a drag.
    fingers.down(Way::Trackpad, 3, (0.0, 0.0), PICTURE, now + ms(2000));
    assert_eq!(fingers.pointer(now + ms(2500)).buttons, [false, false]);
}

#[test]
fn a_slow_or_moving_finger_is_no_tap() {
    let mut fingers = Fingers::new();
    let now = 100.0;
    fingers.down(Way::Trackpad, 1, (0.0, 0.0), PICTURE, now);
    fingers.up(Way::Trackpad, 1, now + ms(600));
    assert_eq!(fingers.pointer(now + ms(610)).buttons, [false, false]);
    fingers.down(Way::Trackpad, 1, (0.0, 0.0), PICTURE, now + ms(1000));
    fingers.moved(Way::Trackpad, 1, (0.0, 0.0), (10.0, 0.0));
    fingers.up(Way::Trackpad, 1, now + ms(1100));
    assert_eq!(fingers.pointer(now + ms(1110)).buttons, [false, false]);
}

#[test]
fn a_light_gun_is_aimed_where_the_finger_is_and_fired_as_its_held() {
    let mut fingers = Fingers::new();
    let now = 100.0;
    fingers.down(Way::Aimed, 1, (100.0, 50.0), (256.0, 240.0), now);
    let shot = fingers.pointer(now);
    assert_eq!(shot.at, Some((100.0, 50.0)));
    assert_eq!(shot.buttons, [true, false]);
    fingers.moved(Way::Aimed, 1, (120.0, 60.0), (20.0, 10.0));
    assert_eq!(fingers.pointer(now + ms(500)).at, Some((120.0, 60.0)));
    assert_eq!(fingers.pointer(now + ms(500)).buttons, [true, false]);
    // Lifted at once: held long enough to be seen, still where it was.
    fingers.down(Way::Aimed, 2, (10.0, 10.0), (256.0, 240.0), now + ms(1000));
    fingers.up(Way::Aimed, 1, now + ms(1000));
    fingers.up(Way::Aimed, 2, now + ms(1005));
    assert_eq!(fingers.pointer(now + ms(1010)).buttons, [true, false]);
    let after = fingers.pointer(now + ms(1200));
    assert_eq!(after.buttons, [false, false]);
    assert_eq!(after.at, Some((10.0, 10.0)));
}
