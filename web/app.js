// The page: its library of files, and a machine run on the canvas, a frame
// at a time, its picture and sound taken after each and what's pressed told
// to it. The emulator itself is the WebAssembly, src/web.rs its side.
import { keep, keepParts, forget, names, bytes, allArt, native } from './files.js';
import { touch, attach, letGo } from './touch.js';
import { games, isVolume, storedNames, shortName, natural } from './disks.js';
import { findCovers } from './covers.js';
import * as bindings from './controls.js';
import { artKey, isMedia, keyOf, plainTitle, tagsOf, takeArt, titleWords } from './art.js';

/// The emulator's WebAssembly: the browser's only, the desktop app's games
/// being the native emulator's, which sorts its ROMs too.
const { default: init, Emulator, RomSorter, put_file, read_file, read_part, written_files, changed_files, take_changed_parts } = native ? {} : await import('./pkg/cpu_emu.js');

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
/// The function key that changes a Disk System's disk; its BIOS's name.
const CHANGE_DISK_KEY = 9;
const FDS_BIOS = 'disksys.rom';
const kind = (name) => name.split('.').pop().toLowerCase();
const isGame = (name) => ['nes', 'fds', 'zip'].includes(kind(name));
const isDisk = (name) => ['dsk', 'do', 'po', 'nib', 'woz', 'hdv', '2mg'].includes(kind(name));
const isOurs = (name) => ['srm', 'state'].includes(kind(name));
const isRom = (name) => kind(name) === 'rom' || DISK_II_ROMS.includes(name) || Object.values(IIE_ROMS).flat().includes(name);
const isUsable = (name) => isGame(name) || isDisk(name) || isOurs(name) || isRom(name);
/// The most a file can be and hold a ROM, a zip's and any other's; and what
/// isn't worth looking in.
const MOST_ZIP = 0x400000;
const MOST = 0x10000;
const notRoms = /\.(png|jpe?g|gif|mp4|xml|txt|nfo|pdf|srm|state|nes|fds)$/i;
const baseName = (path) => path.split(/[/\\]/).pop();
const mayHoldRoms = (file) => !notRoms.test(file.name) && file.size <= (kind(file.name) === 'zip' ? MOST_ZIP : MOST);

let files = [];
/// The disks ticked, in the order they were: the first is in drive 1.
let chosen = [];
/// The games of several disks open, by their titles.
const opened = new Set();
/// What's bound; and in the dialog, whose are showing, the action waiting
/// for a press, and what a gamepad had held when it began.
const controlsBound = bindings.load();
let controlsPlayer = 0;
let awaiting = null;
let heldBefore = new Set();

/// How many covers come between showing them.
const SHOWN_EVERY = 24;
/// The shelf's sizes, in rem, and a cover's height as a share of its width,
/// as the CSS has them; and rows of tiles kept either side of those in view.
const SMALLEST_TILE = 9;
const GAP_ACROSS = 0.9;
const GAP_DOWN = 1.1;
const CAPTION = 3.7;
const COVER_SHAPE = 680 / 497;
const SHELF_EXTRA = 2;
/// How long a message that isn't a problem stays, in milliseconds.
const SHOWN_FOR = 6000;
let saidTimer = 0;
/// The art there is, by key, and its covers' addresses.
let art = new Map();
const covers = new Map();
/// The NES covers there are by their titles' words, the USA's if there's a
/// choice, for a game named differently to its art. Not the Apple's: a
/// game's on both, and one's picture isn't the other's.
let coversByTitle = new Map();

/// A message, in a toast that's there wherever the page is scrolled to. A
/// problem stays until it's dismissed, and something under way until it's
/// replaced; the rest go by themselves.
function say(text, problem = false) {
    clearTimeout(saidTimer);
    const message = page('message');
    message.hidden = !text;
    message.classList.toggle('problem', problem);
    message.setAttribute('role', problem ? 'alert' : 'status');
    message.querySelector('span').textContent = text;
    if (text && !problem && !text.endsWith('…')) {
        saidTimer = setTimeout(() => say(''), SHOWN_FOR);
    }
}
page('dismiss').onclick = () => say('');

const padsNow = () => [...(navigator.getGamepads?.() ?? [])].filter(Boolean);

/// The key hints under the game, as the keys are bound.
function showControlHelp() {
    const lines = [['up', 'Up'], ['down', 'Down'], ['left', 'Left'], ['right', 'Right'], ['a', 'A'], ['b', 'B'], ['start', 'Start'], ['select', 'Select']];
    page('control-help').replaceChildren(...lines.flatMap(([action, label]) => {
        const source = controlsBound[0][action].find((bound) => bound.startsWith('key:'));
        if (!source) {
            return [];
        }
        const key = document.createElement('kbd');
        key.textContent = bindings.said(source);
        return [key, ` ${label} `];
    }));
}

/// The dialog's rows, as they're bound now.
function fillControls() {
    page('control-rows').replaceChildren(...bindings.ACTIONS.map((action) => {
        const item = document.createElement('li');
        const words = document.createElement('span');
        words.className = 'row-text';
        words.textContent = action[0].toUpperCase() + action.slice(1);
        const note = document.createElement('small');
        note.textContent = controlsBound[controlsPlayer][action].map(bindings.said).join(', ') || 'Nothing';
        words.append(note);
        const change = document.createElement('button');
        change.textContent = awaiting === action ? 'Press something…' : 'Change';
        change.onclick = () => {
            awaiting = action;
            heldBefore = new Set(padsNow().flatMap(bindings.rawPressed));
            page('control-status').textContent = 'Press a key or gamepad button. Esc cancels.';
            fillControls();
        };
        item.append(words, change);
        return item;
    }));
    for (const who of [0, 1]) {
        page(`controls-player-${who + 1}`).classList.toggle('on', who === controlsPlayer);
    }
}

function bindAwaited(source) {
    bindings.bind(controlsBound, controlsPlayer, awaiting, source);
    bindings.save(controlsBound);
    page('control-status').textContent = `${awaiting[0].toUpperCase()}${awaiting.slice(1)} is now ${bindings.said(source)}.`;
    awaiting = null;
    showControlHelp();
    fillControls();
}

/// A gamepad's anything newly pressed, while one's being waited for.
function lookForPress() {
    if (awaiting) {
        const pressed = padsNow().flatMap(bindings.rawPressed).find((source) => !heldBefore.has(source));
        if (pressed) {
            bindAwaited(pressed);
        }
    }
}

function openControls() {
    held.clear();
    awaiting = null;
    page('control-status').textContent = '';
    fillControls();
    page('controls-dialog').showModal();
}

page('open-controls').onclick = openControls;
page('game-controls').onclick = openControls;
page('controls-done').onclick = () => page('controls-dialog').close();
page('controls-reset').onclick = () => {
    bindings.reset(controlsBound, controlsPlayer);
    bindings.save(controlsBound);
    awaiting = null;
    page('control-status').textContent = 'Back to the defaults.';
    showControlHelp();
    fillControls();
};
for (const who of [0, 1]) {
    page(`controls-player-${who + 1}`).onclick = () => {
        controlsPlayer = who;
        awaiting = null;
        fillControls();
    };
}
// Esc takes back a press being waited for, not the dialog.
page('controls-dialog').addEventListener('cancel', (event) => {
    if (awaiting) {
        event.preventDefault();
    }
});
addEventListener('keydown', (event) => {
    if (!page('controls-dialog').open) {
        return;
    }
    // Nothing reaches a paused machine.
    event.stopImmediatePropagation();
    if (!awaiting) {
        return;
    }
    event.preventDefault();
    if (event.code === 'Escape') {
        awaiting = null;
        page('control-status').textContent = '';
        fillControls();
    } else if (!/^F\d+$/.test(event.code)) {
        bindAwaited(`key:${event.code}`);
    }
}, true);
showControlHelp();

/// The art kept, and where its covers can be shown from.
async function loadArt() {
    art = await allArt();
    for (const [key, { cover }] of art) {
        if (cover && !covers.has(key)) {
            covers.set(key, URL.createObjectURL(cover));
        }
    }
    coversByTitle = new Map();
    for (const [key, { cover, machine }] of art) {
        const words = titleWords(key);
        if (cover && machine !== 'apple' && (!coversByTitle.has(words) || key.includes('(usa'))) {
            coversByTitle.set(words, key);
        }
    }
}

/// Which shelf's showing.
function showShelf(name) {
    localStorage.setItem('shelf', name);
    page('nes').hidden = name !== 'nes';
    page('apple-shelf').hidden = name !== 'apple';
    page('settings').hidden = name !== 'settings';
    page('tools').hidden = name === 'settings';
    if (name === 'settings') {
        fillSettings();
    }
    placeShelf();
    for (const tab of document.querySelectorAll('.tabs button')) {
        tab.classList.toggle('on', tab.dataset.tab === name);
    }
}

for (const tab of document.querySelectorAll('.tabs button')) {
    tab.onclick = () => showShelf(tab.dataset.tab);
}

/// The files, in order, those a filter hides left out; those the machines
/// write hidden.
async function refresh() {
    files = (await names()).sort(natural);
    chosen = chosen.filter((name) => files.includes(name));
    const wanted = page('filter').value.trim().toLowerCase();
    fillGames(wanted);
    fillDisks(wanted);
    showRoms();
    page('find').hidden = !coverless().length;
}

/// The games with no cover, as `findCovers` wants them.
function coverless() {
    const nes = files.filter(isGame).map((file) => ({
        machine: kind(file) === 'fds' ? 'fds' : 'nes', file, title: plainTitle(file), key: nesArt(file),
    }));
    const apple = games(files.filter(isDisk)).map((game) => {
        const title = diskTitle(game);
        return { machine: 'apple', file: game.disks[0], title, key: artFor(title, ...game.disks) };
    });
    return [...nes, ...apple].filter(({ key }) => !covers.has(key));
}

/// Covers found online, those found showing as they come.
async function findMissing() {
    const button = page('find');
    button.disabled = true;
    say('Asking for the lists of covers…');
    const { found, missed, problems } = await findCovers(coverless(), art, async (done, total, kept) => {
        say(`Finding covers: ${done} of ${total}, ${kept} found…`);
        if (done % SHOWN_EVERY === 0) {
            await loadArt();
            await refresh();
        }
    });
    await loadArt();
    await refresh();
    button.disabled = false;
    const stopped = problems.map((problem) => `Stopped: ${problem}. What's found is kept.`);
    say([`Found ${found} cover${found === 1 ? '' : 's'}${missed ? `, none for ${missed}` : ''}.`, ...stopped].join(' '), problems.length > 0);
}

page('find').onclick = findMissing;

/// Whose art a game has: that of its file, or of any of them, or of its
/// title, for a picture named after it.
const artFor = (title, ...names) => [...names.map(artKey), keyOf(title)].find((key) => art.has(key)) ?? artKey(names[0]);

/// What's said of an empty shelf.
const noGames = native ? 'No games yet. Add a folder of them in Settings.' : 'No games yet. Add .nes, .fds or .zip files, or a folder of them.';

/// Whether what's searched for is in a game's title, file, or what's known
/// of it.
const found = (wanted, title, file, known) => [title, file, known?.developer, known?.genre, known?.year].join(' ').toLowerCase().includes(wanted);

/// `items` as the sort says, by title among those alike, those not known
/// about last.
function arranged(items, titleOf, knownOf) {
    const way = page('sort').value;
    const field = { newest: 'year', oldest: 'year', genre: 'genre', developer: 'developer', players: 'players' }[way];
    const byTitle = (a, b) => natural(titleOf(a), titleOf(b));
    return [...items].sort((a, b) => {
        const [first, second] = [knownOf(a)?.[field], knownOf(b)?.[field]];
        if (!field || first === second) {
            return byTitle(a, b);
        }
        if (!first || !second) {
            return first ? -1 : 1;
        }
        return way === 'newest' ? natural(second, first) : natural(first, second);
    });
}

page('sort').value = localStorage.getItem('sort') ?? 'title';
page('sort').onchange = () => {
    localStorage.setItem('sort', page('sort').value);
    refresh();
};

/// A tab's name, and how many it holds beside it.
function showCount(tab, name, count) {
    const tally = document.createElement('span');
    tally.className = 'tally';
    tally.textContent = count;
    page(tab).replaceChildren(name, ...(count ? [tally] : []));
}

/// Whose art a NES game has: its own, or one of its title's.
function nesArt(name) {
    const key = artFor(plainTitle(name), name);
    return covers.has(key) ? key : coversByTitle.get(titleWords(plainTitle(name))) ?? key;
}

/// A game's title, by what's known of it, else its file.
const titleOf = (name) => art.get(nesArt(name))?.title ?? plainTitle(name);

function fillGames(wanted) {
    const all = files.filter(isGame);
    const knownOf = (name) => art.get(nesArt(name));
    const matches = (name) => found(wanted, titleOf(name), name, knownOf(name));
    const shown = arranged(all.filter(matches), titleOf, knownOf);
    const list = page('games');
    list.nextElementSibling.textContent = wanted ? 'Nothing matches.' : noGames;
    showCount('tab-nes', 'NES', shown.length);
    shelf = shown;
    tiles.clear();
    placeShelf();
}

/// A game's tile.
function gameTile(name) {
    const item = document.createElement('li');
    item.className = 'tile';
    const key = nesArt(name);
    const known = art.get(key);
    const button = document.createElement('button');
    button.className = 'play';
    button.title = [titleOf(name), known?.developer, name].filter(Boolean).join('\n');
    button.onclick = () => (native
        ? playNatively({ kind: 'nes', game: name })
        : start(() => Emulator.nes(name, 8), 'nes', [name]));
    button.append(cover(key, titleOf(name)), caption(titleOf(name), known, tagsOf(name)));
    item.append(button, removeButton(name));
    return item;
}

/// The shelf of games, however long, is only as many tiles as are in view
/// and a few rows either side, the space for the rest being kept: hundreds
/// of them take a browser seconds to draw, and make scrolling heavy.
let shelf = [];
const tiles = new Map();
let shelfRow = 0;
let shelfPlaced = -1;

/// A row's height, and how many tiles are in one: as the CSS's grid has it.
function shelfShape(list) {
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
    const [smallest, across, down, words] = [SMALLEST_TILE, GAP_ACROSS, GAP_DOWN, CAPTION].map((size) => size * rem);
    const width = list.clientWidth;
    const columns = Math.max(1, Math.floor((width + across) / (smallest + across)));
    const tile = (width - across * (columns - 1)) / columns;
    return { columns, row: tile * COVER_SHAPE + words, down };
}

/// Gamepad or arrow keys around the library. A selection, by name on the
/// shelf of games, by place among the Apple's tiles; held directions repeat.
/// Nothing's taken until all's let go, coming back from a game, so that the
/// A that ended it, say, doesn't start another.
let selectedGame = null;
let selectedDisk = -1;
let moving = '';
let moveAgainAt = 0;
let wasPressed = {};
let navigationWaits = false;
let padsSaid = null;
const FIRST_REPEAT = 350;
const NEXT_REPEAT = 110;

const appleShowing = () => !page('apple-shelf').hidden;
const gamesShowing = () => !page('nes').hidden;

/// What's pressed for moving around, on any gamepad, as the first player's
/// are bound. In the app, what the emulator says, which it hasn't yet, just
/// back from a game.
function navigationPressed() {
    if (native) {
        return padsSaid;
    }
    const pressed = {};
    const shoulders = (pad, button) => Boolean(pad.buttons[button]?.pressed);
    for (const pad of padsNow()) {
        const state = padState(pad);
        const steered = { up: state.stickY < -0.5, down: state.stickY > 0.5, left: state.stickX < -0.5, right: state.stickX > 0.5 };
        for (const action of ['up', 'down', 'left', 'right', 'a', 'b', 'start']) {
            pressed[action] ||= steered[action] || boundHeld(0, action, pad, state);
        }
        pressed.previous ||= shoulders(pad, 4);
        pressed.next ||= shoulders(pad, 5);
    }
    return pressed;
}

/// The selection shown, where the tiles are.
function showSelected() {
    document.querySelectorAll('.tile.selected').forEach((tile) => tile.classList.remove('selected'));
    tiles.get(selectedGame)?.classList.add('selected');
    document.querySelectorAll('#disks .tile')[selectedDisk]?.classList.add('selected');
}

/// The selection on the shelf of games moved, and scrolled to.
function moveInGames(way) {
    const list = page('games');
    const { columns, row, down } = shelfShape(list);
    const at = shelf.indexOf(selectedGame);
    if (at < 0) {
        // The first press takes the first tile in view.
        selectedGame = shelf[Math.min(shelfRow, Math.ceil(shelf.length / columns) - 1) * columns] ?? shelf[0];
    } else {
        const step = { up: -columns, down: columns, left: -1, right: 1 }[way];
        selectedGame = shelf[Math.min(shelf.length - 1, Math.max(0, at + step))];
    }
    const height = row + down;
    const rowAt = Math.floor(shelf.indexOf(selectedGame) / columns);
    const top = list.getBoundingClientRect().top + scrollY + rowAt * height;
    const toolbar = document.querySelector('.toolbar').getBoundingClientRect().height;
    if (top < scrollY + toolbar) {
        scrollTo(0, top - toolbar - down);
    } else if (top + row > scrollY + innerHeight) {
        scrollTo(0, top + row + down - innerHeight);
    }
    placeShelf();
    showSelected();
}

function moveInDisks(way) {
    const all = [...document.querySelectorAll('#disks .tile')];
    if (!all.length) {
        return;
    }
    const columns = getComputedStyle(page('disks')).gridTemplateColumns.split(' ').length;
    const step = { up: -columns, down: columns, left: -1, right: 1 }[way];
    selectedDisk = selectedDisk < 0 ? 0 : Math.min(all.length - 1, Math.max(0, selectedDisk + step));
    showSelected();
    all[selectedDisk].scrollIntoView({ block: 'nearest' });
}

const move = (way) => (gamesShowing() ? moveInGames(way) : appleShowing() && moveInDisks(way));

/// A press of confirm: the selected game started, or disk ticked.
function confirmSelected() {
    if (gamesShowing()) {
        tiles.get(selectedGame)?.querySelector('.play').click();
    } else if (appleShowing()) {
        document.querySelectorAll('#disks .tile')[selectedDisk]?.querySelector('label').click();
    }
}

function nextTab(step) {
    const shelves = [...document.querySelectorAll('.tabs button')].filter((tab) => !tab.hidden);
    const at = shelves.findIndex((tab) => tab.classList.contains('on'));
    shelves[(at + step + shelves.length) % shelves.length].click();
}

/// A frame of the library: what's newly pressed acted on.
function navigate(now) {
    if (page('library').hidden || !document.hasFocus()) {
        return;
    }
    const pressed = navigationPressed();
    if (!pressed) {
        return;
    }
    if (navigationWaits) {
        navigationWaits = Object.values(pressed).some(Boolean);
        wasPressed = pressed;
        return;
    }
    const way = ['up', 'down', 'left', 'right'].find((direction) => pressed[direction]) ?? '';
    if (way !== moving) {
        moveAgainAt = now + FIRST_REPEAT;
        moving = way;
        if (way) {
            move(way);
        }
    } else if (way && now >= moveAgainAt) {
        moveAgainAt = now + NEXT_REPEAT;
        move(way);
    }
    const newly = (name) => pressed[name] && !wasPressed[name];
    if (newly('a')) {
        navigationWaits = true;
        confirmSelected();
    } else if (newly('b')) {
        const search = page('filter');
        if (search.value) {
            search.value = '';
            refresh();
        }
    } else if (newly('start') && appleShowing()) {
        navigationWaits = true;
        page('apple').click();
    } else if (newly('previous')) {
        nextTab(-1);
    } else if (newly('next')) {
        nextTab(1);
    }
    wasPressed = pressed;
}

addEventListener('keydown', (event) => {
    const typing = /^(INPUT|SELECT|TEXTAREA)$/.test(document.activeElement?.tagName);
    if (emulator || typing || page('library').hidden || page('controls-dialog').open) {
        return;
    }
    const way = { ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right' }[event.code];
    if (way) {
        event.preventDefault();
        move(way);
    } else if (event.code === 'Enter' && (selectedGame || selectedDisk >= 0) && document.activeElement === document.body) {
        event.preventDefault();
        confirmSelected();
    }
});

/// The tiles for where the page is scrolled to.
function placeShelf(force = true) {
    const list = page('games');
    if (!list.clientWidth) {
        return;
    }
    const { columns, row, down } = shelfShape(list);
    const rows = Math.ceil(shelf.length / columns);
    const scrolled = Math.max(0, -list.getBoundingClientRect().top);
    const first = Math.max(0, Math.floor(scrolled / (row + down)) - SHELF_EXTRA);
    const count = Math.ceil(innerHeight / (row + down)) + 2 * SHELF_EXTRA + 1;
    if (!force && first === shelfRow && shelfPlaced === columns) {
        return;
    }
    [shelfRow, shelfPlaced] = [first, columns];
    list.style.setProperty('--row', `${row}px`);
    list.style.paddingTop = `${first * (row + down)}px`;
    list.style.minHeight = `${Math.max(0, rows * (row + down) - down)}px`;
    const inView = shelf.slice(first * columns, (first + count) * columns);
    list.replaceChildren(...inView.map((name) => {
        if (!tiles.has(name)) {
            tiles.set(name, gameTile(name));
        }
        return tiles.get(name);
    }));
    showSelected();
}

let shelfDue = false;
const placeShelfSoon = () => {
    if (!shelfDue) {
        shelfDue = true;
        requestAnimationFrame(() => {
            shelfDue = false;
            placeShelf(false);
        });
    }
};
addEventListener('scroll', placeShelfSoon, { passive: true });
addEventListener('resize', placeShelfSoon);

/// The picture of a game: its cover, or one made of its title, on a drawing
/// of `media` if it's on any.
function cover(key, title, media = '') {
    const url = covers.get(key);
    if (url) {
        const image = document.createElement('img');
        image.className = 'art';
        image.src = url;
        image.alt = '';
        image.loading = 'lazy';
        image.decoding = 'async';
        return image;
    }
    const blank = document.createElement('span');
    blank.className = `art blank ${media}`;
    const label = document.createElement('span');
    label.className = 'label';
    label.textContent = title;
    blank.append(label);
    const hue = [...title].reduce((sum, char) => (sum * 31 + char.charCodeAt(0)) % 360, 7);
    blank.style.setProperty('--hue', 180 + hue % 170);
    return blank;
}

/// Its title under it, and what's known: the year, the genre, the players;
/// if nothing is, `note`, what tells it from others of the title.
function caption(title, known, note = '') {
    const words = document.createElement('span');
    words.className = 'caption';
    const name = document.createElement('span');
    name.className = 'tile-title';
    name.textContent = title;
    words.append(name);
    const players = known?.players && `${known.players.replace('-', '–')}P`;
    const facts = [known?.year, known?.genre, players].filter(Boolean).join(' · ') || note;
    if (facts) {
        const line = document.createElement('span');
        line.className = 'facts';
        line.textContent = facts;
        words.append(line);
    }
    return words;
}

/// The disks, as games: one tile each, a game of several able to open.
function fillDisks(wanted) {
    const knownOf = (game) => art.get(artFor(diskTitle(game), ...game.disks));
    const matches = (game) => found(wanted, game.title, game.disks.join(' '), knownOf(game));
    const all = games(files.filter(isDisk));
    const shown = arranged(all.filter(matches), (game) => knownOf(game)?.title ?? diskTitle(game), knownOf);
    const list = page('disks');
    list.nextElementSibling.textContent = wanted ? 'Nothing matches.' : 'No disks yet. Add disk images, or a folder of them.';
    showCount('tab-apple', 'Apple II', shown.length);
    list.replaceChildren(...shown.flatMap((game) => {
        const open = game.disks.length > 1 && (opened.has(game.title) || wanted);
        return open ? [diskTile(game, wanted), diskList(game)] : [diskTile(game, wanted)];
    }));
}

/// What tells a disk from others of the title: its brackets, or its kind.
const diskNote = (name) => tagsOf(name) || name.split('.').pop().toLowerCase();

/// What a game of disks is called.
const diskTitle = (game) => (game.disks.length > 1 ? game.title : plainTitle(game.title));

/// A game of disks: ticked, all of them go in, in order.
function diskTile(game, wanted) {
    const several = game.disks.length > 1;
    const title = diskTitle(game);
    const item = document.createElement('li');
    item.className = 'tile';
    const label = document.createElement('label');
    label.className = 'play';
    label.title = several ? title : game.disks[0];
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.className = 'unseen';
    if (several) {
        box.dataset.game = JSON.stringify(game.disks);
        box.onchange = () => {
            const others = chosen.filter((name) => !game.disks.includes(name));
            chosen = box.checked ? [...others, ...game.disks] : others;
            showRoms();
        };
    } else {
        box.value = game.disks[0];
        box.onchange = () => {
            chosen = box.checked ? [...chosen, game.disks[0]] : chosen.filter((other) => other !== game.disks[0]);
            showRoms();
        };
    }
    const drive = document.createElement('span');
    drive.className = 'drive';
    drive.title = 'The order the disks go in';
    const key = artFor(title, ...game.disks);
    const media = game.disks.every(isVolume) ? 'hard' : 'floppy';
    label.append(box, cover(key, title, media), caption(art.get(key)?.title ?? title, art.get(key), several ? '' : diskNote(game.disks[0])), drive);
    item.append(label);
    if (several) {
        const toggle = document.createElement('button');
        const open = opened.has(game.title) || wanted;
        toggle.className = 'opener';
        toggle.textContent = `${game.disks.length} disks ${open ? '▴' : '▾'}`;
        toggle.setAttribute('aria-expanded', Boolean(open));
        toggle.onclick = () => {
            if (!opened.delete(game.title)) {
                opened.add(game.title);
            }
            refresh();
        };
        item.append(toggle);
        const remove = removeButton(game.disks[0]);
        remove.title = `Remove ${game.title}`;
        remove.setAttribute('aria-label', remove.title);
        remove.onclick = async () => {
            if (confirm(`Remove ${game.title} and its ${game.disks.length} disks from ${native ? 'the library' : 'this browser'}?`)) {
                for (const disk of game.disks) {
                    await forget(disk);
                }
                refresh();
            }
        };
        item.append(remove);
    } else {
        item.append(removeButton(game.disks[0]));
    }
    return item;
}

/// A game's disks, opened, across the shelf.
function diskList(game) {
    const item = document.createElement('li');
    item.className = 'opened';
    const list = document.createElement('ul');
    list.replaceChildren(...game.disks.map((name) => {
        const row = document.createElement('li');
        row.append(diskBox(name), removeButton(name));
        return row;
    }));
    item.append(list);
    return item;
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
    label.append(box, shortName(name), drive);
    return label;
}

function removeButton(name) {
    const remove = document.createElement('button');
    remove.className = 'remove';
    remove.textContent = '×';
    remove.title = `Remove ${shortName(name)}`;
    remove.setAttribute('aria-label', remove.title);
    remove.onclick = async () => {
        if (confirm(`Remove ${shortName(name)} from ${native ? 'the library' : 'this browser'}?`)) {
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

const plusRom = () => files.find((name) => kind(name) === 'rom' && name !== FDS_BIOS);
const isFloppy = (name) => ['dsk', 'do', 'nib', 'woz'].includes(kind(name));

const plural = (count) => `${count} ROM${count === 1 ? '' : 's'} found`;

function showRoms() {
    const roms = appleRoms();
    const own = roms.slice(0, -DISK_II_ROMS.length);
    const needed = chosen.some(isFloppy) ? roms : own;
    const short = needed.filter((rom) => !rom.here).length;
    page('apple').disabled = short > 0;
    page('roms').classList.toggle('short', short > 0);
    page('roms').open = short > 0;
    page('roms').querySelector('summary').textContent = short ? `Still needs ${short} ROM${short > 1 ? 's' : ''}` : plural(roms.filter((rom) => rom.here).length);
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
    const disksChosen = chosen.length;
    page('picked').textContent = disksChosen ? `${disksChosen} disk${disksChosen > 1 ? 's' : ''} chosen` : 'No disks chosen';
    page('disks').querySelectorAll('label').forEach((label) => {
        const box = label.querySelector('input');
        const drive = label.querySelector('.drive');
        if (box.dataset.game) {
            const disks = JSON.parse(box.dataset.game);
            const places = disks.map((disk) => chosen.indexOf(disk)).filter((at) => at >= 0);
            box.checked = places.length === disks.length;
            box.indeterminate = places.length > 0 && !box.checked;
            label.classList.toggle('some', box.indeterminate);
            drive.textContent = places.length ? `${Math.min(...places) + 1}–${Math.max(...places) + 1}`.replace(/^(\d+)–\1$/, '$1') : '';
            return;
        }
        box.checked = chosen.includes(box.value);
        const at = chosen.indexOf(box.value);
        drive.textContent = at < 0 ? '' : at + 1;
    });
}

/// Files added: `list`, chosen one by one, replacing any of the name; and
/// those `inFolders` that are new, a folder never replacing what's here,
/// whose saves and disks may be newer than the folder's. `sorted`, in the
/// app, what's been taken out of them already: its `roms`, and the II+'s
/// `chips`.
async function add(given, dropped = [], sorted = null) {
    // Covers and game lists apart, and what's no use, as videos are.
    const artFiles = [...given, ...dropped].filter((file) => isMedia(file.name));
    const list = [...given].filter((file) => !isMedia(file.name));
    const inFolders = dropped.filter((file) => !isMedia(file.name));
    // The Apple's ROMs out of it first, whatever they're called.
    const sorter = sorted ? null : new RomSorter();
    const isOneOfThem = async (file) => sorter && mayHoldRoms(file)
        && sorter.add(file.name, new Uint8Array(await file.arrayBuffer()));
    const notRoms = async (files) => {
        const rest = [];
        for (const file of files) {
            if (!await isOneOfThem(file)) {
                rest.push(file);
            }
        }
        return rest;
    };
    say('Looking through them…');
    const [loose, others] = [await notRoms([...list]), await notRoms(inFolders)];
    const usable = loose.filter((file) => isUsable(file.name));
    const unused = loose.filter((file) => !isUsable(file.name));
    const here = new Set([...files, ...usable.map((file) => file.name)]);
    const fromFolders = others.filter((file) => isUsable(file.name));
    // What each's stored as: in its game's folder, if it's in one; and
    // copies in a folder under a game's left out.
    const stored = storedNames(fromFolders.map((file) => file.path), (path) => isDisk(path));
    const nameOf = (file) => (file.path === undefined ? file.name : stored.get(file.path));
    const added = [];
    const repeated = [];
    for (const file of fromFolders.filter((file) => nameOf(file) !== null && !here.has(nameOf(file)))) {
        const repeat = added.some((other) => nameOf(other) === nameOf(file));
        (repeat ? repeated : added).push(file);
    }
    const copies = fromFolders.filter((file) => nameOf(file) === null);
    usable.push(...added);
    say('');
    try {
        for (const [at, file] of usable.entries()) {
            say(`Adding ${at + 1} of ${usable.length}…`);
            await keep(nameOf(file), new Uint8Array(await file.arrayBuffer()));
        }
        const notes = [];
        const loose = usable.length - added.length;
        if (loose) {
            notes.push(`Added ${loose} file${loose > 1 ? 's' : ''}.`);
        }
        const roms = sorted?.roms ?? sorter.names();
        for (const name of sorted ? [] : roms) {
            await keep(name, sorter.bytes(name));
        }
        if (roms.length) {
            notes.push(`Found ${roms.length} system ROM${roms.length > 1 ? 's' : ''}.`);
        }
        const chips = sorted?.chips ?? sorter.plus_chips();
        if (chips > 0 && chips < 6) {
            notes.push(`Found ${chips} of the II+'s 6 chips; add them all at once.`);
        }
        const made = await takeArt(artFiles, art, (at, total) => say(`Making covers: ${at} of ${total}…`));
        await loadArt();
        if (made) {
            notes.push(`Added ${made} cover${made > 1 ? 's' : ''}.`);
        }
        if (others.length && fromFolders.length) {
            const already = fromFolders.length - added.length - repeated.length - copies.length;
            const other = others.length - fromFolders.length + copies.length;
            notes.push(`Added ${added.length} from the folder, ${already} already here, ${other} not ROMs or disks.`);
        }
        if (repeated.length) {
            const names = [...new Set(repeated.map((file) => file.name))].join(', ');
            notes.push(`Kept only the copy nearest the top of: ${names}`);
        }
        if (unused.length) {
            notes.push(`Not recognised, so not added: ${unused.map((file) => file.name).join(', ')}`);
        }
        sorter?.free();
        say(notes.join(' '), unused.length + repeated.length > 0);
    } catch (error) {
        say(`Couldn't add it: ${error}`, true);
    }
    refresh();
}

const fileOf = (entry) => new Promise((resolve, reject) => entry.file(resolve, reject));

/// The files in a dropped folder, then those in the folders under it, so
/// that of two of a name the one nearer the top comes first. A folder's
/// entries come a batch at a time, until a batch is empty.
async function folderFiles(folder) {
    const reader = folder.createReader();
    const found = [];
    const folders = [];
    for (;;) {
        const entries = await new Promise((resolve, reject) => reader.readEntries(resolve, reject));
        if (!entries.length) {
            break;
        }
        for (const entry of entries) {
            if (entry.isDirectory) {
                folders.push(entry);
            } else {
                const file = await fileOf(entry);
                file.path = entry.fullPath;
                found.push(file);
            }
        }
    }
    for (const under of folders) {
        found.push(...await folderFiles(under));
    }
    return found;
}

async function addDropped(entries) {
    const loose = [];
    const inFolders = [];
    say('Reading the folder…');
    try {
        for (const entry of entries) {
            if (entry.isDirectory) {
                inFolders.push(...await folderFiles(entry));
            } else {
                loose.push(await fileOf(entry));
            }
        }
    } catch (error) {
        say(`Couldn't read it: ${error}`, true);
        return;
    }
    add(loose, inFolders);
}

page('add').onchange = async (event) => {
    await add(event.target.files);
    event.target.value = '';
};
/// A folder chosen rather than dropped: its files by their paths, as a
/// dropped one's are, those nearest the top first.
page('add-folder').onchange = async (event) => {
    const depth = (file) => file.path.split('/').length;
    const inFolder = [...event.target.files];
    for (const file of inFolder) {
        file.path = `/${file.webkitRelativePath}`;
    }
    await add([], inFolder.sort((a, b) => depth(a) - depth(b)));
    event.target.value = '';
};
page('add-folder-button').hidden = !('webkitdirectory' in HTMLInputElement.prototype);
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
    // The entries go when the drop's over, so they're taken before anything's awaited.
    addDropped([...event.dataTransfer.items].map((item) => item.webkitGetAsEntry()).filter(Boolean));
});

page('apple').onclick = () => {
    const model = page('model').value;
    const mockingboards = Number(page('mockingboards').value);
    const roms = model === 'apple2' ? plusRom() ?? '' : '';
    const disks = chosen;
    if (native) {
        playNatively({ kind: 'apple', model, roms, disks, mockingboards });
    } else {
        start(() => Emulator.apple(model, roms, disks, mockingboards), 'apple', disks);
    }
};

/// In the desktop app, a game's on the native emulator, not here: a game
/// over, what it's saved is in the library, and what went wrong is told.
function playNatively(machine) {
    if (machine.game && kind(machine.game) === 'fds' && !files.includes(FDS_BIOS)) {
        say(`Disk System games need the BIOS, ${FDS_BIOS}: add it in Settings.`, true);
        return;
    }
    const fullscreen = machine.kind !== 'controls' && page('start-fullscreen').checked;
    native.play({ ...machine, fullscreen }).catch((problem) => say(String(problem), true));
}
native?.whenPads((pressed) => {
    padsSaid = pressed;
});
native?.whenGameEnds((problem) => {
    navigationWaits = true;
    padsSaid = null;
    say(problem ?? '', Boolean(problem));
    refresh();
});

/// A folder of the desktop app's, remembered, and its games staying where
/// they are: what's taken from it once are the machines' ROMs, into the
/// library, and the covers and game lists beside its games.
async function addFolder(folder) {
    const found = await native.folderFiles(folder);
    const romFiles = found.map(({ path, size }) => ({ path, size, name: baseName(path) })).filter(mayHoldRoms);
    say('Looking through the folder…');
    const { held, roms } = await native.sortRoms(romFiles.map(({ path }) => path));
    for (const path of held) {
        await native.hide(path); // a ROM, not a game
    }
    await native.scan();
    const media = found.map(({ path }) => native.laterFile(path)).filter(({ name }) => isMedia(name));
    const made = await takeArt(media, art, (at, total) => say(`Making covers: ${at} of ${total}…`));
    await loadArt();
    await refresh();
    const count = (number, what) => `${number} ${what}${number === 1 ? '' : 's'}`;
    const also = (number, what) => (number ? ` ${count(number, what)}.` : '');
    say(`Added ${count(native.gamesIn(folder), 'game')} from ${folder}.${also(roms.length, 'system ROM')}${also(made, 'cover')}`);
    if (!page('settings').hidden) {
        showShelf(files.some(isGame) || !files.some(isDisk) ? 'nes' : 'apple');
    }
}

/// Folders and files dropped on the window, or a folder picked.
async function addNatively(folders, dropped) {
    try {
        for (const folder of folders) {
            await addFolder(folder);
        }
        if (dropped.length) {
            const named = dropped.filter((path) => !notRoms.test(baseName(path)));
            const sorted = await native.sortRoms(named);
            await native.scan();
            const others = dropped.filter((path) => !sorted.held.includes(path));
            await add(await Promise.all(others.map(native.fileAt)), [], sorted);
        }
    } catch (problem) {
        say(`Couldn't add it: ${problem}`, true);
    }
}

/// The folders looked through again, the library changed if they have,
/// and said so, or if `askedTo`, said that they haven't.
async function lookForChanges(askedTo = false) {
    const { came, went } = await native.scan().catch((problem) => {
        say(`Couldn't look through the folders: ${problem}`, true);
        return {};
    });
    if (came === undefined) {
        return;
    }
    const counted = (number, what) => `${number} ${what}${number === 1 ? '' : 's'}`;
    if (came || went) {
        await refresh();
        const told = [came && `${counted(came, 'new game')}`, went && `${counted(went, 'game')} gone`].filter(Boolean);
        say(told.join(', ') + '.');
    } else if (askedTo) {
        say('No new games.');
    }
    fillSettings();
}

/// A folder chosen in the system's own dialog, and added.
async function chooseFolder() {
    const folder = await native.pickFolder().catch((problem) => say(String(problem), true));
    if (folder) {
        await addNatively([folder], []);
        fillSettings();
    }
}

const row = (text, detail, button, action) => {
    const item = document.createElement('li');
    const words = document.createElement('span');
    words.className = 'row-text';
    words.textContent = text;
    if (detail) {
        const note = document.createElement('small');
        note.textContent = detail;
        words.append(note);
    }
    item.append(words);
    if (button) {
        const press = document.createElement('button');
        press.textContent = button;
        press.onclick = action;
        item.append(press);
    }
    return item;
};

/// Settings, in the desktop app: the folders, what's hidden and the system
/// files, as they are now.
async function fillSettings() {
    if (!native) {
        return;
    }
    const { folders, hidden, data } = await native.settings();
    const counted = (number, what) => `${number} ${what}${number === 1 ? '' : 's'}`;
    page('folders').replaceChildren(...folders.map((folder) => row(folder, counted(native.gamesIn(folder), 'game'), 'Remove', async () => {
        if (confirm(`Remove ${folder} from the library? Its files stay where they are.`)) {
            await native.removeFolder(folder);
            await lookForChanges();
        }
    })));
    if (!folders.length) {
        page('folders').append(row('No folders yet. Add one to start your library.'));
    }
    page('hidden-games').replaceChildren(...hidden.map((path) => row(shortName(path), path, 'Restore', async () => {
        await native.restore(path);
        await lookForChanges();
    })));
    const has = (name) => files.includes(name);
    const appleFound = [...DISK_II_ROMS, ...IIE_ROMS.apple2ee].filter(has).length;
    page('system-files').replaceChildren(
        row('Disk System BIOS', has(FDS_BIOS) ? 'Found' : 'Missing: Disk System games need disksys.rom'),
        row('Apple II ROMs', `${appleFound} of ${DISK_II_ROMS.length + IIE_ROMS.apple2ee.length} found`),
    );
    page('data-place').textContent = `Covers and system files are kept in ${data}`;
}

/// The desktop app's own way of looking: a window of its own, not a page.
if (native) {
    document.documentElement.classList.add('native');
    document.querySelector('nav.tabs').prepend(document.querySelector('h1'));
    page('tools').append(page('find'));
    page('file-actions').append(document.querySelector('label.primary'));
    document.querySelector('header').hidden = true;
    page('tab-settings').hidden = false;
    addEventListener('contextmenu', (event) => event.preventDefault());
    native.whenDropped(({ folders, files: dropped }) => addNatively(folders, dropped).then(fillSettings));
    native.whenMenu((item) => {
        if (item === 'add-folder') {
            chooseFolder();
        } else if (item === 'settings') {
            showShelf('settings');
        } else if (item === 'find') {
            showShelf(page('nes').hidden && page('apple-shelf').hidden ? 'nes' : localStorage.getItem('shelf'));
            page('filter').focus();
        } else if (item === 'rescan') {
            lookForChanges(true);
        }
    });
    page('open-controls').hidden = true;
    page('add-path').onclick = chooseFolder;
    page('change-controls').onclick = () => playNatively({ kind: 'controls' });
    page('rescan').onclick = () => lookForChanges(true);
    page('start-fullscreen').checked = localStorage.getItem('fullscreen') === 'yes';
    page('start-fullscreen').onchange = ({ target }) => localStorage.setItem('fullscreen', target.checked ? 'yes' : 'no');
}

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
    page('disk').hidden = page('disk-help').hidden = !emulator.disk_system();
    const canvas = page('screen');
    page('machine').style.setProperty('--ratio', emulator.shown_width() / emulator.shown_height());
    canvas.focus();
    pointerMoved();
    last = performance.now();
}

/// What the machine's written, all taken at once, so it's kept as it was:
/// files saved, whole; a hard disk's volume, only the parts written over.
async function saveWritten() {
    const whole = written_files().map((name) => [name, read_file(name)]);
    const parts = changed_files().map((name) => [name,
        [...take_changed_parts(name)].map((offset) => [offset, read_part(name, offset)])]);
    for (const [name, data] of whole) {
        await keep(name, data);
    }
    for (const [name, changed] of parts) {
        await keepParts(name, changed);
    }
}

async function stop() {
    navigationWaits = true;
    await saveWritten();
    emulator.free();
    emulator = null;
    held.clear();
    clearTimeout(pointerTimer);
    letGo(page('handheld'));
    page('machine').hidden = true;
    page('library').hidden = false;
    document.title = 'Open Bus';
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
press('info', () => emulator.function_key(3));
press('disk', () => emulator.function_key(CHANGE_DISK_KEY));
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
function padState(pad) {
    const button = (n) => pad?.buttons[n]?.pressed ?? false;
    // The west button: 2 in the standard layout, but Firefox on Linux goes by
    // the kernel's button names, and an Xbox pad's X is sent as BTN_NORTH.
    const west = /Linux.*Firefox/.test(navigator.userAgent) && /x-?box/i.test(pad?.id) ? 3 : 2;
    const push = (axis) => {
        const travel = pad?.axes[axis] ?? 0;
        return Math.abs(travel) < 0.15 ? 0 : travel;
    };
    const [stickX, stickY] = [push(0), push(1)];
    // A pad the browser has no layout for comes as it is. An Xbox pad on
    // Linux in Chrome is that: Back and Start 6 and 7, its D-pad two axes.
    const raw = pad?.mapping === '' && pad.buttons.length === 11 && pad.axes.length === 8;
    const [back, start] = raw ? [6, 7] : [8, 9];
    const hat = (axis, way) => raw && pad.axes[axis] * way > 0.5;
    return {
        stickX,
        stickY,
        padUp: button(12) || hat(7, -1),
        padDown: button(13) || hat(7, 1),
        padLeft: button(14) || hat(6, -1),
        padRight: button(15) || hat(6, 1),
        a: button(0),
        b: button(west),
        east: button(1),
        select: button(back),
        start: button(start),
    };
}

/// Whether what's bound to `action` is held, for `who`, with gamepad `pad`.
function boundHeld(who, action, pad, state) {
    return bindings.holds(controlsBound, who, action, (source) => (source.startsWith('key:')
        ? held.has(source.slice('key:'.length))
        : bindings.padHeld(source, pad, state)));
}

/// A player's console buttons, a bit each: as they're bound, with the stick
/// steering, and what else presses them.
function padBits(who, pad, state, touched = {}) {
    const steered = { up: state.stickY < -0.5, down: state.stickY > 0.5, left: state.stickX < -0.5, right: state.stickX > 0.5 };
    return bindings.ACTIONS.filter((action) => action !== 'library')
        .map((action) => touched[action] || steered[action] || boundHeld(who, action, pad, state))
        .reduce((bits, on, bit) => bits | (on ? 1 << bit : 0), 0);
}

function controls() {
    const pads = [...(navigator.getGamepads?.() ?? [])].filter(Boolean);
    const [first, second] = [padState(pads[0]), padState(pads[1])];
    const key = (...codes) => codes.some((code) => held.has(code));
    const { stickX, stickY, padUp, padDown, padLeft, padRight } = first;
    const gamepad = padBits(0, pads[0], first, touch);
    const gamepad2 = padBits(1, pads[1], second);
    if (boundHeld(0, 'library', pads[0], first)) {
        page('back').click();
        return;
    }
    // The joystick: the numeric keypad's too, and the Alt keys its buttons.
    const towards = (minus, plus, stick) => {
        if (minus !== plus) {
            return minus ? -1 : 1;
        }
        return stick;
    };
    const x = towards(padLeft || key('Numpad4', 'Numpad7', 'Numpad1'),
        padRight || key('Numpad6', 'Numpad9', 'Numpad3'), stickX);
    const y = towards(padUp || key('Numpad8', 'Numpad7', 'Numpad9'),
        padDown || key('Numpad2', 'Numpad1', 'Numpad3'), stickY);
    const buttons = (first.a || key('Numpad0', 'AltLeft') ? 1 : 0)
        | (first.east || key('NumpadDecimal', 'AltRight') ? 2 : 0);
    const down_ = [...held].some((code) => !/^(Shift|Control|Alt|Meta)/.test(code));
    emulator.controls(gamepad, gamepad2, x, y, buttons, down_, reset);
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
        document.title = `${title} - Open Bus`;
    }
}

/// As many frames as are due, at the machine's own rate, the screen's
/// whatever it is; a tab long away doesn't run to catch up.
function tick(now) {
    requestAnimationFrame(tick);
    if (page('controls-dialog').open) {
        lookForPress();
        last = now;
        return;
    }
    if (!emulator) {
        navigate(now);
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

/// Fullscreen: of the picture, or on a touch screen all of it, the picture
/// alone having no controls.
const touchScreen = matchMedia('(pointer: coarse)');

function fullscreen() {
    if (document.fullscreenElement) {
        document.exitFullscreen();
    } else {
        page(touchScreen.matches ? 'machine' : 'frame').requestFullscreen?.();
    }
}

attach(page('handheld'));
page('full').hidden = !page('frame').requestFullscreen;

page('screen').ondblclick = fullscreen;

/// The pointer over the picture, gone once it's been still a while.
const POINTER_SHOWN = 2000;
let pointerTimer = 0;

function pointerMoved() {
    page('frame').classList.remove('still');
    clearTimeout(pointerTimer);
    pointerTimer = setTimeout(() => page('frame').classList.add('still'), POINTER_SHOWN);
}

// Hiding it can bring an event of no movement at all: that doesn't count.
page('frame').onpointermove = (event) => {
    if (event.movementX || event.movementY) {
        pointerMoved();
    }
};

if (!native) {
    await init();
}
await loadArt();
await refresh();
const firstShelf = native && !files.length ? 'settings' : (files.some(isGame) || !files.some(isDisk) ? 'nes' : 'apple');
const lastShelf = localStorage.getItem('shelf');
showShelf(native && !files.length ? firstShelf : lastShelf && lastShelf !== 'settings' ? lastShelf : firstShelf);
if (native) {
    // With nothing known yet, the folders may yet turn something up.
    const startedEmpty = !files.length;
    lookForChanges().then(() => {
        if (startedEmpty && files.length) {
            showShelf(files.some(isGame) || !files.some(isDisk) ? 'nes' : 'apple');
        }
    });
}
requestAnimationFrame(tick);
