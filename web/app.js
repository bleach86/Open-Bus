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
const isUsable = (name) => isGame(name) || isDisk(name) || isOurs(name) || ['rom', 'bin'].includes(kind(name));

let files = [];
/// The disks ticked, in the order they were: the first is in drive 1.
let chosen = [];

function say(text, problem = false) {
    page('message').textContent = text;
    page('message').classList.toggle('problem', problem);
}

/// The files, in order, those a filter hides left out; those the machines
/// write hidden.
async function refresh() {
    files = (await names()).sort();
    chosen = chosen.filter((name) => files.includes(name));
    const wanted = page('filter').value.trim().toLowerCase();
    const shown = (name) => name.toLowerCase().includes(wanted);
    fill(page('games'), files.filter(isGame).filter(shown), gameButton, wanted, 'No games yet. Add .nes or .zip files.');
    fill(page('disks'), files.filter(isDisk).filter(shown), diskBox, wanted, 'No disks yet. Add disk images.');
    showRoms();
}

function fill(list, items, main, filtered, none) {
    list.nextElementSibling.textContent = filtered ? 'Nothing matches.' : none;
    list.replaceChildren(...items.map((name) => {
        const item = document.createElement('li');
        item.append(main(name), removeButton(name));
        return item;
    }));
}

function gameButton(name) {
    const button = document.createElement('button');
    button.className = 'name';
    button.title = name;
    button.textContent = name.replace(/\.[^.]*$/, '');
    button.onclick = () => start(() => Emulator.nes(name, 8), 'nes', [name]);
    return button;
}

function diskBox(name) {
    const label = document.createElement('label');
    label.className = 'name';
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.value = name;
    box.checked = chosen.includes(name);
    box.onchange = () => {
        chosen = box.checked ? [...chosen, name] : chosen.filter((other) => other !== name);
        showRoms();
    };
    const drive = document.createElement('span');
    drive.className = 'drive';
    drive.title = 'The order the disks go in';
    label.append(box, name, drive);
    return label;
}

function removeButton(name) {
    const remove = document.createElement('button');
    remove.className = 'remove';
    remove.textContent = '×';
    remove.title = `Remove ${name}`;
    remove.setAttribute('aria-label', remove.title);
    remove.onclick = async () => {
        if (confirm(`Remove ${name} from this browser?`)) {
            await forget(name);
            refresh();
        }
    };
    return remove;
}

/// The chosen Apple's ROMs, its own and the Disk II's, each with whether
/// it's here. The Disk II's are needed for floppies, not a hard disk.
function appleRoms() {
    const model = page('model').value;
    const own = model === 'apple2' ? [plusRom() ?? 'A II+ ROM (.rom)'] : IIE_ROMS[model];
    return [...own, ...DISK_II_ROMS].map((name) => ({ name, here: files.includes(name) }));
}

const plusRom = () => files.find((name) => kind(name) === 'rom');
const isFloppy = (name) => ['dsk', 'do', 'nib', 'woz'].includes(kind(name));

function showRoms() {
    const roms = appleRoms();
    const own = roms.slice(0, -DISK_II_ROMS.length);
    const needed = chosen.some(isFloppy) ? roms : own;
    const short = needed.filter((rom) => !rom.here).length;
    page('apple').disabled = short > 0;
    page('roms').classList.toggle('short', short > 0);
    page('roms').open = short > 0;
    page('roms').querySelector('summary').textContent = short ? `Still needs ${short} ROM${short > 1 ? 's' : ''}` : `${roms.filter((rom) => rom.here).length} ROMs found`;
    page('rom-list').replaceChildren(...roms.map(({ name, here }) => {
        const item = document.createElement('li');
        item.classList.toggle('missing', !here);
        const mark = document.createElement('span');
        mark.className = 'mark';
        mark.textContent = here ? '✓' : '✗';
        const label = document.createElement('span');
        label.className = 'rom';
        label.textContent = name;
        item.append(mark, label);
        if (here) {
            item.append(removeButton(name));
        }
        return item;
    }));
    page('disks').querySelectorAll('label').forEach((label) => {
        const at = chosen.indexOf(label.querySelector('input').value);
        label.querySelector('.drive').textContent = at < 0 ? '' : at + 1;
    });
}

async function add(list) {
    const chosenFiles = [...list];
    const usable = chosenFiles.filter((file) => isUsable(file.name));
    const unused = chosenFiles.filter((file) => !isUsable(file.name));
    say('');
    try {
        for (const [at, file] of usable.entries()) {
            say(`Adding ${at + 1} of ${usable.length}…`);
            await keep(file.name, new Uint8Array(await file.arrayBuffer()));
        }
        say(unused.length ? `Not ROMs or disks, so not added: ${unused.map((file) => file.name).join(', ')}` : '', unused.length > 0);
    } catch (error) {
        say(`Couldn't add it: ${error}`, true);
    }
    refresh();
}

page('add').onchange = async (event) => {
    await add(event.target.files);
    event.target.value = '';
};
page('filter').oninput = refresh;
page('model').onchange = showRoms;
const holdingFiles = (event) => event.dataTransfer.types.includes('Files');
addEventListener('dragover', (event) => {
    event.preventDefault();
    document.body.classList.toggle('dropping', holdingFiles(event));
});
addEventListener('dragleave', (event) => {
    if (!event.relatedTarget) {
        document.body.classList.remove('dropping');
    }
});
addEventListener('drop', (event) => {
    event.preventDefault();
    document.body.classList.remove('dropping');
    add(event.dataTransfer.files);
});

page('apple').onclick = () => {
    const model = page('model').value;
    const mockingboards = Number(page('mockingboards').value);
    const roms = model === 'apple2' ? plusRom() ?? '' : '';
    const disks = chosen;
    start(() => Emulator.apple(model, roms, disks, mockingboards), 'apple', disks);
};

// Running a machine.
let emulator = null;
let audio = null;
let player = null;
let last = 0;
let nextSave = 0;
const held = new Set();
let reset = false;
let starting = false;

async function startAudio() {
    if (!audio) {
        audio = new AudioContext({ sampleRate: 48000 });
        await audio.audioWorklet.addModule('sound.js');
        player = new AudioWorkletNode(audio, 'player', { outputChannelCount: [2] });
        player.connect(audio.destination);
    }
    await audio.resume();
}

/// A machine made and run, `machine` nes or apple, `using` the games or
/// disks it's given: the saves and ROMs and those put where it can open
/// them, not the rest of the library.
async function start(make, machine, using) {
    if (starting) {
        return;
    }
    starting = true;
    say('Starting…');
    try {
        for (const name of await names()) {
            if (using.includes(name) || (!isGame(name) && !isDisk(name))) {
                put_file(name, await bytes(name));
            }
        }
        emulator = make();
    } catch (error) {
        say(String(error), true);
        return;
    } finally {
        starting = false;
    }
    let sound = true;
    try {
        await startAudio();
    } catch (error) {
        // No sound where the page isn't secure; it runs all the same.
        console.warn('no sound:', error);
        sound = false;
    }
    say('');
    page('no-sound').hidden = sound;
    page('library').hidden = true;
    page('machine').hidden = false;
    page('machine').dataset.machine = machine;
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
    document.title = 'cpu_emu';
    refresh();
}

page('back').onclick = stop;

/// A button on the bar, the keys going back to the picture after it.
function press(id, action) {
    page(id).onclick = () => {
        action();
        page('screen').focus();
    };
}

press('save', () => emulator.function_key(2));
press('load', () => emulator.function_key(4));
press('full', () => fullscreen());
page('reset').onpointerdown = () => {
    reset = true;
};
page('reset').onpointerup = page('reset').onpointerleave = () => {
    reset = false;
    page('screen').focus();
};
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
    const title = emulator.title();
    if (title !== page('title').textContent) {
        page('title').textContent = title;
        document.title = `${title} - cpu_emu`;
    }
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
