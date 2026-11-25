//! A touch screen's controls, on Android: the game screen's own buttons,
//! drawn by Android over the picture (a NES pad's, or a SNES pad's with X,
//! Y, L and R too), which tell the emulator which of them are held, as a
//! mask of the bits below. They're the first player's, added
//! onto whatever's bound: no control at all, so rebinding a key or a
//! gamepad never moves them, and they never move it. And the game held
//! still, as the game screen's own menus are over it: not by SDL's pause,
//! which takes the window out of fullscreen, bringing Android's bars up.

use crate::host::display::Gamepad;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};

pub const A: u16 = 1 << 0;
pub const B: u16 = 1 << 1;
pub const SELECT: u16 = 1 << 2;
pub const START: u16 = 1 << 3;
pub const UP: u16 = 1 << 4;
pub const DOWN: u16 = 1 << 5;
pub const LEFT: u16 = 1 << 6;
pub const RIGHT: u16 = 1 << 7;
pub const X: u16 = 1 << 8;
pub const Y: u16 = 1 << 9;
pub const L: u16 = 1 << 10;
pub const R: u16 = 1 << 11;

/// What's held on the screen, as it was last told.
static HELD: AtomicU16 = AtomicU16::new(0);
/// Whether the game's held still.
static STILL: AtomicBool = AtomicBool::new(false);

/// The game held still, or let go again.
pub fn hold_still(still: bool) {
    STILL.store(still, Ordering::Relaxed);
}

/// Whether the game's held still.
pub fn still() -> bool {
    STILL.load(Ordering::Relaxed)
}

/// `buttons` held on the screen, the rest let go.
pub fn hold(buttons: u16) {
    HELD.store(buttons, Ordering::Relaxed);
}

/// `pad`, with what's held on the screen held too.
pub fn onto(pad: Gamepad) -> Gamepad {
    let held = HELD.load(Ordering::Relaxed);
    let on = |bit: u16| held & bit != 0;
    Gamepad {
        up: pad.up || on(UP),
        down: pad.down || on(DOWN),
        left: pad.left || on(LEFT),
        right: pad.right || on(RIGHT),
        a: pad.a || on(A),
        b: pad.b || on(B),
        select: pad.select || on(SELECT),
        start: pad.start || on(START),
        x: pad.x || on(X),
        y: pad.y || on(Y),
        l: pad.l || on(L),
        r: pad.r || on(R),
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
    hold(buttons as u16);
}

/// GameActivity's `still`, as its menus are over the game, and gone.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_app_openbus_GameActivity_still(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    still: u8,
) {
    hold_still(still != 0);
}
