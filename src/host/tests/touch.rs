use crate::host::display::Gamepad;
use crate::host::touch::{self, A, LEFT, START};

#[test]
fn the_screens_buttons_are_added_onto_the_pad_and_let_go() {
    let pad = Gamepad {
        b: true,
        ..Gamepad::default()
    };
    touch::hold(A | LEFT | START);
    let held = touch::onto(pad);
    touch::hold(0);
    assert_eq!(
        held,
        Gamepad {
            a: true,
            b: true,
            left: true,
            start: true,
            ..Gamepad::default()
        }
    );
    assert_eq!(touch::onto(pad), pad);
}
