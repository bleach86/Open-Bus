// The NES's controls on a touch screen. Each finger's followed by itself, so
// that Right can be held with A: on the D-pad, by which way it is from the
// middle, and it stays the D-pad's wherever it slides to; on A and B, or
// Select and Start, by what it's over, so it can slide from one to another,
// and between A and B it's both.

export const touch = {
    up: false,
    down: false,
    left: false,
    right: false,
    a: false,
    b: false,
    select: false,
    start: false,
};

/// What each finger's on, and what that presses.
const fingers = new Map();

/// A finger inside this much of the D-pad's middle isn't pushing it.
const DEAD_ZONE = 0.18;
/// How far round from a direction it still counts: tan(67.5), so that each
/// diagonal has an eighth of the circle.
const REACH = 2.414;
/// The share of A and B's width that's each alone, the rest being both.
const ONE_BUTTON = 0.4;

function dpad(element, x, y) {
    const box = element.getBoundingClientRect();
    const dx = (x - (box.left + box.width / 2)) / (box.width / 2);
    const dy = (y - (box.top + box.height / 2)) / (box.height / 2);
    if (Math.hypot(dx, dy) < DEAD_ZONE) {
        return [];
    }
    const pressed = [];
    if (Math.abs(dx) < REACH * Math.abs(dy)) {
        pressed.push(dy < 0 ? 'up' : 'down');
    }
    if (Math.abs(dy) < REACH * Math.abs(dx)) {
        pressed.push(dx < 0 ? 'left' : 'right');
    }
    return pressed;
}

function buttons(element, x) {
    const box = element.getBoundingClientRect();
    const along = (x - box.left) / box.width;
    const pressed = [];
    if (along < 1 - ONE_BUTTON) {
        pressed.push('b');
    }
    if (along > ONE_BUTTON) {
        pressed.push('a');
    }
    return pressed;
}

/// What's pressed at `x`, `y`, by `finger`.
function under(finger, x, y) {
    if (finger.dpad) {
        return dpad(finger.dpad, x, y);
    }
    const element = document.elementFromPoint(x, y)?.closest('[data-pad]');
    if (element?.dataset.pad === 'buttons') {
        return buttons(element, x);
    }
    return element && element.dataset.pad !== 'dpad' ? [element.dataset.pad] : [];
}

function update(root) {
    const pressed = new Set([...fingers.values()].flatMap((finger) => finger.pressed));
    for (const name of Object.keys(touch)) {
        const on = pressed.has(name);
        if (on && !touch[name]) {
            navigator.vibrate?.(8);
        }
        touch[name] = on;
        root.querySelector(`[data-shows="${name}"]`)?.classList.toggle('on', on);
    }
}

/// All let go: a machine put away with a finger on it.
export function letGo(root) {
    fingers.clear();
    update(root);
}

export function attach(root) {
    root.addEventListener('pointerdown', (event) => {
        const element = document.elementFromPoint(event.clientX, event.clientY)?.closest('[data-pad]');
        const finger = { dpad: element?.dataset.pad === 'dpad' ? element : null, pressed: [] };
        finger.pressed = under(finger, event.clientX, event.clientY);
        fingers.set(event.pointerId, finger);
        update(root);
    });
    root.addEventListener('pointermove', (event) => {
        const finger = fingers.get(event.pointerId);
        if (finger) {
            finger.pressed = under(finger, event.clientX, event.clientY);
            update(root);
        }
    });
    const lift = (event) => {
        fingers.delete(event.pointerId);
        update(root);
    };
    root.addEventListener('pointerup', lift);
    root.addEventListener('pointercancel', lift);
    root.addEventListener('contextmenu', (event) => event.preventDefault());
}
