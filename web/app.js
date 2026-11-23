// The page: its library of files, and a machine run on the canvas, a frame
// at a time, its picture and sound taken after each and what's pressed told
// to it. The emulator itself is the WebAssembly, src/web.rs its side.
import { keep, keepParts, forget, names, bytes, allArt, native } from './files.js';
import { touch, attach, letGo } from './touch.js';
import { games, isVolume, storedNames, shortName, natural } from './disks.js';
import { findCovers } from './covers.js';
import * as bindings from './controls.js';
import { artKey, isMedia, keyOf, plainTitle, tagsOf, takeArt, titleWords } from './art.js';
import { drawScene } from './scenes.js';
import { learnDetails, learnedDetails } from './details.js';
import { versionKey, versionName, versionRank } from './versions.js';
import { usageOf } from './keys.js';

/// The emulator's WebAssembly: the browser's only, the desktop app's games
/// being the native emulator's, which sorts its ROMs too.
const {
    default: init,
    Emulator,
    RomSorter,
    put_file,
    read_file,
    read_part,
    written_files,
    changed_files,
    take_changed_parts,
    game_facts,
    rewind,
    drive_light,
    app_version,
} = native ? {} : await import('./pkg/cpu_emu.js');

/// What's kept of the games, favourites, plays, their own settings and what's
/// known of them: the desktop app's library database, or this browser's.
const keeper = native ?? (await import('./kept.js'));

const page = (id) => document.getElementById(id);
/// What the library scrolls in: in the desktop app, the library itself, its
/// scrollbar over the scene behind it; in a browser, the page.
const scroller = native ? page('library') : document.scrollingElement;
/// Too narrow for the sidebar beside the shelf, as a phone is: it's slid in
/// over it, by the menu button. As style.css has it.
const PHONE = '(max-width: 39.99rem)';
const IIE_ROMS = {
    apple2ee: [
        'Apple IIe Enhanced ROM Pages C0-DF - 342-0304-A - 1984.bin',
        'Apple IIe Enhanced ROM Pages E0-FF - 342-0303-A - 1984.bin',
        'Apple IIe Enhanced Video ROM - 342-0265-A - US 1983.bin',
    ],
    apple2e: [
        'Apple IIe ROM Pages C0-DF - 342-0135-A - 1982.bin',
        'Apple IIe ROM Pages E0-FF - 342-0134-A - 1982.bin',
        'Apple IIe Video ROM - 342-0133-A - US 1982.bin',
    ],
};
const DISK_II_ROMS = ['disk2-p5-341-0027.bin', 'disk2-p6-341-0028.bin'];
/// The function key that changes a Disk System's disk; its BIOS's name.
const CHANGE_DISK_KEY = 9;
const FDS_BIOS = 'disksys.rom';
/// The 7800's High Score Cartridge's ROM, and its RAM as it's kept.
const HIGH_SCORE_ROM = 'highscre.bin';
/// The Commander X16's ROM, as it's sorted into the library; and the X16
/// itself, to BASIC, its tile's name and title, first on its shelf.
const X16_ROM = 'x16-rom.bin';
const X16_TITLE = 'Commander X16';
const isX16Itself = (name) => name === X16_ROM;
/// busterm, the X16's BBS terminal, on its shelf in the app even before it's
/// in the library: its tile's name then, and its title.
const BUSTERM_TILE = 'busterm:get';
const BUSTERM_TITLE = 'busterm';
/// The tiles that aren't a game's file: the X16 itself, and busterm to get.
const isStandIn = (name) => isX16Itself(name) || name === BUSTERM_TILE;
/// busterm in the library (its release's zip, or before Open Bus 0.4.3 its
/// program alone) or a folder: its name, or undefined.
const bustermFile = () => files.find((name) => /(^|[\\/])busterm\.(prg|zip)$/i.test(name));
/// Where the X16's games' SD cards are kept, each of its game's name; and
/// the X16's own.
const X16_CARDS = 'x16 cards/';
const x16Card = (game) => `${X16_CARDS}${game ?? X16_TITLE}.img`;
const isCard = (name) => name.startsWith(X16_CARDS) && kind(name) === 'img';
const kind = (name) => name.split('.').pop().toLowerCase();
/// A game's file: a .bin a 2600's or a 7800's, unless it's an Apple's ROM.
const isGame = (name) => ['nes', 'fds', 'a26', 'a78', 'bin', 'zip', 'prg'].includes(kind(name)) && !isRom(name);
const isDisk = (name) => ['dsk', 'do', 'po', 'nib', 'woz', 'hdv', '2mg'].includes(kind(name));
/// The consoles whose games are on the shelf of games, each its own tab; and
/// which a game's is: as the emulator's said of it, else as its file says, a
/// zip a NES's till it's said. The X16's games are zips of its programs.
const CONSOLES = { nes: 'NES', atari2600: 'Atari 2600', atari7800: 'Atari 7800', x16: 'Commander X16' };
const X16_BOARD = 'COMMANDER X16';
const isConsole = (shelf) => shelf in CONSOLES;
/// The Ataris, by their boards' names and their files'.
const ATARIS = { atari2600: ['ATARI 2600', 'a26'], atari7800: ['ATARI 7800', 'a78'] };
function systemOf(name) {
    const board = factsOf.get(name)?.mapper;
    if (board?.startsWith(X16_BOARD)) {
        return 'x16';
    }
    const atari = Object.entries(ATARIS).find(([, [named, ending]]) =>
        board ? board.startsWith(named) : kind(name) === ending,
    );
    return atari?.[0] ?? 'nes';
}
/// Whose a game is, by what the emulator says of its file, `name`, its
/// bytes as `read` has them: a Disk System's a NES's.
async function machineSaid(name, read) {
    const [, board = ''] = game_facts(name, await read()).split('\t');
    if (board.startsWith(X16_BOARD)) {
        return 'x16';
    }
    return Object.keys(ATARIS).find((atari) => board.startsWith(ATARIS[atari][0])) ?? 'nes';
}
/// Whether there's anything to play; and the shelf to show of it first, a
/// console's, the NES's if it has any, unless there are only disks.
const hasGames = () => files.some(isGame) || files.some(isDisk);
function gamesShelf() {
    const games = files.filter(isGame);
    if (!games.length && files.some(isDisk)) {
        return 'apple';
    }
    return Object.keys(CONSOLES).find((system) => games.some((name) => systemOf(name) === system)) ?? 'nes';
}
const isOurs = (name) => ['srm', 'state', 'resume'].includes(kind(name));
const isRom = (name) =>
    kind(name) === 'rom' ||
    name === HIGH_SCORE_ROM ||
    name === X16_ROM ||
    DISK_II_ROMS.includes(name) ||
    Object.values(IIE_ROMS).flat().includes(name);
const isUsable = (name) => isGame(name) || isDisk(name) || isOurs(name) || isRom(name);
/// The most a file can be and hold a ROM, a zip's, a .bin's (the X16's is
/// up to 512K) and any other's; and what isn't worth looking in.
const MOST_ZIP = 0x400000;
const MOST_BIN = 0x80000;
const MOST = 0x10000;
const notRoms = /\.(png|jpe?g|gif|mp4|xml|txt|nfo|pdf|srm|state|nes|fds)$/i;
const baseName = (path) => path.split(/[/\\]/).pop();
const mostOf = (name) => ({ zip: MOST_ZIP, bin: MOST_BIN })[kind(name)] ?? MOST;
const mayHoldRoms = (file) => !notRoms.test(file.name) && file.size <= mostOf(file.name);

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
const TV_TILE = 14;
const GAP_ACROSS = 0.9;
const GAP_DOWN = 1.8;
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
    const lines = [
        ['up', 'Up'],
        ['down', 'Down'],
        ['left', 'Left'],
        ['right', 'Right'],
        ['a', 'A'],
        ['b', 'B'],
        ['start', 'Start'],
        ['select', 'Select'],
    ];
    page('control-help').replaceChildren(
        ...lines.flatMap(([action, label]) => {
            const source = controlsBound[0][action].find((bound) => bound.startsWith('key:'));
            if (!source) {
                return [];
            }
            const key = document.createElement('kbd');
            key.textContent = bindings.said(source);
            const pair = document.createElement('span');
            pair.append(key, ` ${label}`);
            return [pair, ' '];
        }),
    );
}

/// What the dialog calls those whose names aren't words.
const LABELS = {
    turboA: 'Turbo A',
    turboB: 'Turbo B',
    fast: 'Fast forward',
    menuAccept: 'Menu accept',
    menuBack: 'Menu back',
};

/// The dialog's rows, as they're bound now.
function fillControls() {
    page('control-rows').replaceChildren(
        ...bindings.ACTIONS.map((action) => {
            const item = document.createElement('li');
            const words = document.createElement('span');
            words.className = 'row-text';
            words.textContent = LABELS[action] ?? action[0].toUpperCase() + action.slice(1);
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
        }),
    );
    for (const who of [0, 1]) {
        page(`controls-player-${who + 1}`).classList.toggle('on', who === controlsPlayer);
    }
}

function bindAwaited(source) {
    bindings.bind(controlsBound, controlsPlayer, awaiting, source);
    bindings.save(controlsBound);
    page('control-status').textContent =
        `${awaiting[0].toUpperCase()}${awaiting.slice(1)} is now ${bindings.said(source)}.`;
    awaiting = null;
    showControlHelp();
    fillControls();
}

/// A gamepad's anything newly pressed, while one's being waited for.
function lookForPress() {
    if (awaiting) {
        const pressed = padsNow()
            .flatMap(bindings.rawPressed)
            .find((source) => !heldBefore.has(source));
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
addEventListener(
    'keydown',
    (event) => {
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
    },
    true,
);
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
        const words = `${coverSystem(machine)}\n${titleWords(key)}`;
        if (cover && machine !== 'apple' && (!coversByTitle.has(words) || key.includes('(usa'))) {
            coversByTitle.set(words, key);
        }
    }
}

/// Which console's games a cover found by title is of, by its `machine`: an
/// Atari's or the X16's apart, the NES's and the Disk System's together.
const coverSystem = (machine) => (machine in ATARIS || machine === 'x16' ? machine : 'nes');

/// Which shelf's showing; and of the consoles', which the shelf of games
/// has, the last shown.
let shelfShowing = 'nes';
let consoleShowing = 'nes';
function showShelf(name) {
    shelfShowing = name;
    localStorage.setItem('shelf', name);
    if (isConsole(name) && name !== consoleShowing) {
        consoleShowing = name;
        const wanted = page('filter').value.trim().toLowerCase();
        if (shelvesFor === wanted) {
            showGames();
        } else {
            fillGames(wanted);
        }
    }
    page('nes').hidden = !isConsole(name);
    page('recent').hidden = name !== 'recent';
    page('favourites').hidden = name !== 'favourites';
    page('apple-shelf').hidden = name !== 'apple';
    page('settings').hidden = name !== 'settings';
    page('tools').hidden = name === 'settings';
    if (name === 'settings') {
        fillSettings();
        scroller.scrollTo(0, 0);
    }
    placeShelf();
    for (const tab of document.querySelectorAll('.tabs button[data-tab]')) {
        tab.classList.toggle('on', tab.dataset.tab === name);
    }
    const tab = document.querySelector(`.tabs button[data-tab="${name}"]`);
    page('shelf-name').textContent = (tab.querySelector('.tab-name') ?? tab).textContent;
    showMenu(false);
    showSorts();
    // What a gamepad had, gone with the page it was on.
    if (padFocus && !focusable().includes(padFocus)) {
        padFocus = null;
    }
    showSelected();
}

for (const tab of document.querySelectorAll('.tabs button[data-tab]')) {
    tab.onclick = () => showShelf(tab.dataset.tab);
}

/// The shelves, on a phone: the sidebar slid in over the shelf by the menu
/// button, and out again by a shelf chosen, a tap beside it, or Esc.
function showMenu(open) {
    const sidebar = page('sidebar');
    const was = sidebar.classList.contains('open');
    sidebar.classList.toggle('open', open);
    page('menu').setAttribute('aria-expanded', open);
    if (open) {
        sidebar.querySelector('button.on')?.focus();
    } else if (was && sidebar.contains(document.activeElement)) {
        page('menu').focus();
    }
}
page('menu').onclick = () => showMenu(!page('sidebar').classList.contains('open'));
page('shade').onclick = () => showMenu(false);
addEventListener(
    'keydown',
    (event) => {
        if (event.code === 'Escape' && page('sidebar').classList.contains('open')) {
            event.stopImmediatePropagation();
            showMenu(false);
        }
    },
    true,
);

/// The files, in order, those a filter hides left out; those the machines
/// write hidden.
async function refresh() {
    files = (await names()).sort(natural);
    await loadKept();
    chosen = chosen.filter((name) => files.includes(name));
    const wanted = page('filter').value.trim().toLowerCase();
    fillGames(wanted);
    fillDisks(wanted);
    fillKept(wanted);
    showRoms();
    showSorts();
    // Not the X16's: libretro has no set of its games.
    learnDetails(
        files.filter(isGame).flatMap((name) => {
            const facts = factsOf.get(name);
            const set = facts?.mapper.startsWith('MAPPER 20 ') ? 'fds' : systemOf(name);
            return facts?.crc && set !== 'x16' ? [{ name, crc: facts.crc, set }] : [];
        }),
        refresh,
    );
    const missing = coverless().length;
    page('covers-wanted').hidden = !missing;
    page('coverless').textContent =
        `${missing === 1 ? '1 game has' : `${missing} games have`} no cover. Open Bus can look online for them: ` +
        "boxes from libretro's set on GitHub, and Apple II title screens from the Internet Archive. Your games stay here.";
    learnFacts();
}

/// The games with no cover, as `findCovers` wants them.
function coverless() {
    const nes = files.filter(isGame).map((file) => ({
        machine: systemOf(file) in ATARIS ? systemOf(file) : kind(file) === 'fds' ? 'fds' : 'nes',
        file,
        title: plainTitle(file),
        key: nesArt(file),
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
    say(
        [`Found ${found} cover${found === 1 ? '' : 's'}${missed ? `, none for ${missed}` : ''}.`, ...stopped].join(' '),
        problems.length > 0,
    );
}

page('find').onclick = findMissing;

/// Whose art a game has: that of its file, or of any of them, or of its
/// title, for a picture named after it.
const artFor = (title, ...names) =>
    [...names.map(artKey), keyOf(title)].find((key) => art.has(key)) ?? artKey(names[0]);

/// What's said of an empty shelf.
const noGames = native
    ? 'No games yet. Add a folder of them in Settings.'
    : 'No games yet. Add .nes, .fds, .a26, .a78, .bin or .zip files in Settings, or drop them here.';

/// Whether what's searched for is in a game's title, file, or what's known
/// of it.
const found = (wanted, title, file, known) =>
    [title, file, known?.developer, known?.genre, known?.year].join(' ').toLowerCase().includes(wanted);

/// The ways a shelf can be sorted: by title; by what game lists say of its
/// games, KNOWN; and Recent's and Favourites', by how they've been played.
const SORTS = {
    title: 'Title',
    newest: 'Newest',
    oldest: 'Oldest',
    genre: 'Genre',
    developer: 'Developer',
    players: 'Players',
    last: 'Last played',
    most: 'Most played',
};
const KNOWN = { newest: 'year', oldest: 'year', genre: 'genre', developer: 'developer', players: 'players' };
const PLAYED = ['last', 'most'];
const keptShelf = (shelf) => shelf === 'recent' || shelf === 'favourites';
const sortsOf = (shelf) => Object.keys(SORTS).filter((way) => keptShelf(shelf) || !PLAYED.includes(way));

/// How `shelf`'s sorted: as it was last chosen for it; else Recent's by when
/// last played, the rest by title.
function sortOf(shelf) {
    const way = localStorage.getItem(`sort-${shelf}`);
    return sortsOf(shelf).includes(way) ? way : shelf === 'recent' ? 'last' : 'title';
}

/// Which of KNOWN's fields some of each shelf's games have, to be sorted
/// by: as they were when it was last filled.
const sortable = new Map();

/// `shelf`'s `items` as its sort says, by title among those alike, those not
/// known about last. Kept games, Recent's and Favourites', by when they were
/// last played or for how long in all.
function arranged(shelf, items, titleOf, knownOf) {
    const way = sortOf(shelf);
    const field = KNOWN[way];
    const has = (name) => items.some((item) => knownOf(item)?.[name]);
    sortable.set(shelf, new Set(Object.values(KNOWN).filter(has)));
    const byTitle = (a, b) => natural(titleOf(a), titleOf(b));
    if (PLAYED.includes(way)) {
        const played = way === 'last' ? ({ last }) => last ?? -1 : ({ seconds }) => seconds;
        return [...items].sort((a, b) => played(b) - played(a) || byTitle(a, b));
    }
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

/// The sorts of the shelf showing, as it's sorted: those it has nothing to
/// sort by greyed out, as a browser's games have till game lists come with
/// them. Made again only when they've changed, not under a finger.
function showSorts() {
    const select = page('sort');
    const known = sortable.get(shelfShowing) ?? new Set();
    const ways = sortsOf(shelfShowing).map((way) => [way, way in KNOWN && !known.has(KNOWN[way])]);
    const said = JSON.stringify(ways);
    if (select.dataset.ways !== said) {
        select.dataset.ways = said;
        select.replaceChildren(
            ...ways.map(([way, nothing]) => {
                const option = new Option(SORTS[way], way);
                option.disabled = nothing;
                return option;
            }),
        );
    }
    select.value = sortOf(shelfShowing);
}

page('sort').onchange = () => {
    localStorage.setItem(`sort-${shelfShowing}`, page('sort').value);
    refresh();
};

/// A tab's name, and how many it holds beside it.
function showCount(tab, name, count) {
    const label = document.createElement('span');
    label.className = 'tab-name';
    label.textContent = name;
    const tally = document.createElement('span');
    tally.className = 'tally';
    tally.textContent = count;
    page(tab).replaceChildren(label, ...(count ? [tally] : []));
}

/// Whose art a console's game has: its own, or one of its title's, of its
/// console's covers.
function nesArt(name) {
    const key = artFor(plainTitle(name), name);
    const titled = `${coverSystem(systemOf(name))}\n${titleWords(plainTitle(name))}`;
    return covers.has(key) ? key : (coversByTitle.get(titled) ?? key);
}

/// A game's board, in words: a NES's mapper, an Atari's cartridge.
const boardOf = (facts) =>
    facts?.mapper
        .replace(/^MAPPER (\d+) /, 'Mapper $1, ')
        .replace(/^ATARI (2600|7800) /, 'Cartridge ')
        .replace(/^COMMANDER X16 /, 'Program ');

/// A game's title, by what's known of it, else its file.
const titleOf = (name) =>
    isX16Itself(name)
        ? X16_TITLE
        : name === BUSTERM_TILE
          ? BUSTERM_TITLE
          : (art.get(nesArt(name))?.title ?? plainTitle(name));

/// What's known of a NES game: what a game list beside it said, else what's
/// been learned of it from libretro's database.
let learned = new Map();
function nesKnown(name) {
    const listed = art.get(nesArt(name));
    const found = learned.get(name);
    if (!found) {
        return listed;
    }
    const details = ['year', 'genre', 'developer', 'players'].map((field) => [field, listed?.[field] ?? found[field]]);
    return { ...listed, ...Object.fromEntries(details) };
}

/// Each NES game's versions, by any of their files: the best first.
let versionsOf = new Map();

/// The version of each game that's been chosen for it, by `chosenKey`, kept
/// in this browser, or the desktop app's page: its versionKey, a 2600's
/// with its console's name before it, apart from the NES's of its title.
const CHOSEN_VERSIONS = 'versions';
const chosenKey = (name) => (systemOf(name) === 'nes' ? versionKey(name) : `${systemOf(name)}|${versionKey(name)}`);
function chosenVersions() {
    try {
        return JSON.parse(localStorage.getItem(CHOSEN_VERSIONS)) ?? {};
    } catch {
        return {};
    }
}

/// The games among a console's files `all`, one file each: the version
/// chosen, or played last, or the best; each's versions added to
/// `versionsOf`, those that won't load last.
function oneEach(all) {
    const games = new Map();
    for (const name of all) {
        const key = chosenKey(name);
        games.set(key, [...(games.get(key) ?? []), name]);
    }
    const chosen = chosenVersions();
    const playedAt = new Map(
        kept
            .filter(({ machine, plays }) => isConsole(machine.kind) && plays > 0)
            .map(({ machine, last }) => [machine.game, last]),
    );
    const rank = (name) => versionRank(name) + (factsOf.get(name)?.problem ? 1000 : 0);
    return [...games].map(([key, versions]) => {
        versions.sort((a, b) => rank(a) - rank(b) || natural(a, b));
        for (const name of versions) {
            versionsOf.set(name, versions);
        }
        const played = versions.filter((name) => playedAt.has(name));
        const last = played.sort((a, b) => playedAt.get(b) - playedAt.get(a))[0];
        return versions.includes(chosen[key]) ? chosen[key] : (last ?? versions[0]);
    });
}

/// Each console's shelf, as it was last made, and the search it was made
/// for: a tab shown again as it is, nothing else having changed, as all
/// else that would comes by `refresh`.
const shelves = new Map();
let shelvesFor = null;

/// The shelf of games, the console's showing; and each console's tab, how
/// many it has.
function fillGames(wanted) {
    // Each game's title and what's known of it worked out once, not again
    // at each of the sort's comparisons: thousands of games, and a TV's
    // slow to.
    const once = (work) => {
        const done = new Map();
        return (name) => {
            if (!done.has(name)) {
                done.set(name, work(name));
            }
            return done.get(name);
        };
    };
    const [knownOf, titled] = [once(nesKnown), once(titleOf)];
    const matches = (name) =>
        versionsOf.get(name).some((version) => found(wanted, titled(version), version, knownOf(version)));
    versionsOf = new Map();
    const games = files.filter(isGame);
    for (const [system, name] of Object.entries(CONSOLES)) {
        const all = oneEach(games.filter((game) => systemOf(game) === system));
        const shown = arranged(system, all.filter(matches), titled, knownOf);
        // The X16 itself, first, and in the app busterm: there even before
        // their files are, which they offer to download.
        if (system === 'x16') {
            if (native && !bustermFile() && found(wanted, BUSTERM_TITLE, 'bbs terminal')) {
                shown.unshift(BUSTERM_TILE);
            }
            if (found(wanted, X16_TITLE, 'basic')) {
                shown.unshift(X16_ROM);
            }
        }
        showCount(`tab-${system}`, name, shown.length);
        shelves.set(system, shown);
    }
    shelvesFor = wanted;
    showGames();
}

/// The console's shelf showing, as it was made.
function showGames() {
    shelf = shelves.get(consoleShowing) ?? [];
    const list = page('games');
    list.nextElementSibling.textContent = shelvesFor ? 'Nothing matches.' : noGames;
    tiles.clear();
    showLetters();
    placeShelf();
}

/// The first letter of a game's title, as the strip has it: a digit or
/// anything else, #.
function letterOf(name) {
    const first = titleOf(name).trim().charAt(0).toUpperCase();
    return first >= 'A' && first <= 'Z' ? first : '#';
}

/// The strip of letters down the shelf of games' side, while it's sorted
/// by title: each the first game of its letter's, a letter with none dimmed.
/// Pointed at, or dragged along on a touch screen, it jumps there.
function showLetters() {
    const strip = page('letters');
    strip.hidden = sortOf(consoleShowing) !== 'title' || !shelf.length;
    if (strip.hidden) {
        return;
    }
    const firsts = new Map();
    shelf.forEach((name, at) => firsts.has(letterOf(name)) || firsts.set(letterOf(name), at));
    strip.replaceChildren(
        ...['#', ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'].map((letter) => {
            const button = document.createElement('button');
            button.textContent = letter;
            button.dataset.letter = letter;
            button.disabled = !firsts.has(letter);
            button.setAttribute('aria-label', letter === '#' ? 'Numbers and symbols' : letter);
            button.onclick = () => jumpToLetter(letter);
            return button;
        }),
    );
    letterShown = null;
}
/// Dragged along, by a finger or the mouse: the letter level with it, by
/// height alone, the finger free to wander off to the side; one with no
/// games, the nearest that has. A fingertip covers more than a letter, so
/// the one it's on stays till it's moved a third of a letter past it, not
/// flicking between two as it rests on their edge; and only the first
/// finger's followed. As it goes, the letter big in the middle of the
/// screen, the finger over the strip's own, and a tap of a phone's buzzer
/// each time it changes.
const STAY = 1 / 3;
let scrubbing = null;
let scrubber = null;
/// Where the strip was on the screen when it was touched: the finger's
/// measured against that, on the screen, not the page, which a phone's
/// browser can move under it as its address bar comes and goes.
let stripOnScreen = null;
function scrub(event) {
    const letters = [...page('letters').children];
    const { top, height } = stripOnScreen;
    const along = ((event.screenY - top) / height) * letters.length;
    const now = letters.findIndex((button) => button.dataset.letter === scrubbing);
    if (now >= 0 && along >= now - STAY && along < now + 1 + STAY) {
        return;
    }
    const at = Math.min(letters.length - 1, Math.max(0, Math.floor(along)));
    const nearest = letters
        .map((button, place) => ({ button, away: Math.abs(place - at) }))
        .filter(({ button }) => !button.disabled)
        .sort((a, b) => a.away - b.away)[0]?.button;
    if (!nearest || nearest.dataset.letter === scrubbing) {
        return;
    }
    scrubbing = nearest.dataset.letter;
    jumpToLetter(scrubbing);
    const bubble = page('letter-bubble');
    bubble.textContent = scrubbing;
    bubble.classList.add('shown');
    navigator.vibrate?.(5);
}
page('letters').onpointerdown = (event) => {
    event.preventDefault();
    if (scrubber !== null) {
        return;
    }
    scrubber = event.pointerId;
    page('letters').setPointerCapture(event.pointerId);
    const { top, height } = page('letters').getBoundingClientRect();
    stripOnScreen = { top: top + event.screenY - event.clientY, height };
    scrubbing = null;
    scrub(event);
};
// Nor the page scrolled by a finger on the strip, as some browsers still
// would, the two then fighting.
for (const kind of ['touchstart', 'touchmove']) {
    page('letters').addEventListener(kind, (event) => event.preventDefault(), { passive: false });
}
page('letters').onpointermove = (event) => {
    if (event.pointerId === scrubber) {
        scrub(event);
    }
};
page('letters').onpointerup = page('letters').onpointercancel = (event) => {
    if (event.pointerId !== scrubber) {
        return;
    }
    scrubber = scrubbing = null;
    page('letter-bubble').classList.remove('shown');
};

/// The shelf of games at the first game of `letter`: its row at the top,
/// and it the one selected.
function jumpToLetter(letter) {
    const at = shelf.findIndex((name) => letterOf(name) === letter);
    if (at < 0) {
        return;
    }
    selectedGame = shelf[at];
    const list = page('games');
    const { columns, row, down } = shelfShape(list);
    const toolbar = document.querySelector('.toolbar').getBoundingClientRect().height;
    const top = list.getBoundingClientRect().top + scroller.scrollTop + Math.floor(at / columns) * (row + down);
    scroller.scrollTo(0, top - toolbar - down);
    placeShelf();
    showSelected();
}

/// The letter of the row at the top lit on the strip, as the shelf scrolls.
let letterShown = null;
function showLetterAt(firstShown) {
    const letter = shelf[firstShown] && letterOf(shelf[firstShown]);
    if (letter === letterShown || page('letters').hidden) {
        return;
    }
    letterShown = letter;
    for (const button of page('letters').children) {
        button.classList.toggle('on', button.dataset.letter === letter);
    }
}

/// A game's tile: on the shelf of games, for all of its versions, `name`
/// the one it plays; or, `alone`, for that one alone, as it was played.
function gameTile(name, alone = false) {
    const versions = alone ? [name] : (versionsOf.get(name) ?? [name]);
    const item = document.createElement('li');
    item.className = 'tile';
    item.dataset.game = name;
    item.oncontextmenu = (event) => {
        event.preventDefault();
        openGameSettings(name);
    };
    const key = nesArt(name);
    const known = nesKnown(name);
    const button = document.createElement('button');
    button.className = 'play';
    const facts = factsOf.get(name);
    const machine = { kind: systemOf(name), game: name };
    button.dataset.tip = [
        titleOf(name),
        known?.developer,
        name,
        boardOf(facts),
        facts?.problem && `Won't load: ${facts.problem}`,
        versions.length > 1 && `${versions.length} versions: ${versions.map(versionName).join(' · ')}`,
    ]
        .filter(Boolean)
        .join('\n');
    button.onclick = () => playGame(machine);
    // One the emulator says won't load, greyed out, and why it won't said.
    if (facts?.problem) {
        item.classList.add('unplayable');
        button.onclick = () => say(`${titleOf(name)} won't load: ${facts.problem}.`, true);
    }
    // How many versions there are, on its cover's corner.
    let art = cover(key, titleOf(name));
    if (versions.length > 1) {
        const badge = document.createElement('span');
        badge.className = 'versions';
        badge.textContent = `${versions.length} versions`;
        const box = document.createElement('span');
        box.className = 'art-box';
        box.append(art, badge);
        art = box;
    }
    button.append(art, caption(titleOf(name), known, tagsOf(name)));
    item.append(button, removeButton(name), starButton(machine, titleOf(name)));
    return item;
}

/// busterm, before it's in the library: its latest release offered for
/// download, from GitHub, and started once it's here.
function bustermTile() {
    const item = document.createElement('li');
    item.className = 'tile';
    item.dataset.game = BUSTERM_TILE;
    const button = document.createElement('button');
    button.className = 'play';
    button.dataset.tip = `${BUSTERM_TITLE}\nA BBS terminal for the X16's modem, downloaded the first time`;
    button.onclick = () => getBusterm();
    button.append(cover('', BUSTERM_TITLE), caption(BUSTERM_TITLE, null, 'BBS terminal'));
    item.append(button);
    return item;
}

/// busterm downloaded, asked first, the X16's ROM too if it isn't here; and
/// started.
async function getBusterm() {
    if (!files.includes(X16_ROM) && !(await getX16Rom())) {
        return;
    }
    const question =
        "busterm, a BBS terminal for the X16's modem, isn't in the library yet. Download its latest release from GitHub? It's about 40 KB.";
    if (!(await native.ask(question))) {
        return;
    }
    say('Downloading busterm…');
    try {
        await native.fetchBusterm();
    } catch (problem) {
        say(`Couldn't get busterm: ${problem}`, true);
        return;
    }
    say('');
    await refresh();
    const name = bustermFile();
    if (name) {
        playGame({ kind: 'x16', game: name });
    }
}

/// A newer busterm than the library's, `{ have, latest }`, as the app found
/// as it started; offered when it's next played, once.
let newerBusterm = null;

/// busterm, about to be played: the library's copy brought up to date first,
/// if there's a newer one and that's wanted. The machine to play: the new
/// one's, which may be somewhere new (in its own folder, both its files).
async function updateBusterm(machine) {
    const update = newerBusterm;
    if (!update || machine.kind !== 'x16' || machine.game !== bustermFile() || !native.isOwn(machine.game)) {
        return machine;
    }
    newerBusterm = null;
    if (!(await native.ask(`busterm ${update.latest} is out (you have ${update.have}). Update it first?`))) {
        return machine;
    }
    say('Downloading busterm…');
    try {
        await native.fetchBusterm(true);
        say(`busterm updated to ${update.latest}`);
    } catch (problem) {
        say(`busterm not updated: ${problem}`, true);
        return machine;
    }
    await refresh();
    return { ...machine, game: bustermFile() ?? machine.game };
}

/// The X16 itself: started to BASIC, with a card of its own, blank to begin
/// with and kept, and what's saved on it.
function x16Tile() {
    const item = document.createElement('li');
    item.className = 'tile';
    item.dataset.game = X16_ROM;
    const button = document.createElement('button');
    button.className = 'play';
    button.dataset.tip = `${X16_TITLE}\nBASIC, with an SD card of its own`;
    button.onclick = () => playGame({ kind: 'x16' });
    button.append(cover('', X16_TITLE), caption(X16_TITLE, null, 'BASIC'));
    item.append(button);
    return item;
}

/// The X16's ROM, asked for, it not being here: in the app, its release
/// downloaded; in a browser, which GitHub won't let download it, its zip
/// downloaded by hand and chosen. Whether it's here now.
function getX16Rom() {
    const dialog = page('x16-rom-dialog');
    const status = page('x16-rom-status');
    const buttons = [page('x16-rom-fetch'), page('x16-rom-choose'), page('x16-rom-zip')];
    status.textContent = '';
    page('x16-rom-fetch').hidden = !native;
    page('x16-rom-by-hand').hidden = page('x16-rom-link').hidden = page('x16-rom-choose').hidden = !!native;
    const working = (doing) => {
        status.textContent = doing;
        for (const button of buttons) {
            button.disabled = !!doing;
        }
    };
    working('');
    return new Promise((resolve) => {
        const done = (got) => {
            dialog.onclose = null;
            dialog.close();
            resolve(got);
        };
        dialog.onclose = () => resolve(false);
        page('x16-rom-cancel').onclick = () => done(false);
        const got = async () => {
            await refresh();
            if (files.includes(X16_ROM)) {
                say('');
                done(true);
            } else {
                working('');
                status.textContent = "That wasn't the X16's ROM: choose the release's zip, or the rom.bin in it.";
            }
        };
        page('x16-rom-fetch').onclick = async () => {
            working('Downloading…');
            try {
                await native.fetchX16Rom();
            } catch (problem) {
                working('');
                status.textContent = `Couldn't get it: ${problem}`;
                return;
            }
            await got();
        };
        page('x16-rom-zip').onchange = async (event) => {
            const chosen = [...event.target.files];
            event.target.value = '';
            if (chosen.length) {
                working('Adding it…');
                await add(chosen);
                await got();
            }
        };
        dialog.showModal();
        page(native ? 'x16-rom-fetch' : 'x16-rom-link').focus();
    });
}

/// A game started: in the desktop app, on the native emulator; here, a NES
/// or 2600 game with what's chosen of it, an Apple's disks as they were.
async function playGame(machine) {
    if (!native) {
        drive_light(page('drive-light').checked);
    }
    if (machine.kind === 'x16' && !files.includes(X16_ROM)) {
        if (await getX16Rom()) {
            playGame(machine);
        }
        return;
    }
    if (native) {
        playNatively(await updateBusterm(machine));
    } else if (machine.kind === 'x16') {
        const card = x16Card(machine.game);
        const x16 = () => Emulator.x16(X16_ROM, machine.game ?? '', card, localSeconds());
        start(x16, 'x16', [machine.game, card].filter(Boolean), machine);
    } else if (machine.kind === 'atari7800') {
        const chosen = await keeper.gameSettings(machine);
        rewind(page('rewind').checked);
        start(() => Emulator.atari7800(machine.game, chosen.region ?? ''), 'atari7800', [machine.game], machine);
    } else if (machine.kind === 'atari2600') {
        const chosen = await keeper.gameSettings(machine);
        rewind(page('rewind').checked);
        start(() => Emulator.atari2600(machine.game, chosen.region ?? ''), 'atari2600', [machine.game], machine);
    } else if (machine.kind === 'nes') {
        const chosen = await keeper.gameSettings(machine);
        const of = (name) => chosen[name] ?? '';
        const ram = of('ram') || page('power-on-ram').value;
        rewind(page('rewind').checked);
        const nes = () => Emulator.nes(machine.game, 8, of('region'), of('console'), of('port2'), ram);
        start(nes, 'nes', [machine.game], machine);
    } else {
        const { model, roms, disks, mockingboards } = machine;
        start(() => Emulator.apple(model, roms, disks, mockingboards), 'apple', disks, machine);
    }
}

/// The games kept of, favourites and those played, the last played first;
/// and which are favourites, by `gameKey`. Loaded as the library's
/// refreshed, its files known by then.
let kept = [];
let favoured = new Set();
const gameKey = (machine) =>
    machine.kind === 'apple' ? `apple\n${machine.disks.join('\n')}` : `${machine.kind}\n${machine.game}`;

async function loadKept() {
    [kept, factsOf] = await Promise.all([keeper.keptGames(), keeper.gameFacts()]);
    learned = learnedDetails();
    favoured = new Set(kept.filter(({ favourite }) => favourite).map(({ machine }) => gameKey(machine)));
}

/// What the emulator's said of each NES game, by name: its mapper, and why
/// it won't load, if it won't. Loaded with `kept`. The desktop app learns
/// it, and says when it has; here, the WebAssembly's asked of each game, in
/// the background, those it hasn't been yet.
let factsOf = new Map();
native?.whenFactsChange(() => refresh());
const learnFacts = () => native || keeper.learnFacts(files.filter(isGame), bytes, game_facts, refresh);

/// A game's star, in the desktop app: lit if it's a favourite, and pressed,
/// made one, or not.
function starButton(machine, title) {
    const on = favoured.has(gameKey(machine));
    const star = document.createElement('button');
    star.className = on ? 'star on' : 'star';
    star.textContent = on ? '★' : '☆';
    star.title = `${on ? 'Not a favourite' : 'A favourite'}: ${title}`;
    star.setAttribute('aria-label', star.title);
    star.setAttribute('aria-pressed', on);
    star.onclick = async () => {
        await keeper.favourite(machine, !on);
        refresh();
    };
    return star;
}

/// A console game's details: its cover, and what's known of it, from game
/// lists or libretro's database, from playing it, and from the emulator; its
/// versions, the one it plays chosen; and its own settings, over Settings',
/// those its console has, each kept as it's changed, what automatic would be
/// said if it's known. Played from here, or made a favourite.
async function openGameSettings(name) {
    const system = systemOf(name);
    const machine = { kind: system, game: name };
    const chosen = await keeper.gameSettings(machine);
    const dialog = page('game-settings');
    const facts = factsOf.get(name);
    const title = titleOf(name);
    page('game-settings-title').textContent = title;
    page('game-settings-about').textContent = shortName(name);
    page('game-cover').replaceChildren(cover(nesArt(name), title));
    if (native) {
        fillGameSaves(machine);
    }
    const known = nesKnown(name);
    const keptOf = kept.find((game) => game.machine.kind === system && game.machine.game === name);
    const played =
        keptOf?.plays > 0 &&
        `${keptOf.plays === 1 ? 'Once' : `${keptOf.plays} times`}, ${howLong(keptOf.seconds)} in all; ` +
            `last ${whenPlayed(keptOf.last).toLowerCase()}`;
    const versions = versionsOf.get(name) ?? [name];
    const lines = [
        ['Year', known?.year],
        ['Genre', known?.genre],
        ['Developer', known?.developer],
        ['Players', known?.players],
        ['Played', played || 'Not yet'],
        ['Version', versions.length < 2 && versionName(name)],
        ['Console', facts && (system === 'nes' ? (facts.famicom ? 'Famicom' : 'NES') : CONSOLES[system])],
        ['Board', boardOf(facts)],
        ["Won't load", facts?.problem],
    ];
    page('game-facts').replaceChildren(
        ...lines
            .filter(([, value]) => value)
            .flatMap(([term, value]) => {
                const [dt, dd] = [document.createElement('dt'), document.createElement('dd')];
                dt.textContent = term;
                dd.textContent = value;
                dd.classList.toggle('problem', term === "Won't load");
                return [dt, dd];
            }),
    );
    const favourite = page('game-favourite');
    const on = favoured.has(gameKey(machine));
    favourite.textContent = on ? '★ Favourite' : '☆ Favourite';
    favourite.setAttribute('aria-pressed', on);
    favourite.onclick = async () => {
        await keeper.favourite(machine, !on);
        await refresh();
        const at = padFocus === favourite;
        dialog.close();
        await openGameSettings(name);
        if (at) {
            focusOn(favourite);
        }
    };
    // Its versions, the one chosen played from now on, its own settings
    // shown.
    const version = page('game-version');
    version.closest('label').hidden = versions.length < 2;
    version.replaceChildren(
        ...versions.map(
            (one) => new Option(versionName(one) + (factsOf.get(one)?.problem ? " (won't load)" : ''), one),
        ),
    );
    version.value = name;
    version.onchange = async () => {
        const chosen = chosenVersions();
        chosen[chosenKey(name)] = version.value;
        try {
            localStorage.setItem(CHOSEN_VERSIONS, JSON.stringify(chosen));
        } catch {
            // Not kept, then: chosen till the page is closed.
        }
        if (selectedGame === name) {
            selectedGame = version.value;
        }
        dialog.close();
        await refresh();
        await openGameSettings(version.value);
        if (document.body.classList.contains('pointer-gone')) {
            focusOn(version);
        }
    };
    const automatic = { console: facts && (facts.famicom ? 'Famicom' : 'NES'), port2: facts?.zapper && 'Zapper' };
    // Only the settings its console has.
    for (const label of dialog.querySelectorAll('label[data-for]')) {
        label.hidden = label.dataset.for !== system;
    }
    const selects = [...dialog.querySelectorAll('select[data-setting]')];
    for (const select of selects) {
        const setting = select.dataset.setting;
        const known = automatic[setting];
        if (setting in automatic) {
            select.options[0].textContent = known ? `Automatic: ${known}` : 'Automatic';
        }
        select.value = chosen[setting] ?? '';
        select.onchange = () =>
            keeper.chooseSettings(
                machine,
                Object.fromEntries(selects.filter(({ value }) => value).map((one) => [one.dataset.setting, one.value])),
            );
    }
    page('game-settings-play').onclick = () => {
        dialog.close();
        playGame(machine);
    };
    page('game-settings-close').onclick = () => dialog.close();
    dialog.showModal();
    // A gamepad's on Play; a finger or the mouse, on nothing yet, not its
    // first choice ringed for no reason.
    if (document.body.classList.contains('pointer-gone')) {
        focusOn(page('game-settings-play'));
    } else {
        dialog.focus();
    }
}
page('game-settings').addEventListener('close', () => {
    padFocus = null;
    showSelected();
});
const settingsOpen = () => page('game-settings').open;

/// Start pressed on a NES game: its details.
function settingsOfSelected() {
    const tile = padFocus ? padFocus.closest('.tile') : gamesShowing() && tiles.get(selectedGame);
    if (tile?.dataset.game && !isStandIn(tile.dataset.game)) {
        openGameSettings(tile.dataset.game);
    }
}

/// The shelves of games kept of: those played, when and for how long under
/// each; and the favourites; each sorted as it's chosen for it. Those a
/// search leaves out, not shown.
function fillKept(wanted) {
    const titled = ({ machine }) => (machine.kind === 'apple' ? appleTitle(machine) : titleOf(machine.game));
    const knownOf = ({ machine }) =>
        machine.kind === 'apple' ? art.get(artFor(appleTitle(machine), ...machine.disks)) : nesKnown(machine.game);
    const filesOf = ({ machine }) => (machine.kind === 'apple' ? machine.disks.join(' ') : machine.game);
    const shown = kept.filter((game) => found(wanted, titled(game), filesOf(game), knownOf(game)));
    const recent = arranged(
        'recent',
        shown.filter(({ plays }) => plays > 0),
        titled,
        knownOf,
    );
    const favourites = arranged(
        'favourites',
        shown.filter(({ favourite }) => favourite),
        titled,
        knownOf,
    );
    for (const [list, onShelf, tab, name, none] of [
        ['recent-games', recent, 'tab-recent', 'Recent', 'The games you play come here, the last played first.'],
        ['favourite-games', favourites, 'tab-favourites', 'Favourites', 'Star a game, on its shelf, to keep it here.'],
    ]) {
        page(list).replaceChildren(...onShelf.map((game) => keptTile(game, list === 'recent-games')));
        page(list).nextElementSibling.textContent = wanted ? 'Nothing matches.' : none;
        showCount(tab, name, onShelf.length);
    }
}

/// A kept game's tile: a NES game's as on its shelf; an Apple game's started
/// at once. Played, when last and for how long in all in place of its facts.
function keptTile({ machine, seconds, last }, played) {
    const item = machine.kind === 'apple' ? appleTile(machine) : gameTile(machine.game, true);
    if (played) {
        const words = item.querySelector('.caption');
        const line = words.querySelector('.facts') ?? words.appendChild(document.createElement('span'));
        line.className = 'facts';
        line.textContent = `${whenPlayed(last)} · ${howLong(seconds)}`;
    }
    return item;
}

/// What an Apple game kept of is called: its disks' game's, or each's.
function appleTitle(machine) {
    return games(machine.disks).map(diskTitle).join(' + ');
}

/// An Apple game of disks, kept of: started at once, on the model and the
/// cards it was last started with, or as the Apple shelf has them now.
function appleTile(machine) {
    const title = appleTitle(machine);
    const item = document.createElement('li');
    item.className = 'tile';
    const button = document.createElement('button');
    button.className = 'play';
    button.dataset.tip = [title, ...machine.disks].join('\n');
    button.onclick = () => {
        const model = machine.model ?? page('model').value;
        const roms = model === 'apple2' ? (machine.roms ?? plusRom() ?? '') : '';
        const mockingboards = machine.mockingboards ?? Number(page('mockingboards').value);
        playGame({ kind: 'apple', model, roms, disks: machine.disks, mockingboards });
    };
    const key = artFor(title, ...machine.disks);
    const media = machine.disks.every(isVolume) ? 'hard' : 'floppy';
    button.append(cover(key, title, media), caption(art.get(key)?.title ?? title, art.get(key)));
    item.append(button, starButton(machine, title));
    return item;
}

/// When a game was last played, `at` seconds since 1970: today, yesterday,
/// days ago this week, or the day.
function whenPlayed(at) {
    const then = new Date(at * 1000);
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const days = Math.ceil((today - then) / 86_400_000);
    if (then >= today) {
        return 'Today';
    }
    if (days <= 1) {
        return 'Yesterday';
    }
    if (days < 7) {
        return `${days} days ago`;
    }
    const year = then.getFullYear() === today.getFullYear() ? undefined : 'numeric';
    return then.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year });
}

/// How long it's been played, `seconds` in all: in hours and minutes.
function howLong(seconds) {
    const minutes = Math.round(seconds / 60);
    if (minutes < 1) {
        return 'under a minute';
    }
    const hours = Math.floor(minutes / 60);
    return hours ? `${hours} h ${minutes % 60} min` : `${minutes} min`;
}

/// The shelf of games, however long, is only as many tiles as are in view
/// and a few rows either side, the space for the rest being kept: hundreds
/// of them take a browser seconds to draw, and make scrolling heavy.
let shelf = [];
const tiles = new Map();
let shelfRow = 0;
let shelfPlaced = -1;
let shelfSize = 0;

/// A row's height, and how many tiles are in one: as the CSS's grid has it.
function shelfShape(list) {
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
    const tv = document.documentElement.classList.contains('tv');
    const [smallest, across, down, words] = [tv ? TV_TILE : SMALLEST_TILE, GAP_ACROSS, GAP_DOWN, CAPTION].map(
        (size) => size * rem,
    );
    const width = list.clientWidth;
    if (tv) {
        // Whole rows, as many as the screen's height has room for at the
        // TV's size, but two at least: the tiles as big as that lets them be,
        // as many across as fit, the shelf centred.
        const room = innerHeight - document.querySelector('.toolbar').getBoundingClientRect().height - 2 * down;
        const rows = Math.max(2, Math.floor((room + down) / (smallest * COVER_SHAPE + words + down)));
        const row = (room - (rows - 1) * down) / rows;
        const tile = Math.min((row - words) / COVER_SHAPE, width);
        return { columns: Math.max(1, Math.floor((width + across) / (tile + across))), row, down, tile, rows };
    }
    const columns = Math.max(1, Math.floor((width + across) / (smallest + across)));
    const tile = (width - across * (columns - 1)) / columns;
    return { columns, row: tile * COVER_SHAPE + words, down, tile };
}

/// Gamepad or arrow keys around the library. On the shelf of games a tile's
/// selected, by name; anywhere else, the page's buttons, choices, ticks and
/// the Apple's disks are moved between by where they are. Held directions
/// repeat.
/// Nothing's taken until all's let go, coming back from a game, so that the
/// A that ended it, say, doesn't start another.
let selectedGame = null;
/// What's moved to off the shelf of games, if anything is.
let padFocus = null;
let moving = '';
let moveAgainAt = 0;
let wasPressed = {};
let navigationWaits = false;
let padsSaid = null;
const FIRST_REPEAT = 350;
const NEXT_REPEAT = 110;

const appleShowing = () => !page('apple-shelf').hidden;
const gamesShowing = () => !page('nes').hidden;

/// What's pressed for moving around, on any gamepad: its directions, Select
/// and Start as the first player's are bound; confirming and going back by
/// its maker's convention. In the app, what the emulator says, which it
/// hasn't yet, just back from a game; on Android, the webview's own.
function navigationPressed() {
    if (native?.tellsPads) {
        return padsSaid;
    }
    const pressed = {};
    const shoulders = (pad, button) => Boolean(pad.buttons[button]?.pressed);
    for (const pad of padsNow()) {
        const state = padState(pad);
        const steered = {
            up: state.stickY < -0.5,
            down: state.stickY > 0.5,
            left: state.stickX < -0.5,
            right: state.stickX > 0.5,
        };
        for (const action of ['up', 'down', 'left', 'right', 'select', 'start']) {
            pressed[action] ||= steered[action] || boundHeld(0, action, pad, state);
        }
        // Choosing and going back as the menus' are bound, not the game's A
        // and B: an Xbox pad's, the bottom and the right, unless they've
        // been changed.
        pressed.a ||= menuHeld('menuAccept', pad, state);
        pressed.b ||= menuHeld('menuBack', pad, state);
        pressed.previous ||= shoulders(pad, 4);
        pressed.next ||= shoulders(pad, 5);
        pressed.pageup ||= shoulders(pad, 6);
        pressed.pagedown ||= shoulders(pad, 7);
    }
    return pressed;
}

/// Whether it's a gamepad that's going round the library: not the mouse, nor
/// the keyboard.
let padInUse = false;

/// Whose a pad is, by its name: Sony's, Nintendo's, or, as most are laid
/// out, an Xbox pad's. In the desktop app, whose pads the page doesn't see,
/// as it says.
let padMaker = 'xbox';
function makerOf(pad) {
    const id = pad?.id ?? '';
    if (/054c|sony|playstation|dualshock|dualsense/i.test(id)) {
        return 'playstation';
    }
    return /057e|nintendo|pro controller|joy-con/i.test(id) ? 'nintendo' : 'xbox';
}

/// The menus' choosing and going back, as they're bound: the page's own,
/// in a browser; in the app, the emulator's, as its controls have them, by
/// the page's names for its buttons.
let menuFromApp = null;
const menuSources = (action) => menuFromApp?.[action] ?? controlsBound[0][action];

/// The emulator's names for a gamepad's buttons, as the page names them:
/// by where they are, as an Xbox pad's are labelled.
const FROM_EMULATOR = {
    a: 'a',
    b: 'east',
    x: 'b',
    y: 'north',
    back: 'select',
    start: 'start',
    dpup: 'padUp',
    dpdown: 'padDown',
    dpleft: 'padLeft',
    dpright: 'padRight',
    lefttrigger: 'leftTrigger',
    righttrigger: 'rightTrigger',
    leftshoulder: 'btn4',
    rightshoulder: 'btn5',
    leftstick: 'btn10',
    rightstick: 'btn11',
    guide: 'btn16',
};

/// The menus' buttons from the app, as its controls have them now.
async function learnMenus() {
    const told = await native?.menuButtons().catch(() => null);
    if (!told) {
        return;
    }
    const ours = (sources) =>
        sources.flatMap((source) => {
            const name = FROM_EMULATOR[source.replace(/^pad:/, '')];
            return source.startsWith('pad:') && name ? [`pad:${name}`] : [];
        });
    menuFromApp = { menuAccept: ours(told.accept), menuBack: ours(told.back) };
    showPadHints();
}
learnMenus();

/// Whether a gamepad holds what's bound to a menu's `action`.
const menuHeld = (action, pad, state) =>
    menuSources(action).some((source) => source.startsWith('pad:') && bindings.padHeld(source, pad, state));

/// What a pad's face buttons are labelled, by its maker, where they are.
const FACES = {
    xbox: { a: 'A', east: 'B', b: 'X', north: 'Y' },
    playstation: { a: '✕', east: '○', b: '□', north: '△' },
    nintendo: { a: 'B', east: 'A', b: 'Y', north: 'X' },
};

/// The library's buttons on the pad in use, by its maker's labels: those
/// that choose and go back, as the menus' are bound; and its Select and
/// Start, shoulders and triggers.
function padNames() {
    // The page's own pad, but in the desktop app, which says whose it is.
    const told = native?.tellsPads;
    const pad = told ? null : padsNow()[0];
    const maker = told ? padMaker : makerOf(pad);
    const named = (action, or) => {
        const source = menuSources(action).find((source) => source.startsWith('pad:'));
        const name = source?.slice('pad:'.length);
        return FACES[maker][name] ?? (source ? bindings.said(source).replace(/^Pad /, '') : or);
    };
    const [confirm, back] = [named('menuAccept', FACES[maker].a), named('menuBack', FACES[maker].east)];
    if (maker === 'playstation') {
        const select = /dualsense|0ce6/i.test(pad?.id ?? '') ? 'Create' : 'Share';
        return { confirm, back, select, start: 'Options', shoulders: 'L1 R1', triggers: 'L2 R2' };
    }
    if (maker === 'nintendo') {
        return { confirm, back, select: '−', start: '+', shoulders: 'L R', triggers: 'ZL ZR' };
    }
    return { confirm, back, select: 'Select', start: 'Start', shoulders: 'LB RB', triggers: 'LT RT' };
}

/// What the gamepad's buttons do here: on a game, a disk, the strip of
/// letters, anything else moved to, or in a game's details.
function padHints() {
    const { confirm: A, back: B, select, start, shoulders, triggers } = padNames();
    const changes = padFocus instanceof HTMLSelectElement || padFocus?.matches('.setting');
    const press = [A, changes ? 'Change' : 'Press'];
    if (settingsOpen()) {
        return [press, [B, 'Close']];
    }
    const shelves = [shoulders, 'Shelves'];
    if (padFocus?.closest('#letters')) {
        return [['↑ ↓', 'Letters'], [A, 'Back to the games'], shelves];
    }
    const tile = padFocus ? padFocus.closest('.tile') : gamesShowing() ? tiles.get(selectedGame) : null;
    if (tile?.closest('#disks')) {
        return [[A, 'Choose'], [start, 'Start the Apple'], shelves];
    }
    if (tile) {
        return [
            [A, 'Play'],
            ...(tile.dataset.game && !isStandIn(tile.dataset.game) ? [[start, 'Details']] : []),
            [select, 'Favourite'],
            ...(!padFocus && gamesShowing() ? [[triggers, 'Page']] : []),
            shelves,
        ];
    }
    if (padFocus) {
        const back = gamesShowing() ? 'Back to the games' : padFocus.closest('#settings') && 'Back';
        return [press, ...(back ? [[B, back]] : []), shelves];
    }
    return [shelves];
}

/// The gamepad's buttons, what they do here, along the foot of the library
/// while it's a gamepad that's in use; in a game's details, under them. Made
/// again only when they've changed.
function showPadHints() {
    const bar = page('pad-hints');
    bar.hidden = !padInUse || page('library').hidden || page('controls-dialog').open;
    if (bar.hidden) {
        return;
    }
    // In a game's details, above its buttons, which keep to its foot.
    if (settingsOpen()) {
        const actions = page('game-settings').querySelector(':scope > .row-actions');
        if (bar.nextElementSibling !== actions) {
            actions.before(bar);
        }
    } else if (bar.parentElement !== document.body) {
        document.body.append(bar);
    }
    const hints = padHints().filter(([button]) => button);
    const said = JSON.stringify(hints);
    if (bar.dataset.said !== said) {
        bar.dataset.said = said;
        bar.replaceChildren(
            ...hints.map(([button, what]) => {
                const hint = document.createElement('span');
                const name = document.createElement('b');
                name.textContent = button;
                hint.append(name, what);
                return hint;
            }),
        );
    }
}

/// The selection shown: a tile's, on the shelf of games or a disk's; or
/// what else is moved to, ringed.
function showSelected() {
    document.querySelectorAll('.tile.selected').forEach((tile) => tile.classList.remove('selected'));
    document.querySelectorAll('.pad-focus').forEach((element) => element.classList.remove('pad-focus'));
    if (!padFocus) {
        tiles.get(selectedGame)?.classList.add('selected');
    } else if (padFocus.closest('.tile')) {
        padFocus.closest('.tile').classList.add('selected');
    } else {
        padFocus.classList.add('pad-focus');
    }
    showPadHints();
}

/// How much of the screen's foot the gamepad's hints cover, if they're
/// showing over the shelf, and a little more: what's scrolled to, above it.
function hintsCover() {
    const bar = page('pad-hints');
    if (bar.hidden || bar.parentElement !== document.body) {
        return 0;
    }
    return innerHeight - bar.getBoundingClientRect().top + 8;
}

/// The selection on the shelf of games moved, and scrolled to.
function moveInGames(way) {
    const list = page('games');
    const { columns, row, down, rows } = shelfShape(list);
    const at = shelf.indexOf(selectedGame);
    if (at < 0) {
        // The first press takes the first tile in view.
        selectedGame = shelf[Math.min(shelfRow, Math.ceil(shelf.length / columns) - 1) * columns] ?? shelf[0];
    } else {
        const toolbar = document.querySelector('.toolbar').getBoundingClientRect().height;
        const page = Math.max(1, Math.floor((innerHeight - toolbar) / (row + down))) * columns;
        const step = { up: -columns, down: columns, left: -1, right: 1, pageup: -page, pagedown: page }[way] ?? 0;
        selectedGame = shelf[Math.min(shelf.length - 1, Math.max(0, at + step))];
    }
    const height = row + down;
    const rowAt = Math.floor(shelf.indexOf(selectedGame) / columns);
    const scrolled = scroller.scrollTop;
    const top = list.getBoundingClientRect().top + scrolled + rowAt * height;
    const toolbar = document.querySelector('.toolbar').getBoundingClientRect().height;
    if (top < scrolled + toolbar) {
        scroller.scrollTo(0, top - toolbar - down);
    } else if (top + row > scrolled + innerHeight - hintsCover()) {
        // The TV's shelf, whole rows to the screen, lined up by the top one.
        scroller.scrollTo(
            0,
            rows ? top - toolbar - down - (rows - 1) * height : top + row + down + hintsCover() - innerHeight,
        );
    }
    placeShelf();
    showSelected();
}

/// What can be moved to off the shelf of games: the toolbar's buttons and
/// choice, the page's buttons, choices and ticks, the Apple's disks; not the
/// search, which wants typing, nor a tile's remove button.
function focusable() {
    if (settingsOpen()) {
        return [...page('game-settings').querySelectorAll('select, button')];
    }
    const all = document.querySelectorAll(
        [
            '.toolbar button',
            '.toolbar select',
            '.shelf:not([hidden]) button',
            '.shelf:not([hidden]) select',
            '.shelf:not([hidden]) summary',
            '.shelf:not([hidden]) label.setting',
            '.shelf:not([hidden]) label.button',
            '#disks .tile label',
        ].join(),
    );
    return [...all].filter(
        (element) =>
            !element.closest('#games') &&
            !element.matches('.remove, .star, .setting select') &&
            !element.disabled &&
            (element.checkVisibility?.({ visibilityProperty: true, checkVisibilityCSS: true }) ??
                element.getClientRects().length > 0),
    );
}

/// The nearest of `options` from `from` the way `way` goes: by how far
/// along it, and more by how far off to the side, if they're not side by
/// side at all. A row as wide as the page is under all of what's above it.
function nearest(from, way, options) {
    const at = from.getBoundingClientRect();
    const [x, y] = [at.left + at.width / 2, at.top + at.height / 2];
    const apart = (start, end, otherStart, otherEnd) => Math.max(0, otherStart - end, start - otherEnd);
    let best = null;
    let bestScore = Infinity;
    for (const option of options.filter((option) => option !== from)) {
        const there = option.getBoundingClientRect();
        const [dx, dy] = [there.left + there.width / 2 - x, there.top + there.height / 2 - y];
        const across = apart(at.left, at.right, there.left, there.right);
        const updown = apart(at.top, at.bottom, there.top, there.bottom);
        const [along, aside] = { up: [-dy, across], down: [dy, across], left: [-dx, updown], right: [dx, updown] }[
            way
        ] ?? [0, 0];
        const score = along + 2 * aside;
        if (along > 1 && score < bestScore) {
            [best, bestScore] = [option, score];
        }
    }
    return best;
}

/// What stays in view over the shelf as it scrolls: the toolbar, the
/// Apple's choices, and the strip of letters.
const pinned = (element) => element.closest('.toolbar, .fields, #letters');

/// `element` moved to, and scrolled to, unless it's pinned, and so in view:
/// its tile all of it, and clear of what's pinned over it, not under it.
function focusOn(element) {
    padFocus = element;
    // In a game's details, the browser's focus there too: one ring, not two.
    if (settingsOpen()) {
        element.focus({ preventScroll: true });
    }
    if (element.closest('#letters')) {
        jumpToLetter(element.dataset.letter);
    }
    // A section of Settings moved to, shown: what's moved to is what's there.
    if (element.closest('#sections') && !element.classList.contains('on')) {
        element.click();
    }
    showSelected();
    if (pinned(element)) {
        return;
    }
    const box = element.closest('.tile') ?? element;
    box.scrollIntoView({ block: 'nearest' });
    const bars = document.querySelectorAll('.toolbar, .shelf:not([hidden]) .fields');
    const covered = Math.max(...[...bars].map((bar) => bar.getBoundingClientRect().bottom));
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
    const top = box.getBoundingClientRect().top - GAP_DOWN * rem;
    const bottom = box.getBoundingClientRect().bottom + hintsCover();
    if (top < covered) {
        scroller.scrollBy(0, top - covered);
    } else if (bottom > innerHeight) {
        scroller.scrollBy(0, bottom - innerHeight);
    }
}

/// Where a move from `from` the way `way` goes, of `options`. The sidebar's
/// shelves only to the left of what's beside them, not up or down off the
/// toolbar or the shelf. And onto what's pinned over the shelf only once
/// there's nothing more that way on it: the row above, scrolled away under
/// it, before what's over it.
function towards(from, way, options) {
    const sideways = from.closest('#sidebar') || way === 'left';
    const choices = sideways ? options : options.filter((option) => !option.closest('#sidebar'));
    const unpinned = choices.filter((option) => !pinned(option));
    return (!pinned(from) && nearest(from, way, unpinned)) || nearest(from, way, choices);
}

/// Back on the shelf of games' own selection.
function backToShelf() {
    padFocus = null;
    moveInGames('here');
}

/// A move: on the shelf of games, tile to tile, up off its top row onto the
/// toolbar; anywhere else, to the nearest that way, down off the bottom of
/// the toolbar onto the shelf of games.
function move(way) {
    if (gamesShowing() && !padFocus && !settingsOpen()) {
        const { columns } = shelfShape(page('games'));
        const at = shelf.indexOf(selectedGame);
        const tile = tiles.get(selectedGame);
        const above = way === 'up' && at >= 0 && at < columns && tile && towards(tile, 'up', focusable());
        // Right off the last of a row, onto the strip of letters, at this one's.
        const lastOfRow = at >= 0 && (at % columns === columns - 1 || at === shelf.length - 1);
        if (way === 'right' && lastOfRow && !page('letters').hidden) {
            return focusOn(page('letters').querySelector(`[data-letter="${letterOf(selectedGame)}"]`));
        }
        // Left off the first of a row, onto the shelf's own tab, the sidebar's
        // to go along.
        const tab = document.querySelector('#sidebar button.on');
        if (way === 'left' && at % columns === 0 && tab?.checkVisibility()) {
            return focusOn(tab);
        }
        return above ? focusOn(above) : moveInGames(way);
    }
    // Left off the strip, back to the shelf, at its letter.
    if (way === 'left' && padFocus?.closest('#letters')) {
        return backToShelf();
    }
    if (padFocus && !page('settings').hidden) {
        return moveInSettings(way);
    }
    // Right off the sidebar, onto the shelf it shows: its games, or what's
    // on it, not the toolbar over it.
    if (way === 'right' && padFocus?.closest('#sidebar')) {
        if (gamesShowing() && shelf.length) {
            return backToShelf();
        }
        const onShelf = focusable().filter((option) => !pinned(option) && !option.closest('#sidebar'));
        const next = nearest(padFocus, 'right', onShelf);
        if (next) {
            return focusOn(next);
        }
    }
    // Down the sidebar's along it, to the shelf it shows, as a TV remote has
    // no shoulder buttons to change shelves with.
    if (way === 'down' && padFocus?.closest('.toolbar') && !padFocus.closest('#sidebar') && gamesShowing()) {
        return backToShelf();
    }
    const options = focusable();
    if (!options.includes(padFocus)) {
        const disk = options.find((option) => option.closest('#disks'));
        return focusOn(disk ?? options.find((option) => !option.closest('.toolbar')) ?? options[0]);
    }
    const next = towards(padFocus, way, options);
    if (next) {
        focusOn(next);
    } else if (way === 'down' && gamesShowing() && !settingsOpen()) {
        backToShelf();
    }
}

/// A move in Settings: right off the sidebar onto its sections, along them
/// each showing its own, right again onto that one's settings, and left
/// back the way it came; up and down only along each, never off it, nor
/// onto the toolbar over them.
function moveInSettings(way) {
    const sections = [...page('sections').querySelectorAll('button')];
    const section = sections.find((button) => button.classList.contains('on'));
    const settings = focusable().filter((option) => option.closest('.pane'));
    // A phone's sections go across, over its settings.
    const across = getComputedStyle(page('sections')).flexDirection === 'row';
    const [back, on, into] = across ? ['left', 'right', 'down'] : ['up', 'down', 'right'];
    if (padFocus.closest('#sidebar')) {
        const shelves = focusable().filter((option) => option.closest('#sidebar'));
        const next = way === 'right' ? section : nearest(padFocus, way, shelves);
        return next && focusOn(next);
    }
    if (padFocus.closest('#sections')) {
        if (way === back || way === on) {
            const next = sections[sections.indexOf(padFocus) + (way === back ? -1 : 1)];
            return next && focusOn(next);
        }
        if (way === into) {
            const first = rowsOf(settings)[0]?.[0];
            return first && focusOn(first);
        }
        const tab = document.querySelector('#sidebar button.on');
        return way === 'left' && tab?.checkVisibility() && focusOn(tab);
    }
    // A section's own, a row at a time, down the page as it reads; along
    // a row, its buttons side by side.
    const rows = rowsOf(settings);
    const at = rows.findIndex((row) => row.includes(padFocus));
    // From anywhere else, as the toolbar over a phone's: its first.
    if (at < 0) {
        return focusOn(rows[0]?.[0] ?? section);
    }
    const centre = (element) => {
        const { left, right } = element.getBoundingClientRect();
        return (left + right) / 2;
    };
    if (way === 'up' || way === 'down') {
        const row = rows[at + (way === 'up' ? -1 : 1)];
        if (!row) {
            if (way === 'up') {
                scroller.scrollTo(0, 0);
                return across && focusOn(section);
            }
            return;
        }
        const x = centre(padFocus);
        focusOn(row.reduce((best, item) => (Math.abs(centre(item) - x) < Math.abs(centre(best) - x) ? item : best)));
        // The first row: the section's top, its heading, with it.
        if (at + (way === 'up' ? -1 : 1) === 0) {
            scroller.scrollTo(0, 0);
        }
        return;
    }
    const along = rows[at];
    const next = along[along.indexOf(padFocus) + (way === 'left' ? -1 : 1)];
    if (next) {
        return focusOn(next);
    }
    if (way === 'left') {
        return focusOn(section);
    }
}

/// `elements` in rows, as they're laid out: those side by side together,
/// the rows from the top, each from the left.
function rowsOf(elements) {
    const rows = [];
    const placed = elements
        .map((element) => [element, element.getBoundingClientRect()])
        .sort(([, a], [, b]) => a.top - b.top || a.left - b.left);
    for (const [element, box] of placed) {
        const last = rows.at(-1);
        // Side by side: overlapping, top to bottom, by most of the shorter.
        const overlap = last && Math.min(box.bottom, last.bottom) - Math.max(box.top, last.top);
        if (last && overlap > Math.min(box.height, last.bottom - last.top) / 2) {
            last.items.push(element);
            last.bottom = Math.max(last.bottom, box.bottom);
        } else {
            rows.push({ items: [element], top: box.top, bottom: box.bottom });
        }
    }
    return rows.map((row) => row.items.sort((a, b) => a.getBoundingClientRect().left - b.getBoundingClientRect().left));
}

/// Select pressed: the selected game's star, a favourite or not.
function starSelected() {
    const tile = padFocus ? padFocus.closest('.tile') : gamesShowing() && tiles.get(selectedGame);
    tile?.querySelector('.star')?.click();
}

/// A press of confirm: the selected game started; or what's moved to
/// pressed, a disk ticked, a choice on to its next option.
function confirmSelected() {
    const choice =
        padFocus instanceof HTMLSelectElement
            ? padFocus
            : padFocus?.matches('.setting') && padFocus.querySelector('select');
    if (choice) {
        choice.selectedIndex = (choice.selectedIndex + 1) % choice.options.length;
        choice.dispatchEvent(new Event('change'));
    } else if (padFocus?.closest('#letters')) {
        backToShelf(); // there already: back on the shelf, at its first
    } else if (padFocus) {
        padFocus.click();
    } else if (gamesShowing()) {
        tiles.get(selectedGame)?.querySelector('.play').click();
    }
}

/// The next shelf, or the one before, by the shoulder buttons: the gamepad
/// starting afresh on it, but for the shelf of games' own selection.
function nextTab(step) {
    const shelves = [...document.querySelectorAll('.tabs button[data-tab]')].filter((tab) => !tab.hidden);
    const at = shelves.findIndex((tab) => tab.classList.contains('on'));
    padFocus = null;
    shelves[(at + step + shelves.length) % shelves.length].click();
}

/// The pointer hidden while a gamepad or the arrow keys are going round the
/// library, and what's under it not lit up as if it were chosen; back once
/// the mouse moves.
const pointerGone = (gone) => document.body.classList.toggle('pointer-gone', gone);
/// Where the pointer was last, for the gamepad to take up from.
let pointerAt = null;
addEventListener('mousemove', (event) => {
    if (event.movementX || event.movementY) {
        pointerGone(false);
        pointerAt = [event.clientX, event.clientY];
        padInUse = false;
        showPadHints();
    }
});

/// Whether `element` is in view: on the screen, below what's pinned over the
/// shelf; or pinned, or the sidebar's, and always there.
function inView(element) {
    if (!element?.isConnected) {
        return false;
    }
    if (pinned(element) || element.closest('#sidebar')) {
        return true;
    }
    const bars = document.querySelectorAll('.toolbar, .shelf:not([hidden]) .fields');
    const covered = Math.max(...[...bars].map((bar) => bar.getBoundingClientRect().bottom));
    const { top, bottom } = (element.closest('.tile') ?? element).getBoundingClientRect();
    return bottom > covered && top < innerHeight;
}

/// `tile` the selection: on the shelf of games, its game; elsewhere, what's
/// moved to on it.
function selectTile(tile) {
    if (tile.closest('#games')) {
        selectedGame = tile.dataset.game;
        padFocus = null;
    } else {
        padFocus = focusable().find((element) => tile.contains(element)) ?? padFocus;
    }
    showSelected();
}

/// The gamepad, or the arrow keys, taking up from where the mouse left
/// things: from the tile under the pointer, if it's been in use and is on
/// one; else, if what they had has been scrolled out of view, from the first
/// tile in view, or whatever else is, that press only showing it. Whether
/// it was taken so.
function takeUp() {
    if (settingsOpen()) {
        return false;
    }
    const pointing = !document.body.classList.contains('pointer-gone') && pointerAt;
    const under = pointing && document.elementFromPoint(...pointerAt)?.closest('.shelf:not([hidden]) .tile');
    if (under) {
        selectTile(under);
        return false;
    }
    if (inView(padFocus ?? (gamesShowing() ? tiles.get(selectedGame) : null))) {
        return false;
    }
    const tile = [...document.querySelectorAll('.shelf:not([hidden]) .tile')].find(inView);
    if (tile) {
        selectTile(tile);
        return true;
    }
    // In Settings, its section that's showing.
    const section = !page('settings').hidden && page('sections').querySelector('button.on');
    const other =
        section || focusable().find((element) => !pinned(element) && !element.closest('#sidebar') && inView(element));
    if (other) {
        padFocus = other;
        showSelected();
    }
    return Boolean(other);
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
    if (pageDialog()) {
        padInDialog(pageDialog(), pressed, now);
        wasPressed = pressed;
        return;
    }
    const way = ['up', 'down', 'left', 'right', 'pageup', 'pagedown'].find((direction) => pressed[direction]) ?? '';
    // A first press, after the mouse or a scroll: from where they left it.
    const pressing = Object.values(pressed).some(Boolean);
    if (pressing && !Object.values(wasPressed).some(Boolean) && takeUp()) {
        pointerGone(true);
        padInUse = true;
        showPadHints();
        [moving, moveAgainAt, wasPressed] = [way, now + FIRST_REPEAT, pressed];
        return;
    }
    if (pressing) {
        pointerGone(true);
        padInUse = true;
    }
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
        goBack();
    } else if (newly('start') && appleShowing()) {
        navigationWaits = true;
        page('apple').click();
    } else if (newly('start') && !settingsOpen()) {
        settingsOfSelected();
    } else if (newly('select')) {
        starSelected();
    } else if (newly('previous')) {
        nextTab(-1);
    } else if (newly('next')) {
        nextTab(1);
    }
    if (pressing) {
        showPadHints();
    }
    wasPressed = pressed;
}

/// Back, as a gamepad's B has it, and a TV remote's Back, in the app on
/// Android: a message put away, out of a game's settings, onto the shelf
/// of games, or the search cleared. Whether there was anywhere to go back
/// from.
function goBack() {
    const dialog = pageDialog();
    if (dialog) {
        backInDialog(dialog);
        return true;
    }
    if (!page('message').hidden) {
        say('');
        return true;
    }
    if (settingsOpen()) {
        page('game-settings').close();
        return true;
    }
    if (padFocus && gamesShowing()) {
        backToShelf();
        return true;
    }
    // In Settings, out of a section's own, out of its sections.
    if (padFocus?.closest('.pane')) {
        focusOn(page('sections').querySelector('button.on'));
        return true;
    }
    const tab = document.querySelector('#sidebar button.on');
    if (padFocus?.closest('#sections') && tab?.checkVisibility()) {
        focusOn(tab);
        return true;
    }
    const search = page('filter');
    if (search.value) {
        search.value = '';
        refresh();
        return true;
    }
    return false;
}
window.openBusBack = goBack;

addEventListener('keydown', (event) => {
    // A switch clicked keeps the focus, but takes no typing.
    const typing = document.activeElement?.matches('input:not([type="checkbox"]), select, textarea');
    const dialog = page('controls-dialog').open || pageDialog();
    if (emulator || typing || page('library').hidden || dialog || settingsOpen()) {
        return;
    }
    // A TV remote's keys are named, but they're no keyboard's.
    const key = event.code || event.key;
    const way = { ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right' }[key];
    // The arrows the page's: what the browser had focused, a button pressed
    // or a dialog's opener given it back, isn't what Enter presses now.
    if (way && document.activeElement !== document.body) {
        document.activeElement.blur();
    }
    const confirming = key === 'Enter' && (selectedGame || padFocus) && document.activeElement === document.body;
    if (way || confirming) {
        event.preventDefault();
        padInUse = false;
        const taken = takeUp();
        pointerGone(true);
        if (taken) {
            return;
        }
    }
    if (way) {
        move(way);
    } else if (confirming) {
        confirmSelected();
    }
});

/// The tiles for where the page is scrolled to.
function placeShelf(force = true) {
    const list = page('games');
    if (!list.clientWidth) {
        return;
    }
    const shape = shelfShape(list);
    const { columns, row, down, tile } = shape;
    list.style.setProperty('--columns', columns);
    list.style.setProperty('--tile', `${tile}px`);
    const rows = Math.ceil(shelf.length / columns);
    const scrolled = Math.max(0, -list.getBoundingClientRect().top);
    const first = Math.max(0, Math.floor(scrolled / (row + down)) - SHELF_EXTRA);
    // The row nearest the toolbar's foot, the top one shown.
    const below = document.querySelector('.toolbar').getBoundingClientRect().bottom - list.getBoundingClientRect().top;
    showLetterAt(Math.max(0, Math.round(below / (row + down))) * columns);
    const count = Math.ceil(innerHeight / (row + down)) + 2 * SHELF_EXTRA + 1;
    if (!force && first === shelfRow && shelfPlaced === columns && shelfSize === row) {
        return;
    }
    [shelfRow, shelfPlaced, shelfSize] = [first, columns, row];
    list.style.setProperty('--row', `${row}px`);
    list.style.paddingTop = `${first * (row + down)}px`;
    // On the TV's shelf, whole screenfuls, so that the last lines up too.
    const filled = shape.rows ? Math.ceil(rows / shape.rows) * shape.rows : rows;
    list.style.minHeight = `${Math.max(0, filled * (row + down) - down)}px`;
    const inView = shelf.slice(first * columns, (first + count) * columns).map((name) => {
        if (!tiles.has(name)) {
            tiles.set(name, isX16Itself(name) ? x16Tile() : name === BUSTERM_TILE ? bustermTile() : gameTile(name));
        }
        return tiles.get(name);
    });
    // Only the tiles coming and going: taking them all off and putting them
    // back has WebKit draw every cover again, and they flicker as the shelf
    // scrolls.
    const staying = new Set(inView);
    for (const element of [...list.children]) {
        if (!staying.has(element)) {
            element.remove();
        }
    }
    let next = list.firstElementChild;
    for (const element of inView) {
        if (element === next) {
            next = next.nextElementSibling;
        } else {
            list.insertBefore(element, next);
        }
    }
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
(native ? scroller : window).addEventListener('scroll', placeShelfSoon, { passive: true });
addEventListener('resize', placeShelfSoon);

/// In the desktop app, the mouse wheel and the keys that scroll glide the
/// library here, a little each frame: WebKit on Linux scrolls ahead of
/// drawing what it brings in, the sidebar and toolbar jumping about. Scrolled
/// from here, as a gamepad does, each step's drawn before it's shown.
if (native) {
    const LINE = 40;
    let target = null;
    let left = 0; // where the last step left it
    const glide = () => {
        if (target === null) {
            return;
        }
        const now = scroller.scrollTop;
        if (Math.abs(now - left) > 1) {
            target = null; // scrolled some other way meanwhile
            return;
        }
        // A quarter of the way there each frame, but at least a pixel.
        const gap = target - now;
        const to = Math.abs(gap) <= 2 ? target : now + Math.sign(gap) * Math.max(1, Math.abs(gap) / 4);
        scroller.scrollTo(0, to);
        left = scroller.scrollTop;
        if (to === target || left === now) {
            target = null; // there, or as far as it goes
        } else {
            requestAnimationFrame(glide);
        }
    };
    /// Glided to `to`, from where it's going if it's moving already.
    const glideTo = (to) => {
        const moving = target !== null;
        target = Math.max(
            0,
            Math.min(scroller.scrollHeight - scroller.clientHeight, to(moving ? target : scroller.scrollTop)),
        );
        if (!moving) {
            left = scroller.scrollTop;
            requestAnimationFrame(glide);
        }
    };
    addEventListener(
        'wheel',
        (event) => {
            if (event.ctrlKey || event.target.closest('#hidden-games')) {
                return; // a zoom, or a list scrolling in a box of its own
            }
            event.preventDefault();
            const by = event.deltaY * [1, LINE, scroller.clientHeight][event.deltaMode];
            glideTo((from) => from + by);
        },
        { passive: false },
    );
    addEventListener('keydown', (event) => {
        const on = document.activeElement;
        if (/^(INPUT|SELECT|TEXTAREA)$/.test(on?.tagName) || event.ctrlKey || event.altKey || event.metaKey) {
            return;
        }
        // A screenful, less the toolbar over its top.
        const screen = scroller.clientHeight - document.querySelector('.toolbar').getBoundingClientRect().height - LINE;
        const space = event.code === 'Space' && on === document.body; // not a button's press
        const to = {
            PageDown: (from) => from + screen,
            PageUp: (from) => from - screen,
            Space: space && ((from) => from + (event.shiftKey ? -screen : screen)),
            Home: () => 0,
            End: () => Infinity,
        }[event.code];
        if (to) {
            event.preventDefault();
            glideTo(to);
        }
    });
}

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
    blank.style.setProperty('--hue', 180 + (hue % 170));
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
    const titled = (game) => knownOf(game)?.title ?? diskTitle(game);
    const shown = arranged('apple', all.filter(matches), titled, knownOf);
    const list = page('disks');
    list.nextElementSibling.textContent = wanted
        ? 'Nothing matches.'
        : 'No disks yet. Add disk images, or a folder of them.';
    showCount('tab-apple', 'Apple II', shown.length);
    list.replaceChildren(
        ...shown.flatMap((game) => {
            const open = game.disks.length > 1 && (opened.has(game.title) || wanted);
            return open ? [diskTile(game, wanted), diskList(game)] : [diskTile(game, wanted)];
        }),
    );
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
    label.dataset.tip = several ? title : game.disks[0];
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
    label.append(
        box,
        cover(key, title, media),
        caption(art.get(key)?.title ?? title, art.get(key), several ? '' : diskNote(game.disks[0])),
        drive,
    );
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
        const { verb, question } = removing(game.disks[0], `${game.title} and its ${game.disks.length} disks`);
        remove.title = `${verb} ${game.title}`;
        remove.setAttribute('aria-label', remove.title);
        remove.onclick = async () => {
            if (await sure(question)) {
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
    item.append(starButton({ kind: 'apple', disks: game.disks }, title));
    return item;
}

/// A game's disks, opened, across the shelf.
function diskList(game) {
    const item = document.createElement('li');
    item.className = 'opened';
    const list = document.createElement('ul');
    list.replaceChildren(
        ...game.disks.map((name) => {
            const row = document.createElement('li');
            row.append(diskBox(name), removeButton(name));
            return row;
        }),
    );
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

/// Whether `question`'s answered yes: in the app by the system's own dialog,
/// as its webview's `confirm` goes on without waiting for an answer.
const sure = (question) =>
    native?.android ? askInPage(question) : native ? native.ask(question) : Promise.resolve(confirm(question));

/// `question` asked in the page, not by Android's own dialog: as the rest of
/// the page is, with a gamepad, its buttons where its maker has them. Whether
/// it was OK'd.
function askInPage(question) {
    const dialog = page('ask-dialog');
    page('ask-question').textContent = question;
    let yes = false;
    page('ask-no').onclick = () => dialog.close();
    page('ask-yes').onclick = () => {
        yes = true;
        dialog.close();
    };
    return new Promise((resolve) => {
        dialog.onclose = () => {
            letGoOfFocus();
            resolve(yes);
        };
        dialog.showModal();
        page('ask-yes').focus();
    });
}

/// A dialog of the page's own closed, the browser's focus not given back
/// to what opened it, for a remote's OK to press: its selection's the page's.
const letGoOfFocus = () => document.activeElement?.blur();

/// The page's own dialog that's open, of those a gamepad goes round as it
/// does: asking, the folders, the X16's ROM.
const pageDialog = () => ['ask-dialog', 'folder-dialog', 'x16-rom-dialog'].map(page).find((dialog) => dialog.open);

/// What taking `name` out of the library is, and what's asked first, of
/// `what`: in the app a folder's game is only hidden, its own copy deleted;
/// in a browser, its copy there deleted.
function removing(name, what) {
    if (native && !native.isOwn(name)) {
        return {
            verb: 'Hide',
            question: `Hide ${what}? The files stay in their folder, and Settings can bring them back.`,
        };
    }
    return {
        verb: 'Remove',
        question: `Remove ${what} from ${native ? 'the library? Its copy there is deleted.' : 'this browser?'}`,
    };
}

function removeButton(name) {
    const remove = document.createElement('button');
    const { verb, question } = removing(name, shortName(name));
    remove.className = 'remove';
    remove.textContent = '×';
    remove.title = `${verb} ${shortName(name)}`;
    remove.setAttribute('aria-label', remove.title);
    remove.onclick = async () => {
        if (await sure(question)) {
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
    page('roms').querySelector('summary').textContent = short
        ? `Still needs ${short} ROM${short > 1 ? 's' : ''}`
        : plural(roms.filter((rom) => rom.here).length);
    page('rom-list').replaceChildren(
        ...roms.map(({ name, here }) => {
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
        }),
    );
    const disksChosen = chosen.length;
    page('picked').textContent = disksChosen
        ? `${disksChosen} disk${disksChosen > 1 ? 's' : ''} chosen`
        : 'No disks chosen';
    page('disks')
        .querySelectorAll('label')
        .forEach((label) => {
            const box = label.querySelector('input');
            const drive = label.querySelector('.drive');
            if (box.dataset.game) {
                const disks = JSON.parse(box.dataset.game);
                const places = disks.map((disk) => chosen.indexOf(disk)).filter((at) => at >= 0);
                box.checked = places.length === disks.length;
                box.indeterminate = places.length > 0 && !box.checked;
                label.classList.toggle('some', box.indeterminate);
                drive.textContent = places.length
                    ? `${Math.min(...places) + 1}–${Math.max(...places) + 1}`.replace(/^(\d+)–\1$/, '$1')
                    : '';
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
    const hadGames = hasGames();
    // Covers and game lists apart, and what's no use, as videos are.
    const artFiles = [...given, ...dropped].filter((file) => isMedia(file.name));
    const list = [...given].filter((file) => !isMedia(file.name));
    const inFolders = dropped.filter((file) => !isMedia(file.name));
    // The Apple's ROMs out of it first, whatever they're called.
    const sorter = sorted ? null : new RomSorter();
    const isOneOfThem = async (file) =>
        sorter && mayHoldRoms(file) && sorter.add(file.name, new Uint8Array(await file.arrayBuffer()));
    const notRoms = async (files) => {
        const rest = [];
        for (const file of files) {
            if (!(await isOneOfThem(file))) {
                rest.push(file);
            }
        }
        return rest;
    };
    say('Looking through them…');
    const [loose, others] = [await notRoms([...list]), await notRoms(inFolders)];
    const unused = loose.filter((file) => !isUsable(file.name));
    const fromFolders = others.filter((file) => isUsable(file.name));
    // Where each's kept: by its name; or a game whose name another machine's
    // game has, in its machine's folder, "atari7800/Joust (USA).zip", both
    // kept. Whose each name is, a game's machine asked only when another's
    // comes with its name.
    const owners = new Map(
        files.map((name) => [name, isGame(name) ? () => machineSaid(name, () => bytes(name)) : null]),
    );
    const ownerOf = async (name) => {
        const owner = owners.get(name);
        if (typeof owner === 'function') {
            owners.set(name, await owner());
        }
        return owners.get(name);
    };
    const fileBytes = async (file) => new Uint8Array(await file.arrayBuffer());
    /// Where `file`, named `name`, is to be kept, and whether there's one of
    /// its machine's there already.
    async function placeOf(file, name) {
        const own = isGame(name) ? () => machineSaid(name, () => fileBytes(file)) : null;
        if (!owners.has(name)) {
            owners.set(name, own);
            return { name, there: false };
        }
        if (!own) {
            return { name, there: true };
        }
        const machine = await own();
        if ((await ownerOf(name)) === machine) {
            return { name, there: true };
        }
        const apart = `${machine}/${name}`;
        const there = owners.has(apart);
        owners.set(apart, machine);
        return { name: apart, there };
    }
    // Those chosen one by one, in place of one of their machine's of the
    // name.
    const keptAs = new Map();
    for (const file of loose.filter((file) => isUsable(file.name))) {
        keptAs.set(file, (await placeOf(file, file.name)).name);
    }
    const usable = [...keptAs.keys()];
    // What each's stored as: in its game's folder, if it's in one; and
    // copies in a folder under a game's left out.
    const stored = storedNames(
        fromFolders.map((file) => file.path),
        (path) => isDisk(path),
    );
    const nameOf = (file) => (file.path === undefined ? file.name : stored.get(file.path));
    const added = [];
    const repeated = [];
    const addedNames = new Set();
    for (const file of fromFolders.filter((file) => nameOf(file) !== null)) {
        const { name, there } = await placeOf(file, nameOf(file));
        if (!there) {
            added.push(file);
            addedNames.add(name);
            keptAs.set(file, name);
        } else if (addedNames.has(name)) {
            repeated.push(file);
        }
    }
    const copies = fromFolders.filter((file) => nameOf(file) === null);
    usable.push(...added);
    say('');
    try {
        for (const [at, file] of usable.entries()) {
            say(`Adding ${at + 1} of ${usable.length}…`);
            await keep(keptAs.get(file), new Uint8Array(await file.arrayBuffer()));
            if (!native) {
                keeper.forgetFacts([keptAs.get(file)]);
            }
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
    await refresh();
    // The first games: their shelf shown, not Settings, where they came from.
    if (!hadGames && hasGames()) {
        showShelf(gamesShelf());
    } else if (!page('settings').hidden) {
        fillSettings();
    }
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
        found.push(...(await folderFiles(under)));
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
                inFolders.push(...(await folderFiles(entry)));
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

for (const input of document.querySelectorAll('.add-files')) {
    input.onchange = async (event) => {
        await add(event.target.files);
        event.target.value = '';
    };
}
/// A folder chosen rather than dropped: its files by their paths, as a
/// dropped one's are, those nearest the top first.
page('add-folder').onchange = async (event) => {
    const depth = (file) => file.path.split('/').length;
    const inFolder = [...event.target.files];
    for (const file of inFolder) {
        file.path = `/${file.webkitRelativePath}`;
    }
    await add(
        [],
        inFolder.sort((a, b) => depth(a) - depth(b)),
    );
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
    const roms = model === 'apple2' ? (plusRom() ?? '') : '';
    playGame({ kind: 'apple', model, roms, disks: chosen, mockingboards });
};

/// In the desktop app, a game's on the native emulator, not here: a game
/// over, what it's saved is in the library, and what went wrong is told.
function playNatively(machine) {
    if (machine.game && kind(machine.game) === 'fds' && !files.includes(FDS_BIOS)) {
        say(`Disk System games need the BIOS, ${FDS_BIOS}: add it in Settings.`, true);
        return;
    }
    const fullscreen = machine.kind !== 'controls' && (page('start-fullscreen').checked || page('tv-layout').checked);
    const fast = page('performance-mode').checked;
    const ram = page('power-on-ram').value;
    const resume = machine.kind !== 'controls' && page('quick-resume').checked;
    const rewind = page('rewind').checked;
    const driveLight = page('drive-light').checked;
    const vsync = page('vsync').checked;
    native
        .play({ ...machine, fullscreen, fast, ram, resume, rewind, driveLight, vsync })
        .catch((problem) => say(String(problem), true));
}
native?.whenPads((pressed, maker) => {
    padsSaid = pressed;
    padMaker = maker;
});
/// The TV layout: everything bigger, for across a room, the window
/// fullscreen, and the pointer out of the way until the mouse moves.
function tvLayout(on) {
    document.documentElement.classList.toggle('tv', on);
    native?.fullscreen(on);
    pointerGone(on);
    placeShelf(true);
}

native?.whenGameEnds((problem) => {
    // Its controls perhaps changed in the game.
    learnMenus();
    if (page('tv-layout').checked) {
        native.fullscreen(true);
    }
    navigationWaits = true;
    padsSaid = null;
    say(problem ?? '', Boolean(problem));
    refresh();
});

/// A folder of the desktop app's, remembered, and its games staying where
/// they are: what's taken from it once are the machines' ROMs, into the
/// library, and the covers and game lists beside its games.
async function addFolder(folder) {
    // Said at once: a network share's folders take a while to look through.
    say(`Looking through ${folder}…`);
    const found = await native.folderFiles(folder);
    // Its games shown as soon as they're found; the system ROMs among its
    // files, and its covers, made out after, while they're there to play,
    // each of those files having to be read, and a share's slow to.
    await native.addFolderFiles(found);
    await refresh();
    if (!page('settings').hidden) {
        showShelf(gamesShelf());
    }
    const count = (number, what) => `${number} ${what}${number === 1 ? '' : 's'}`;
    const added = () => `Added ${count(native.gamesIn(folder), 'game')} from ${folder}.`;
    say(`${added()} Looking for covers and system ROMs…`);
    const romFiles = found.map(({ path, size }) => ({ path, size, name: baseName(path) })).filter(mayHoldRoms);
    const { held, roms } = await native.sortRoms(romFiles.map(({ path }) => path));
    for (const path of held) {
        await native.hide(path); // a ROM, not a game
    }
    const media = found.map(({ path }) => native.laterFile(path)).filter(({ name }) => isMedia(name));
    const made = await takeArt(media, art, (at, total) => say(`${added()} Making covers: ${at} of ${total}…`));
    await loadArt();
    await refresh();
    const also = (number, what) => (number ? ` ${count(number, what)}.` : '');
    say(`${added()}${also(roms.length, 'system ROM')}${also(made, 'cover')}`);
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
    const picked = native.android
        ? browseFolders().then((path) => path && native.addFolder(path))
        : native.pickFolder();
    const folder = await picked.catch((problem) => say(String(problem), true));
    if (folder) {
        await addNatively([folder], []);
        fillSettings();
    }
}

/// A folder chosen in the page's own list of them, as the app on Android has
/// no folder picker fit for a TV: from where folders are (the device's
/// storage, a drive put in), down through them. Its path, or nothing.
function browseFolders() {
    const dialog = page('folder-dialog');
    const list = page('folder-list');
    let [here, chosen] = [null, null];
    // The app let read them first, by Android's settings.
    const show = async (path, from = null) => {
        const allowed = await native.storageAccess().catch(() => false);
        page('folder-access').hidden = allowed;
        list.hidden = !allowed;
        if (!allowed) {
            page('folder-use').disabled = true;
            page('folder-allow').focus();
            return;
        }
        let found;
        try {
            found = await native.foldersIn(path);
        } catch (problem) {
            say(String(problem), true);
            return;
        }
        here = path;
        page('folder-where').textContent = path ?? 'Where are your games?';
        // Rows all: this one added, up a folder, into one; as a remote or
        // a gamepad goes down a list.
        const item = (text, action, kind = '') => {
            const row = document.createElement('li');
            const button = document.createElement('button');
            button.textContent = text;
            if (kind) {
                button.className = kind;
            }
            button.onclick = action;
            row.append(button);
            return row;
        };
        const into = found.folders.map((folder) => {
            const row = item(folder.name, () => show(folder.path, null));
            row.firstChild.dataset.path = folder.path;
            return row;
        });
        const adding = path === null ? [] : [item('+ Add this folder', () => page('folder-use').click(), 'use')];
        const up = path === null ? [] : [item('‹ Up a folder', () => show(found.up, path), 'up')];
        list.replaceChildren(...adding, ...up, ...into);
        page('folder-use').disabled = path === null;
        // The one come up from, or the first to go into; else this one added.
        const back = into.find((row) => row.firstChild.dataset.path === from)?.firstChild;
        (back ?? into[0]?.firstChild ?? list.querySelector('button') ?? page('folder-cancel')).focus();
    };
    // Back from Android's settings, it's looked at again.
    const returned = () => {
        if (dialog.open && !document.hidden) {
            show(here);
        }
    };
    page('folder-allow').onclick = () => native.askStorageAccess().catch((problem) => say(String(problem), true));
    page('folder-cancel').onclick = () => dialog.close();
    page('folder-use').onclick = () => {
        chosen = here;
        dialog.close();
    };
    document.addEventListener('visibilitychange', returned);
    return new Promise((resolve) => {
        dialog.onclose = () => {
            document.removeEventListener('visibilitychange', returned);
            letGoOfFocus();
            resolve(chosen);
        };
        dialog.showModal();
        show(null);
    });
}

/// A dialog of the page's own gone round by a gamepad: its buttons by its
/// directions, the browser's focus moved, held directions repeating as
/// they do on the shelf; A presses the one there, and B goes back.
function padInDialog(dialog, pressed, now) {
    const newly = (name) => pressed[name] && !wasPressed[name];
    const buttons = [...dialog.querySelectorAll('button:not([disabled])')].filter(
        (button) => button.checkVisibility?.() ?? button.getClientRects().length > 0,
    );
    const from = buttons.includes(document.activeElement) ? document.activeElement : null;
    const held = ['up', 'down', 'left', 'right'].find((direction) => pressed[direction]) ?? '';
    let way = '';
    if (held !== moving) {
        [moving, moveAgainAt, way] = [held, now + FIRST_REPEAT, held];
    } else if (held && now >= moveAgainAt) {
        [moveAgainAt, way] = [now + NEXT_REPEAT, held];
    }
    if (way) {
        const next = from ? nearest(from, way, buttons) : buttons[0];
        next?.focus();
        next?.scrollIntoView({ block: 'nearest' });
    } else if (newly('a')) {
        from?.click();
    } else if (newly('b')) {
        backInDialog(dialog);
    }
}

/// Back, in a dialog of the page's own: up a folder, in the list of them,
/// if it's not at the top; else out of it.
function backInDialog(dialog) {
    const up = dialog.querySelector('button.up');
    if (up) {
        up.click();
    } else {
        dialog.close();
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

/// Settings: the system files, as they are now; and in the desktop app,
/// the folders and what's hidden.
async function fillSettings() {
    const has = (name) => files.includes(name);
    const appleFound = [...DISK_II_ROMS, ...IIE_ROMS.apple2ee].filter(has).length;
    const x16 = has(X16_ROM)
        ? `Found: ${await x16Release()}`
        : "Missing: X16 games need a release's rom.bin, or its zip";
    page('system-files').replaceChildren(
        row('Disk System BIOS', has(FDS_BIOS) ? 'Found' : 'Missing: Disk System games need disksys.rom'),
        row('Apple II ROMs', `${appleFound} of ${DISK_II_ROMS.length + IIE_ROMS.apple2ee.length} found`),
        row(
            'Atari 7800 High Score Cartridge',
            has(HIGH_SCORE_ROM)
                ? 'Found: plugged in for every game'
                : 'Not added: it keeps high scores, for the games that know of it',
        ),
        has(X16_ROM)
            ? row('Commander X16 ROM', x16)
            : row('Commander X16 ROM', x16, 'Get it', async () => (await getX16Rom()) && fillSettings()),
    );
    if (native) {
        fillFolders();
        fillSaves();
        fillX16Files();
    }
}

/// The kinds of save, as Settings and a game's details name them: all of a
/// kind, and one.
const SAVE_KINDS = {
    save: ['Game saves', 'Game save'],
    state: ['Save states', 'Save state'],
    resume: ['Resume points', 'Resume point'],
    card: ['X16 cards', 'X16 card, with what it saves'],
    disk: ['Disk changes', 'Disk changes'],
};

/// `bytes` as a person reads them.
const sizeOf = (bytes) =>
    bytes < 1024
        ? `${bytes} bytes`
        : bytes < 1048576
          ? `${Math.round(bytes / 1024)} KB`
          : `${(bytes / 1048576).toFixed(1)} MB`;

/// Settings' saves: where they're kept, to be changed; and how many of each
/// kind there are, each kind to be cleared, asked first.
async function fillSaves(overview) {
    let saves = overview;
    try {
        saves ??= await native.savesOverview();
    } catch (problem) {
        page('saves-where-now').textContent = "a folder that can't be read";
        say(`Saves: ${problem}`);
        return;
    }
    page('saves-where-now').textContent = saves.folder;
    page('saves-default').hidden = !saves.chosen;
    const counted = (number, what) => `${number} ${what}${number === 1 ? '' : 's'}`;
    page('saves-kinds').replaceChildren(
        ...Object.entries(SAVE_KINDS)
            .filter(([kind]) => ['save', 'state', 'resume'].includes(kind) || saves.kinds[kind])
            .map(([kind, [name]]) => {
                const { count = 0, bytes = 0 } = saves.kinds[kind] ?? {};
                if (!count) {
                    return row(name, 'None');
                }
                return row(name, `${counted(count, 'file')}, ${sizeOf(bytes)}`, 'Clear', async () => {
                    const sure = await native.ask(
                        `Delete all ${counted(count, name.toLowerCase().replace(/s$/, ''))}? This can't be undone.`,
                    );
                    if (sure) {
                        fillSaves(await native.clearSaves([kind]));
                    }
                });
            }),
    );
}

/// The folder saves are kept in changed, what's there moved, asked first.
async function moveSaves(path) {
    const where = path ?? "the app's own folder";
    if (!(await native.ask(`Keep saves in ${where} from now on? Those there are now are moved there.`))) {
        return;
    }
    try {
        fillSaves(await native.setSavesFolder(path));
        say('Saves moved');
    } catch (problem) {
        say(`Saves not moved: ${problem}`);
    }
}
/// The X16's files folder, in Settings: where it is, opened, changed.
async function fillX16Files(given) {
    let files = given;
    try {
        files ??= await native.x16Files();
    } catch (problem) {
        page('x16-files-where').textContent = "a folder that can't be found";
        say(`The X16's files: ${problem}`);
        return;
    }
    page('x16-files-where').textContent = files.folder;
    page('x16-files-default').hidden = !files.chosen;
}
if (native) {
    page('x16-files-open').hidden = native.android;
    page('x16-files-open').onclick = async () => {
        try {
            await native.openX16Files();
        } catch (problem) {
            say(`The folder can't be opened: ${problem}`);
        }
    };
    // The folder changed, or its default again; or, refused, why.
    const x16FilesTo = async (path) => {
        try {
            fillX16Files(await native.setX16Files(path));
        } catch (problem) {
            say(`The X16's files not changed: ${problem}`, true);
        }
    };
    page('x16-files-change').onclick = async () => {
        const path = await (native.android ? browseFolders() : native.pickFolder());
        if (path) {
            await x16FilesTo(path);
        }
    };
    page('x16-files-default').onclick = () => x16FilesTo(null);
}
if (native) {
    page('saves-change').onclick = async () => {
        const path = await (native.android ? browseFolders() : native.pickFolder());
        if (path) {
            moveSaves(path);
        }
    };
    page('saves-default').onclick = () => moveSaves(null);
}

/// A game's own saves, in its details: each, with when it was saved, to be
/// deleted, asked first.
async function fillGameSaves(machine) {
    const list = page('game-saves');
    let saves;
    try {
        saves = await native.gameSaves(machine);
    } catch (problem) {
        list.replaceChildren(row("Its saves can't be read", String(problem)));
        return;
    }
    if (!saves.length) {
        list.replaceChildren(row('None yet'));
        return;
    }
    list.replaceChildren(
        ...saves.map((save) => {
            const [, name] = SAVE_KINDS[save.kind] ?? [save.kind, save.kind];
            const when = whenPlayed(save.written).toLowerCase();
            return row(name, `${sizeOf(save.bytes)}, saved ${when}`, 'Delete', async () => {
                if (await native.ask(`Delete this game's ${name.toLowerCase()}? This can't be undone.`)) {
                    await native.clearGameSaves(machine, [save.kind]);
                    fillGameSaves(machine);
                }
            });
        }),
    );
}

/// Which release the X16's ROM is, as its KERNAL says, at $FF80 in its
/// first bank: R49, or less than 0 for a pre-release of one.
async function x16Release() {
    try {
        const version = (await bytes(X16_ROM))[0x3f80];
        return version < 0x80 ? `release R${version}` : `a pre-release of R${0x100 - version}`;
    } catch {
        return 'its release not known';
    }
}

async function fillFolders() {
    const { folders, hidden, data } = await native.settings();
    const counted = (number, what) => `${number} ${what}${number === 1 ? '' : 's'}`;
    page('folders').replaceChildren(
        ...folders.map((folder) =>
            row(folder, counted(native.gamesIn(folder), 'game'), 'Remove', async () => {
                if (await sure(`Remove ${folder} from the library? Its files stay where they are.`)) {
                    await native.removeFolder(folder);
                    await refresh();
                    fillSettings();
                }
            }),
        ),
    );
    if (!folders.length) {
        page('folders').append(row('No folders yet. Add one to start your library.'));
    }
    // How many, and the list itself only when it's asked for, in a box of
    // its own height: hundreds of them would make Settings go on for ever.
    page('hidden-list').hidden = !hidden.length;
    page('nothing-hidden').hidden = hidden.length > 0;
    page('hidden-count').textContent = `${hidden.length} game${hidden.length === 1 ? '' : 's'} hidden`;
    // Gone from the list as soon as they're chosen; back on the shelves once
    // the app's said so.
    const restoring = async (paths, rows) => {
        rows.forEach((item) => item.remove());
        const left = page('hidden-games').children.length;
        page('hidden-count').textContent = `${left} game${left === 1 ? '' : 's'} hidden`;
        await native.restore(paths);
        await refresh();
        say(`${paths.length} game${paths.length === 1 ? '' : 's'} back in the library.`);
        fillSettings();
    };
    page('hidden-games').replaceChildren(
        ...hidden.map((path) => {
            const item = row(shortName(path), path, 'Restore', () => restoring([path], [item]));
            return item;
        }),
    );
    page('restore-all').onclick = () => restoring(hidden, [...page('hidden-games').children]);
    page('data-place').textContent = `Covers, saves and system files are kept in ${data}`;
}

/// The desktop app's own way of looking: a window of its own, not a page.
if (native) {
    document.documentElement.classList.add('native');
    addEventListener('contextmenu', (event) => event.preventDefault());
    native.whenDropped(({ folders, files: dropped }) => addNatively(folders, dropped).then(fillSettings));
    // The keys a menu bar would have, without one: Ctrl, or Cmd on a Mac,
    // with O to add a folder, R to look for new games, F to search, comma for
    // Settings, Q to quit. R's not the page reloaded either.
    addEventListener('keydown', (event) => {
        if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey || page('library').hidden) {
            return;
        }
        const shortcut = {
            KeyO: chooseFolder,
            KeyR: () => lookForChanges(true),
            KeyF: () => {
                showShelf(page('nes').hidden && page('apple-shelf').hidden ? gamesShelf() : shelfShowing);
                page('filter').focus();
            },
            Comma: () => showShelf('settings'),
            KeyQ: native.quit,
        }[event.code];
        if (shortcut) {
            event.preventDefault();
            shortcut();
        }
    });
    page('add-path').onclick = chooseFolder;
    page('rescan').onclick = () => lookForChanges(true);
    // A slip of the mouse, beside Settings, shouldn't close it: asked first.
    // Not Ctrl+Q, as nobody presses that by chance.
    page('quit-app').onclick = async () => {
        if (await sure('Quit Open Bus?')) {
            native.quit();
        }
    };
    native.whenFilesChange(async ({ came, went }) => {
        if (!came && !went) {
            return;
        }
        await refresh();
        const counted = (number, what) => `${number} ${what}${number === 1 ? '' : 's'}`;
        say(
            [came && `${counted(came, 'new game')}`, went && `${counted(went, 'game')} gone`]
                .filter(Boolean)
                .join(', ') + '.',
        );
        if (!page('settings').hidden) {
            fillSettings();
        }
    });
    page('quit-app').hidden = false;
    // On Android, all of it fullscreen, and big enough for a TV, already.
    if (native.android) {
        for (const setting of ['start-fullscreen', 'tv-layout']) {
            page(setting).closest('.setting').hidden = true;
        }
    }
    page('start-fullscreen').checked = localStorage.getItem('fullscreen') === 'yes';
    page('start-fullscreen').onchange = ({ target }) =>
        localStorage.setItem('fullscreen', target.checked ? 'yes' : 'no');
    page('performance-mode').checked = localStorage.getItem('performance') === 'yes';
    page('quick-resume').checked = localStorage.getItem('resume') === 'yes';
    page('tv-layout').checked = !native.android && localStorage.getItem('tv') === 'yes';
    page('tv-layout').onchange = ({ target }) => {
        localStorage.setItem('tv', target.checked ? 'yes' : 'no');
        tvLayout(target.checked);
    };
    if (page('tv-layout').checked) {
        tvLayout(true);
    }
    page('quick-resume').onchange = ({ target }) => localStorage.setItem('resume', target.checked ? 'yes' : 'no');
    page('performance-mode').onchange = ({ target }) =>
        localStorage.setItem('performance', target.checked ? 'yes' : 'no');
    page('vsync').checked = localStorage.getItem('vsync') !== 'no';
    page('vsync').onchange = ({ target }) => localStorage.setItem('vsync', target.checked ? 'yes' : 'no');
}
/// Settings' sections, one showing at a time: the last shown, again.
function showSection(name) {
    const sections = [...page('sections').querySelectorAll('button')];
    const shown = sections.find((button) => button.dataset.section === name) ?? sections[0];
    for (const button of sections) {
        button.classList.toggle('on', button === shown);
    }
    for (const pane of document.querySelectorAll('#settings .pane')) {
        pane.hidden = pane.id !== `pane-${shown.dataset.section}`;
    }
    localStorage.setItem('section', shown.dataset.section);
    scroller.scrollTo(0, 0);
}
for (const button of page('sections').querySelectorAll('button')) {
    button.onclick = () => showSection(button.dataset.section);
}
showSection(localStorage.getItem('section'));

// The Settings of both: what's to be read once a game's started.
page('change-controls').onclick = native ? () => playNatively({ kind: 'controls' }) : openControls;
page('rewind').checked = localStorage.getItem('rewind') !== 'no';
page('rewind').onchange = ({ target }) => localStorage.setItem('rewind', target.checked ? 'yes' : 'no');
page('drive-light').checked = localStorage.getItem('driveLight') !== 'no';
page('drive-light').onchange = ({ target }) => localStorage.setItem('driveLight', target.checked ? 'yes' : 'no');
page('power-on-ram').value = localStorage.getItem('ram') ?? 'ones';
page('power-on-ram').onchange = ({ target }) => localStorage.setItem('ram', target.value);
page('background').value = localStorage.getItem('background') ?? 'sunset';
page('background').onchange = ({ target }) => {
    localStorage.setItem('background', target.value);
    showScene();
};

/// The scene behind the library: the smooth one, or one in pixels, about as
/// many lines tall as the NES's picture and scaled up by whole pixels, more
/// of it to the sides in a wider window: as it's chosen in Settings, and
/// centred on the library, right of the sidebar, if there's room for one.
/// Or none, plain, the sidebar not frosted either: what older graphics,
/// drawing them in stripes, draw well.
function showScene() {
    const name = page('background').value;
    const scene = page('scene');
    document.documentElement.classList.toggle('plain', name === 'plain');
    scene.classList.toggle('pixel', name !== 'smooth');
    let canvas = scene.querySelector('canvas');
    if (name === 'smooth' || name === 'plain') {
        canvas?.remove();
        return;
    }
    if (!canvas) {
        canvas = document.createElement('canvas');
        scene.append(canvas);
    }
    // A window with no size yet, as Android's is opened behind its screen
    // saver: drawn once it has one, as it's resized.
    if (!innerWidth || !innerHeight) {
        return;
    }
    const scale = Math.max(1, Math.round(innerHeight / 240));
    canvas.width = Math.ceil(innerWidth / scale);
    canvas.height = Math.ceil(innerHeight / scale);
    canvas.style.width = `${canvas.width * scale}px`;
    canvas.style.height = `${canvas.height * scale}px`;
    const sidebar = matchMedia(PHONE).matches ? 0 : page('sidebar').getBoundingClientRect().width;
    drawScene(name, canvas, (sidebar + innerWidth) / 2 / scale);
}
showScene();
// The page shown now, as it's to look: before, it was a browser's, with the
// smooth scene.
document.documentElement.classList.remove('loading');
let sceneDue = null;
addEventListener('resize', () => {
    clearTimeout(sceneDue);
    sceneDue = setTimeout(showScene, 100);
});

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
/// them, not the rest of the library. `game`, as it's kept, played from now.
async function start(make, machine, using, game) {
    if (starting) {
        return;
    }
    starting = true;
    say('Starting…');
    try {
        // Not the X16's cards, but the one it's given: they're big.
        for (const name of await names()) {
            if (using.includes(name) || (!isGame(name) && !isDisk(name) && !isCard(name))) {
                put_file(name, await bytes(name));
            }
        }
        emulator = make();
        playing = { game, since: Date.now() };
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
    showPadHints(); // the library's, gone with it
    page('machine').hidden = false;
    // A console's, the NES's or the 2600's, laid out as a console's; the
    // X16 as both, its keyboard's and its SNES pads'.
    page('machine').dataset.machine = ['apple', 'x16'].includes(machine) ? machine : 'console';
    page('x16-help').hidden = machine !== 'x16';
    rawKeys = emulator.raw_keys();
    hasMouse = emulator.has_mouse();
    page('disk').hidden = page('disk-help').hidden = !emulator.disk_system();
    page('switch-help').hidden = machine !== 'atari2600';
    page('switch-help-7800').hidden = machine !== 'atari7800';
    page('keypad-help').hidden = emulator.keypads() !== 1;
    page('keypads-help').hidden = emulator.keypads() !== 2;
    aiming = emulator.aimed_at();
    page('frame').classList.toggle('aiming', aiming);
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
    const parts = changed_files().map((name) => [
        name,
        [...take_changed_parts(name)].map((offset) => [offset, read_part(name, offset)]),
    ]);
    for (const [name, data] of whole) {
        await keep(name, data);
    }
    for (const [name, changed] of parts) {
        await keepParts(name, changed);
    }
}

/// The game being played here, as it's kept, and since when.
let playing = null;

async function stop() {
    navigationWaits = true;
    if (playing) {
        keeper.played(playing.game, playing.since / 1000, (Date.now() - playing.since) / 1000);
        playing = null;
    }
    await saveWritten();
    letGoKeys();
    emulator.free();
    emulator = null;
    held.clear();
    rawKeys = hasMouse = false;
    if (locked()) {
        document.exitPointerLock();
    }
    fastForward = rewinding = false;
    clearTimeout(pointerTimer);
    letGo(page('handheld'));
    // Its fullscreen gone with it: left, the browser would keep a hidden
    // machine fullscreen, and the library under it all, untouchable.
    if (page('machine').contains(document.fullscreenElement)) {
        document.exitFullscreen().catch(() => {});
    }
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
        north: button(west === 3 ? 2 : 3),
        select: button(back),
        start: button(start),
        rightTrigger: raw ? push(5) > 0.5 : button(7),
        leftTrigger: raw ? push(2) > 0.5 : button(6),
    };
}

/// Whether what's bound to `action` is held, for `who`, with gamepad `pad`.
/// Whether what's bound to `action` is held, for `who`, with gamepad `pad`:
/// not by the keyboard, if the machine has all its keys.
function boundHeld(who, action, pad, state) {
    return bindings.holds(controlsBound, who, action, (source) =>
        source.startsWith('key:')
            ? !rawKeys && held.has(source.slice('key:'.length))
            : bindings.padHeld(source, pad, state),
    );
}

/// A player's console buttons, a bit each: as they're bound, with the stick
/// steering, and what else presses them.
function padBits(who, pad, state, touched = {}) {
    const steered = {
        up: state.stickY < -0.5,
        down: state.stickY > 0.5,
        left: state.stickX < -0.5,
        right: state.stickX > 0.5,
    };
    const buttons = bindings.BUTTONS.map(
        (action) => touched[action] || steered[action] || boundHeld(who, action, pad, state),
    );
    // And the first player's, the menus', for the menu over a game.
    const menus = who === 0 ? ['menuAccept', 'menuBack'].map((action) => boundHeld(0, action, pad, state)) : [];
    return [...buttons, ...menus].reduce((bits, on, bit) => bits | (on ? 1 << bit : 0), 0);
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
    const x = towards(
        padLeft || key('Numpad4', 'Numpad7', 'Numpad1'),
        padRight || key('Numpad6', 'Numpad9', 'Numpad3'),
        stickX,
    );
    const y = towards(
        padUp || key('Numpad8', 'Numpad7', 'Numpad9'),
        padDown || key('Numpad2', 'Numpad1', 'Numpad3'),
        stickY,
    );
    const buttons =
        (first.a || key('Numpad0', 'AltLeft') ? 1 : 0) | (first.east || key('NumpadDecimal', 'AltRight') ? 2 : 0);
    const down_ = [...held].some((code) => !/^(Shift|Control|Alt|Meta)/.test(code));
    fastForward = boundHeld(0, 'fast', pads[0], first);
    rewinding = boundHeld(0, 'rewind', pads[0], first);
    emulator.controls(gamepad, gamepad2, x, y, buttons, down_, reset, fastForward, rewinding);
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
/// Whether what's bound to fast-forward is held: three frames run for one,
/// as the host's FAST has it.
let fastForward = false;
const FAST = 3;
/// And whether rewind is, which wins.
let rewinding = false;

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
    const speed = fastForward && !rewinding ? FAST : 1;
    const period = 1000 / emulator.hz() / speed;
    if (now - last > 250) {
        last = now - period;
    }
    let ran = false;
    for (let frames = 0; frames < 3 * speed && now - last >= period; frames++) {
        controls();
        emulator.frame(localSeconds());
        last += period;
        ran = true;
        putCopied();
        const samples = emulator.samples();
        player?.port.postMessage(samples, [samples.buffer]);
    }
    if (ran) {
        show();
        mouseWanted();
    }
    if (now > nextSave) {
        nextSave = now + 2000;
        saveWritten();
    }
}

/// Now, where the page is: seconds after 1970 began there.
const localSeconds = () => (Date.now() - new Date().getTimezoneOffset() * 60000) / 1000;

const ARROWS = { ArrowLeft: 0, ArrowRight: 1, ArrowUp: 2, ArrowDown: 3 };

/// Whether the machine takes the keys themselves, as they're pressed and let
/// go, by where they are: the X16's; and those held for it, to be let go.
let rawKeys = false;
const rawHeld = new Set();

/// The keys held for the machine let go, as the page's no longer typed in.
function letGoKeys() {
    for (const code of rawHeld) {
        emulator?.key_change(usageOf(code), false);
    }
    rawHeld.clear();
}

/// Ctrl, Shift, Alt and the logo keys let go for the machine, as the
/// clipboard's keys are pressed with them: they're the page's, and a paste
/// into an X16 with Ctrl held would scroll as slowly as its KERNAL scrolls
/// for Ctrl.
function letGoModifiers() {
    for (const code of rawHeld) {
        if (/^(Control|Shift|Alt|Meta)/.test(code)) {
            emulator?.key_change(usageOf(code), false);
            rawHeld.delete(code);
        }
    }
}
addEventListener('blur', letGoKeys);

addEventListener('keydown', (event) => {
    if (!emulator) {
        return;
    }
    const code = event.code;
    if ((code.startsWith('Numpad') && code !== 'NumpadEnter') || code.startsWith('Alt')) {
        held.add(code);
        event.preventDefault();
        return;
    }
    // Pasting and copying: Ctrl's the machine's, as Ctrl+C is, so Shift
    // too, as in a terminal, or Cmd on a Mac.
    event.preventDefault();
    const ctrl = event.ctrlKey || event.metaKey;
    const shifted = event.shiftKey || event.metaKey;
    if ((ctrl && shifted && code === 'KeyV') || (event.shiftKey && code === 'Insert')) {
        letGoModifiers();
        pasteClipboard();
        return;
    }
    if ((ctrl && shifted && code === 'KeyC') || (ctrl && code === 'Insert')) {
        letGoModifiers();
        emulator.copy_screen();
        return;
    }
    held.add(code);
    const number = /^F(\d+)$/.exec(code)?.[1];
    // The machine's keys, all but the page's own, F11 and F12, and the
    // clipboard's; the function keys the host's too, for those it has.
    if (rawKeys && code !== 'F11' && code !== 'F12' && usageOf(code) !== undefined) {
        rawHeld.add(code);
        emulator.key_change(usageOf(code), true);
        if (number && !event.repeat) {
            emulator.function_key(Number(number));
        }
        return;
    }
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

/// The screen's text, once it's copied, put on the clipboard.
function putCopied() {
    const text = emulator.copied();
    if (text !== undefined) {
        navigator.clipboard?.writeText(text).catch(() => emulator?.say("Couldn't copy: the browser said no"));
    }
}

/// What's on the clipboard typed on the machine's keyboard: asked of the
/// browser, as a key or the button's pressed, which it may not give a page.
/// The browser's own pasting isn't waited for: with nothing to type in
/// focused, not all browsers paste at all.
async function pasteClipboard() {
    try {
        const text = await navigator.clipboard.readText();
        emulator?.paste(text);
    } catch {
        emulator?.say("The browser won't share the clipboard");
    }
}

press('copy', () => emulator.copy_screen());
press('paste', pasteClipboard);

addEventListener('keyup', (event) => {
    held.delete(event.code);
    if (rawHeld.delete(event.code)) {
        emulator?.key_change(usageOf(event.code), false);
    }
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

// Not a light gun's: its double clicks are shots; nor a machine's with a
// mouse, its clicks its own.
page('screen').ondblclick = () => aiming || hasMouse || fullscreen();

/// Whether the machine has a mouse: the pointer locked to the picture for
/// it by a click there, the browser's Esc letting it go; its moves summed,
/// in the picture's dots, however far they go, and its buttons. Asked each
/// frame, as a machine may have one only while a program has it on.
let hasMouse = false;

/// Whether it has one now: let go, if it hasn't.
function mouseWanted() {
    const wanted = emulator.has_mouse();
    if (wanted !== hasMouse) {
        hasMouse = wanted;
        if (!wanted && locked()) {
            document.exitPointerLock();
        }
    }
}
const mouse = { x: 0, y: 0, buttons: 0 };
const locked = () => document.pointerLockElement === page('screen');
function tellMouse() {
    emulator?.pointer(mouse.x, mouse.y, locked(), locked() ? mouse.buttons : 0);
}
page('screen').addEventListener('mousedown', (event) => {
    if (!hasMouse) {
        return;
    }
    if (!locked()) {
        // Where it was clicked, the mouse's from there; that click not its.
        const canvas = page('screen');
        const box = canvas.getBoundingClientRect();
        mouse.x = ((event.clientX - box.left) / box.width) * canvas.width;
        mouse.y = ((event.clientY - box.top) / box.height) * canvas.height;
        canvas.requestPointerLock?.();
        return;
    }
    mouse.buttons = event.buttons & 3;
    tellMouse();
});
addEventListener('mouseup', (event) => {
    if (hasMouse && locked()) {
        mouse.buttons = event.buttons & 3;
        tellMouse();
    }
});
addEventListener('mousemove', (event) => {
    if (hasMouse && locked()) {
        const canvas = page('screen');
        const box = canvas.getBoundingClientRect();
        mouse.x += (event.movementX / box.width) * canvas.width;
        mouse.y += (event.movementY / box.height) * canvas.height;
        tellMouse();
    }
});
document.addEventListener('pointerlockchange', () => {
    mouse.buttons = 0;
    tellMouse();
});

/// Whether the machine's aimed at, a light gun's: the pointer on the picture
/// its, where it is and its buttons, the left (or a finger) and the right.
let aiming = false;

/// Where it was last aimed: a finger lifted leaves the picture as it goes,
/// and the game looks for the light a frame or two after, so it's still
/// aimed there, as a gun would be. Only a mouse aims away, leaving it.
let aimedAt = [0, 0, false];

function aim(event) {
    if (!aiming) {
        return;
    }
    const lifted = event.pointerType !== 'mouse' && ['pointerleave', 'pointercancel'].includes(event.type);
    if (!lifted) {
        const canvas = page('screen');
        const box = canvas.getBoundingClientRect();
        const x = ((event.clientX - box.left) / box.width) * canvas.width;
        const y = ((event.clientY - box.top) / box.height) * canvas.height;
        const on = event.type !== 'pointerleave' && x >= 0 && y >= 0 && x < canvas.width && y < canvas.height;
        aimedAt = [x, y, on];
    }
    emulator?.pointer(...aimedAt, event.buttons & 3);
}

for (const type of ['pointerdown', 'pointermove', 'pointerup', 'pointerleave', 'pointercancel']) {
    page('screen').addEventListener(type, aim);
}
// Held to the picture while a button's down, so that it's let go there too.
page('screen').addEventListener('pointerdown', (event) => aiming && page('screen').setPointerCapture(event.pointerId));
page('screen').addEventListener('contextmenu', (event) => aiming && event.preventDefault());

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
// Which Open Bus this is, at the foot of Settings: the desktop app's own, or
// this page's emulator's.
page('app-version').textContent = `Open Bus ${native ? await native.appVersion() : app_version()}`;

/// The version whose update was put off ("Not now"), not offered again by
/// itself: Settings' "Check for updates" still offers it.
const PUT_OFF_UPDATE = 'update put off';

/// A newer Open Bus, from its releases on GitHub, offered: installed, and the
/// app started again as it, if it's wanted. `byHand`, from Settings: what was
/// found said, even nothing.
async function offerUpdate(byHand) {
    let update;
    try {
        update = await native.updateCheck();
    } catch (problem) {
        if (byHand) {
            say(`Couldn't check for updates: ${problem}`, true);
        }
        return;
    }
    if (!update) {
        if (byHand) {
            say('This is the newest Open Bus.');
        }
        return;
    }
    // By itself, never over a game or another dialog: it asks again next
    // time the app starts.
    const busy = playing || document.querySelector('dialog[open]');
    if (!byHand && (busy || localStorage.getItem(PUT_OFF_UPDATE) === update.version)) {
        return;
    }
    const dialog = page('update-dialog');
    const status = page('update-status');
    page('update-version').textContent = update.version;
    page('update-have').textContent = await native.appVersion();
    page('update-notes').textContent = update.notes ?? '';
    page('update-notes').hidden = !update.notes;
    status.textContent = '';
    const buttons = [page('update-now'), page('update-later')];
    const working = (doing) => {
        status.textContent = doing;
        for (const button of buttons) {
            button.disabled = !!doing;
        }
    };
    working('');
    page('update-later').onclick = () => {
        localStorage.setItem(PUT_OFF_UPDATE, update.version);
        dialog.close();
    };
    page('update-now').onclick = async () => {
        working('Downloading…');
        const megabytes = (bytes) => `${(bytes / 2 ** 20).toFixed(1)} MB`;
        const stop = await native.whenUpdateProgresses(({ got, total }) => {
            status.textContent =
                total && got >= total
                    ? 'Installing…'
                    : total
                      ? `Downloading… ${Math.floor((got * 100) / total)}% of ${megabytes(total)}`
                      : `Downloading… ${megabytes(got)}`;
        });
        try {
            // It starts again as the new version, so this doesn't return.
            await native.updateInstall();
        } catch (problem) {
            working('');
            status.textContent = `Couldn't update: ${problem}`;
        } finally {
            stop();
        }
    };
    // Not closed while it's updating: the app would start again anyway.
    dialog.oncancel = (event) => page('update-now').disabled && event.preventDefault();
    dialog.showModal();
    page('update-now').focus();
}

// Updates: the desktop app's, not Android's (its APK's installed by hand) or
// a browser's.
if (native && !native.android) {
    page('update-actions').hidden = false;
    page('update-check').onclick = () => offerUpdate(true);
}
await loadArt();
await refresh();
// No games yet, and it starts in Settings, where they're added.
const lastShelf = localStorage.getItem('shelf');
// No games yet: Settings, where they're added.
if (!hasGames()) {
    showSection('library');
}
showShelf(!hasGames() ? 'settings' : lastShelf && lastShelf !== 'settings' ? lastShelf : gamesShelf());
// A newer version offered, once the library's showing.
if (native && !native.android) {
    offerUpdate(false);
}
// Whether busterm has a newer release than the library's, found meanwhile,
// never waited for: offered when it's next played.
if (native) {
    native
        .bustermUpdate()
        .then((update) => (newerBusterm = update))
        .catch(() => {});
}
if (native) {
    // With nothing known yet, the folders may yet turn something up.
    const startedEmpty = !hasGames();
    lookForChanges().then(() => {
        if (startedEmpty && hasGames()) {
            showShelf(gamesShelf());
        }
    });
}
requestAnimationFrame(tick);
