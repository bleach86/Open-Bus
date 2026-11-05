// The page: its library of files, and a machine run on the canvas, a frame
// at a time, its picture and sound taken after each and what's pressed told
// to it. The emulator itself is the WebAssembly, src/web.rs its side.
import init, { Emulator, put_file, read_file, written_files } from './pkg/cpu_emu.js';
import { keep, forget, names, bytes } from './files.js';

const page = (id) => document.getElementById(id);
const IIE_ROMS = {
    apple2ee: ['Apple IIe Enhanced ROM Pages C0-DF - 342-0304-A - 1984.bin',
        'Apple IIe Enhanced ROM Pages E0-FF - 342-0303-A - 1984.bin',
        'Apple IIe Enhanced Video ROM - 342-0265-A - US 1983.bin'],
    apple2e: ['Apple IIe ROM Pages C0-DF - 342-0135-A - 1982.bin',
        'Apple IIe ROM Pages E0-FF - 342-0134-A - 1982.bin',
        'Apple IIe Video ROM - 342-0133-A - US 1982.bin'],
};
const DISK_II_ROMS = ['disk2-p5-341-0027.bin', 'disk2-p6-341-0028.bin'];
const kind = (name) => name.split('.').pop().toLowerCase();
const isGame = (name) => ['nes', 'zip'].includes(kind(name));
const isDisk = (name) => ['dsk', 'do', 'po', 'nib', 'woz', 'hdv', '2mg'].includes(kind(name));
const isOurs = (name) => ['srm', 'state'].includes(kind(name));

let files = [];

/// The files, in order; those the machines write hidden.
async function refresh() {
    files = (await names()).sort();
    fill(page('games'), files.filter(isGame), (name) => {
        const button = document.createElement('button');
        button.textContent = 'Start';
        button.onclick = () => start(() => Emulator.nes(name, 8));
        return button;
    });
    fill(page('disks'), files.filter(isDisk), null);
    showRoms();
}

function fill(list, items, action) {
    list.replaceChildren(...items.map((name) => {
        const item = document.createElement('li');
        if (action) {
            const label = document.createElement('span');
            label.className = 'name';
            label.textContent = name;
            item.append(label, action(name));
        } else {
            const label = document.createElement('label');
            const box = document.createElement('input');
            box.type = 'checkbox';
            box.value = name;
            label.append(box, ' ', name);
            item.append(label);
        }
        const remove = document.createElement('button');
        remove.textContent = 'Remove';
        remove.onclick = async () => {
            await forget(name);
            refresh();
        };
        item.append(remove);
        return item;
    }));
}

/// The ROMs the chosen Apple needs and hasn't got: its own, and the Disk
/// II's with disks chosen.
function missingRoms() {
    const model = page('model').value;
    const own = model === 'apple2' ? [plusRom()].filter(Boolean) : IIE_ROMS[model];
    const needed = [...own, ...DISK_II_ROMS];
    const missing = needed.filter((name) => !files.includes(name));
    if (model === 'apple2' && !plusRom()) {
        missing.unshift('a II+ ROM (.rom)');
    }
    return missing;
}

const plusRom = () => files.find((name) => kind(name) === 'rom');

function showRoms() {
    const missing = missingRoms();
    page('roms').textContent = missing.length ? `Still needs: ${missing.join(', ')}` : 'ROMs found.';
}

async function add(list) {
    page('error').textContent = '';
    for (const file of list) {
        await keep(file.name, new Uint8Array(await file.arrayBuffer()));
    }
    refresh();
}

page('add').onchange = (event) => add(event.target.files);
page('model').onchange = showRoms;
addEventListener('dragover', (event) => event.preventDefault());
addEventListener('drop', (event) => {
    event.preventDefault();
    add(event.dataTransfer.files);
});

page('apple').onclick = () => {
    const model = page('model').value;
    const chosen = [...page('disks').querySelectorAll('input:checked')].map((box) => box.value);
    const mockingboards = Number(page('mockingboards').value);
    const roms = model === 'apple2' ? plusRom() ?? '' : '';
    start(() => Emulator.apple(model, roms, chosen, mockingboards));
};

// Running a machine.
let emulator = null;
let audio = null;
let player = null;
let last = 0;
let nextSave = 0;
const held = new Set();
let reset = false;

async function startAudio() {
    if (!audio) {
        audio = new AudioContext({ sampleRate: 48000 });
        await audio.audioWorklet.addModule('sound.js');
        player = new AudioWorkletNode(audio, 'player', { outputChannelCount: [2] });
        player.connect(audio.destination);
    }
    await audio.resume();
}

async function start(make) {
    page('error').textContent = '';
    try {
        // Every file the machine might open, ours and yours.
        for (const name of await names()) {
            put_file(name, await bytes(name));
        }
        emulator = make();
    } catch (error) {
        page('error').textContent = String(error);
        return;
    }
    try {
        await startAudio();
    } catch (error) {
        // No sound where the page isn't secure; it runs all the same.
        console.warn('no sound:', error);
    }
    page('library').hidden = true;
    page('machine').hidden = false;
    const canvas = page('screen');
    canvas.style.setProperty('--ratio', emulator.shown_width() / emulator.shown_height());
    canvas.focus();
    last = performance.now();
}

async function saveWritten() {
    for (const name of written_files()) {
        await keep(name, read_file(name));
    }
}

async function stop() {
    await saveWritten();
    emulator.free();
    emulator = null;
    held.clear();
    page('machine').hidden = true;
    page('library').hidden = false;
    refresh();
}

page('back').onclick = stop;
addEventListener('pagehide', saveWritten);

/// The gamepad's and the keyboard's controls, each frame.
function controls() {
    const pad = navigator.getGamepads?.().find(Boolean);
    const button = (n) => pad?.buttons[n]?.pressed ?? false;
    const key = (...codes) => codes.some((code) => held.has(code));
    // The west button: 2 in the standard layout, but Firefox on Linux goes by
    // the kernel's button names, and an Xbox pad's X is sent as BTN_NORTH.
    const west = /Linux.*Firefox/.test(navigator.userAgent) && /x-?box/i.test(pad?.id) ? 3 : 2;
    const push = (axis) => {
        const travel = pad?.axes[axis] ?? 0;
        return Math.abs(travel) < 0.15 ? 0 : travel;
    };
    const [stickX, stickY] = [push(0), push(1)];
    const up = button(12) || stickY < -0.5 || key('ArrowUp');
    const down = button(13) || stickY > 0.5 || key('ArrowDown');
    const left = button(14) || stickX < -0.5 || key('ArrowLeft');
    const right = button(15) || stickX > 0.5 || key('ArrowRight');
    const gamepad = [up, down, left, right,
        button(0) || key('KeyX'), button(west) || key('KeyZ'),
        button(8) || key('ShiftRight', 'Tab'), button(9) || key('Enter')];
    // The joystick: the numeric keypad's too, and the Alt keys its buttons.
    const towards = (minus, plus, stick) => {
        if (minus !== plus) {
            return minus ? -1 : 1;
        }
        return stick;
    };
    const x = towards(button(14) || key('Numpad4', 'Numpad7', 'Numpad1'),
        button(15) || key('Numpad6', 'Numpad9', 'Numpad3'), stickX);
    const y = towards(button(12) || key('Numpad8', 'Numpad7', 'Numpad9'),
        button(13) || key('Numpad2', 'Numpad1', 'Numpad3'), stickY);
    const buttons = (button(0) || key('Numpad0', 'AltLeft') ? 1 : 0)
        | (button(1) || key('NumpadDecimal', 'AltRight') ? 2 : 0);
    const down_ = [...held].some((code) => !/^(Shift|Control|Alt|Meta)/.test(code));
    emulator.controls(gamepad.reduce((bits, on, bit) => bits | (on ? 1 << bit : 0), 0),
        x, y, buttons, down_, reset);
}

function show() {
    const [width, height] = [emulator.width(), emulator.height()];
    const canvas = page('screen');
    if (canvas.width !== width || canvas.height !== height) {
        canvas.width = width;
        canvas.height = height;
    }
    const image = new ImageData(new Uint8ClampedArray(emulator.pixels()), width, height);
    canvas.getContext('2d').putImageData(image, 0, 0);
    page('title').textContent = emulator.title();
}

/// As many frames as are due, at the machine's own rate, the screen's
/// whatever it is; a tab long away doesn't run to catch up.
function tick(now) {
    requestAnimationFrame(tick);
    if (!emulator) {
        return;
    }
    const period = 1000 / emulator.hz();
    if (now - last > 250) {
        last = now - period;
    }
    let ran = false;
    for (let frames = 0; frames < 3 && now - last >= period; frames++) {
        controls();
        const here = (Date.now() - new Date().getTimezoneOffset() * 60000) / 1000;
        emulator.frame(here);
        last += period;
        ran = true;
        const samples = emulator.samples();
        player?.port.postMessage(samples, [samples.buffer]);
    }
    if (ran) {
        show();
    }
    if (now > nextSave) {
        nextSave = now + 2000;
        saveWritten();
    }
}

const ARROWS = { ArrowLeft: 0, ArrowRight: 1, ArrowUp: 2, ArrowDown: 3 };

addEventListener('keydown', (event) => {
    if (!emulator) {
        return;
    }
    const code = event.code;
    if (code.startsWith('Numpad') && code !== 'NumpadEnter' || code.startsWith('Alt')) {
        held.add(code);
        event.preventDefault();
        return;
    }
    event.preventDefault();
    held.add(code);
    const number = /^F(\d+)$/.exec(code)?.[1];
    if (number) {
        if (event.repeat) {
            return;
        }
        if (code === 'F11') {
            fullscreen();
        } else if (code === 'F12') {
            reset = true;
        } else {
            emulator.function_key(Number(number));
        }
        return;
    }
    if (code in ARROWS) {
        emulator.key_arrow(ARROWS[code]);
    } else if (event.key === 'Enter') {
        emulator.key_char(13);
    } else if (event.key === 'Backspace') {
        emulator.key_char(8);
    } else if (event.key === 'Escape') {
        emulator.key_char(27);
    } else if (event.key === 'Tab') {
        emulator.key_char(9);
    } else if (event.ctrlKey && /^[a-z]$/i.test(event.key)) {
        emulator.key_char(event.key.toUpperCase().charCodeAt(0) - 64);
    } else if (event.key.length === 1 && event.key.charCodeAt(0) < 128) {
        emulator.key_char(event.key.charCodeAt(0));
    }
});

addEventListener('keyup', (event) => {
    held.delete(event.code);
    if (event.code === 'F12') {
        reset = false;
    }
});

function fullscreen() {
    if (document.fullscreenElement) {
        document.exitFullscreen();
    } else {
        page('frame').requestFullscreen();
    }
}

page('screen').ondblclick = fullscreen;

await init();
await refresh();
requestAnimationFrame(tick);
