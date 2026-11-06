// What's pressed for what, on the page: as host/controls.rs has it for the
// native window, and the same rules, but a browser's own names for things,
// so that they're kept apart. `key:KeyX` is a key by its code. `pad:a` is one
// of the names padState gives a gamepad's buttons, which allow for how
// browsers differ; `pad:btn7` or `pad:axis2+` is what was pressed on it when
// it was bound, whatever that may be. A stick always steers as the D-pad does.

export const ACTIONS = ['up', 'down', 'left', 'right', 'a', 'b', 'select', 'start', 'library'];
const STORED = 'controls';

const padDefaults = { up: 'pad:padUp', down: 'pad:padDown', left: 'pad:padLeft', right: 'pad:padRight',
    a: 'pad:a', b: 'pad:b', select: 'pad:select', start: 'pad:start' };
const keyDefaults = { up: 'key:ArrowUp', down: 'key:ArrowDown', left: 'key:ArrowLeft', right: 'key:ArrowRight',
    a: 'key:KeyX', b: 'key:KeyZ', start: 'key:Enter' };

/// As the page has always had them: the first player's on the keyboard too.
function player(first) {
    const bound = Object.fromEntries(ACTIONS.map((action) => [action, []]));
    for (const [action, source] of Object.entries(padDefaults)) {
        bound[action].push(source);
    }
    if (first) {
        for (const [action, source] of Object.entries(keyDefaults)) {
            bound[action].push(source);
        }
        bound.select.push('key:ShiftRight', 'key:Tab');
    }
    return bound;
}

export const defaults = () => [player(true), player(false)];

/// What's kept, or the defaults.
export function load(storage = localStorage) {
    try {
        const kept = JSON.parse(storage.getItem(STORED));
        const good = Array.isArray(kept) && kept.length === 2
            && kept.every((bound) => ACTIONS.every((action) => Array.isArray(bound[action])));
        return good ? kept : defaults();
    } catch {
        return defaults();
    }
}

export const save = (controls, storage = localStorage) => storage.setItem(STORED, JSON.stringify(controls));

const kindOf = (source) => source.split(':')[0];

/// `source` bound to `action` for `who`: in place of the same kind bound to
/// it before, and no longer anything else of theirs.
export function bind(controls, who, action, source) {
    for (const [other, sources] of Object.entries(controls[who])) {
        controls[who][other] = sources.filter((old) => old !== source
            && !(other === action && kindOf(old) === kindOf(source)));
    }
    controls[who][action].push(source);
}

export function reset(controls, who) {
    controls[who] = defaults()[who];
}

/// Whether `action`'s held for `who`, by what `isHeld` says of each binding.
export const holds = (controls, who, action, isHeld) => controls[who][action].some(isHeld);

/// What's said of a binding.
export function said(source) {
    const [kind, name] = source.split(':');
    if (kind === 'pad') {
        const raw = /^(btn|axis)(.*)$/.exec(name);
        const word = raw ? `${raw[1] === 'btn' ? 'button ' : 'axis '}${raw[2]}` : name.replace(/^pad(?=[A-Z])/, '');
        return `Pad ${word}`;
    }
    return name.replace(/^Key|^Digit/, '').replace(/^Arrow/, '').replace(/^(.*)(Left|Right)$/, '$2 $1');
}

/// A gamepad's own buttons and axes that are pressed now, as bindings.
export function rawPressed(pad) {
    const pressed = [];
    pad.buttons.forEach((button, at) => {
        if (button.pressed) {
            pressed.push(`pad:btn${at}`);
        }
    });
    pad.axes.forEach((travel, at) => {
        if (Math.abs(travel) > 0.5) {
            pressed.push(`pad:axis${at}${travel > 0 ? '+' : '-'}`);
        }
    });
    return pressed;
}

/// Whether a gamepad's held of `source`: of `state`, what padState made of
/// `pad`, or `pad` itself.
export function padHeld(source, pad, state) {
    const name = source.slice('pad:'.length);
    const raw = /^(btn|axis)(\d+)([+-]?)$/.exec(name);
    if (!raw) {
        return Boolean(state[name]);
    }
    if (!pad) {
        return false;
    }
    const at = Number(raw[2]);
    return raw[1] === 'btn' ? Boolean(pad.buttons[at]?.pressed) : (pad.axes[at] ?? 0) * (raw[3] === '+' ? 1 : -1) > 0.5;
}
