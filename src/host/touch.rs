//! A touch screen's controls, on Android: the game screen's own buttons,
//! drawn by Android over the picture, which tell the emulator which of them
//! are held, as a mask of the bits below. They're the first player's, added
//! onto whatever's bound: no control at all, so rebinding a key or a
//! gamepad never moves them, and they never move it.

use crate::host::display::Gamepad;
use std::sync::atomic::{AtomicU8, Ordering};

pub const A: u8 = 1 << 0;
pub const B: u8 = 1 << 1;
pub const SELECT: u8 = 1 << 2;
pub const START: u8 = 1 << 3;
pub const UP: u8 = 1 << 4;
pub const DOWN: u8 = 1 << 5;
pub const LEFT: u8 = 1 << 6;
pub const RIGHT: u8 = 1 << 7;

/// What's held on the screen, as it was last told.
static HELD: AtomicU8 = AtomicU8::new(0);

/// `buttons` held on the screen, the rest let go.
pub fn hold(buttons: u8) {
    HELD.store(buttons, Ordering::Relaxed);
}

/// `pad`, with what's held on the screen held too.
pub fn onto(pad: Gamepad) -> Gamepad {
    let held = HELD.load(Ordering::Relaxed);
    let on = |bit: u8| held & bit != 0;
    Gamepad {
        up: pad.up || on(UP),
        down: pad.down || on(DOWN),
        left: pad.left || on(LEFT),
        right: pad.right || on(RIGHT),
        a: pad.a || on(A),
        b: pad.b || on(B),
        select: pad.select || on(SELECT),
        start: pad.start || on(START),
        ..pad
    }
}

/// The game screen's way in: GameActivity's `touched`, from Android's own
/// thread, as its buttons are pressed and let go.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_app_openbus_GameActivity_touched(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    buttons: i32,
) {
    hold(buttons as u8);
}
