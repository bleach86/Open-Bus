//! A touch screen's controls, on Android: the game screen's own buttons,
//! drawn by Android over the picture (a NES pad's, or a SNES pad's with X,
//! Y, L and R too, or a joystick's stick and its two buttons, A and B),
//! which tell the emulator which of them are held, as a mask of the bits
//! below, and where the stick's pushed. They're the first player's, added
//! onto whatever's bound: no control at all, so rebinding a key or a
//! gamepad never moves them, and they never move it. And the game held
//! still, as the game screen's own menus are over it: not by SDL's pause,
//! which takes the window out of fullscreen, bringing Android's bars up.

use crate::host::display::Gamepad;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};

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
/// The stick, while it's held: where it's pushed, across and down, -1 to
/// 1 each, as two f32s' bits, across low; and whether it's held.
static STICK: AtomicU64 = AtomicU64::new(0);
static STEERING: AtomicBool = AtomicBool::new(false);

/// The stick held, pushed `x` across and `y` down; or let go, centred.
pub fn steer(x: f32, y: f32, held: bool) {
    STICK.store(
        u64::from(x.to_bits()) | u64::from(y.to_bits()) << 32,
        Ordering::Relaxed,
    );
    STEERING.store(held, Ordering::Relaxed);
}

/// Where the stick's pushed, across and down, while it's held.
pub fn stick() -> Option<(f32, f32)> {
    if !STEERING.load(Ordering::Relaxed) {
        return None;
    }
    let bits = STICK.load(Ordering::Relaxed);
    Some((
        f32::from_bits(bits as u32),
        f32::from_bits((bits >> 32) as u32),
    ))
}

/// What's held on the screen, as it was last told.
pub fn held() -> u16 {
    HELD.load(Ordering::Relaxed)
}

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

/// GameActivity's `steered`, as the stick's pushed, and let go.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_app_openbus_GameActivity_steered(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    x: f32,
    y: f32,
    held: u8,
) {
    steer(x, y, held != 0);
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
