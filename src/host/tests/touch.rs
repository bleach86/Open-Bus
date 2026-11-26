use crate::host::display::Gamepad;
use crate::host::touch::{self, A, LEFT, R, START, X};

#[test]
fn the_screens_buttons_are_added_onto_the_pad_and_let_go() {
    let pad = Gamepad {
        b: true,
        ..Gamepad::default()
    };
    touch::hold(A | LEFT | START | X | R);
    let held = touch::onto(pad);
    touch::hold(0);
    assert_eq!(
        held,
        Gamepad {
            a: true,
            b: true,
            left: true,
            start: true,
            x: true,
            r: true,
            ..Gamepad::default()
        }
    );
    assert_eq!(touch::onto(pad), pad);
}

#[test]
fn the_screens_stick_is_where_its_pushed_while_its_held() {
    touch::steer(0.5, -1.0, true);
    let pushed = touch::stick();
    touch::steer(0.0, 0.0, false);
    assert_eq!(pushed, Some((0.5, -1.0)));
    assert_eq!(touch::stick(), None, "let go, it's not the joystick's");
}
