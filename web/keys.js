// The keys of a keyboard by where they are, as a machine that takes the
// keys themselves has them: each KeyboardEvent.code's USB HID usage (the
// keyboard page's), the same numbers SDL's scancodes are, as the desktop
// app's emulator has them.

const NAMED = {
    Enter: 0x28,
    Escape: 0x29,
    Backspace: 0x2a,
    Tab: 0x2b,
    Space: 0x2c,
    Minus: 0x2d,
    Equal: 0x2e,
    BracketLeft: 0x2f,
    BracketRight: 0x30,
    Backslash: 0x31,
    IntlHash: 0x32,
    Semicolon: 0x33,
    Quote: 0x34,
    Backquote: 0x35,
    Comma: 0x36,
    Period: 0x37,
    Slash: 0x38,
    CapsLock: 0x39,
    PrintScreen: 0x46,
    ScrollLock: 0x47,
    Pause: 0x48,
    Insert: 0x49,
    Home: 0x4a,
    PageUp: 0x4b,
    Delete: 0x4c,
    End: 0x4d,
    PageDown: 0x4e,
    ArrowRight: 0x4f,
    ArrowLeft: 0x50,
    ArrowDown: 0x51,
    ArrowUp: 0x52,
    NumLock: 0x53,
    NumpadDivide: 0x54,
    NumpadMultiply: 0x55,
    NumpadSubtract: 0x56,
    NumpadAdd: 0x57,
    NumpadEnter: 0x58,
    Numpad0: 0x62,
    NumpadDecimal: 0x63,
    IntlBackslash: 0x64,
    ContextMenu: 0x65,
    IntlRo: 0x87,
    ControlLeft: 0xe0,
    ShiftLeft: 0xe1,
    AltLeft: 0xe2,
    MetaLeft: 0xe3,
    ControlRight: 0xe4,
    ShiftRight: 0xe5,
    AltRight: 0xe6,
    MetaRight: 0xe7,
};

/// The HID usage of the key at `code`, if it's one a keyboard has.
export function usageOf(code) {
    if (code in NAMED) {
        return NAMED[code];
    }
    let match = /^Key([A-Z])$/.exec(code);
    if (match) {
        return 0x04 + match[1].charCodeAt(0) - 65;
    }
    match = /^Digit(\d)$/.exec(code);
    if (match) {
        return match[1] === '0' ? 0x27 : 0x1d + Number(match[1]);
    }
    match = /^F(\d+)$/.exec(code);
    if (match && match[1] >= 1 && match[1] <= 12) {
        return 0x39 + Number(match[1]);
    }
    match = /^Numpad([1-9])$/.exec(code);
    if (match) {
        return 0x58 + Number(match[1]);
    }
    return undefined;
}
