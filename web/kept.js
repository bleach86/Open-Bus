// What the page keeps of its games in a browser, as the desktop app's library
// database keeps it there (desktop.js has its side): the favourites, and the
// games played, how often, for how long in all and when last; what's chosen
// of a game for itself, its own settings; and what's known of each NES
// game's file without running it, as the emulator says, for as long as it's
// the emulator it was. In this browser's storage, by the games' names.

import { names } from './files.js';

const KEPT = 'kept-games';
const SETTINGS = 'game-settings';
const FACTS = 'game-facts';

/// What's kept under `key`, or `empty`: none's there, or the storage can't be
/// had, as in a private window.
function read(key, empty) {
    try {
        return JSON.parse(localStorage.getItem(key)) ?? empty;
    } catch {
        return empty;
    }
}

function write(key, value) {
    try {
        localStorage.setItem(key, JSON.stringify(value));
    } catch {
        // Not kept, then: the page goes on without.
    }
}

/// A game as it's kept, by its files: a console's game, a NES's or a 2600's;
/// an Apple's disks.
const gameKey = (machine) =>
    machine.kind === 'apple' ? `apple\n${machine.disks.join('\n')}` : `${machine.kind}\n${machine.game}`;

/// A machine as it's kept: a console's game; an Apple's disks, and its
/// model, ROMs and Mockingboards if it's been started.
function keptOf(machine) {
    if (machine.kind !== 'apple') {
        return { kind: machine.kind, game: machine.game };
    }
    const kept = { kind: 'apple', disks: machine.disks };
    for (const field of ['model', 'roms', 'mockingboards']) {
        if (machine[field] !== undefined) {
            kept[field] = machine[field];
        }
    }
    return kept;
}

/// The games that are favourites, or have been played, the last played
/// first: those whose files aren't here now left out, and an Apple played
/// with no disks, as for BASIC, which isn't a game.
export async function keptGames() {
    const here = new Set(await names());
    const present = ({ machine }) =>
        machine.kind === 'apple'
            ? machine.disks.length > 0 && machine.disks.every((disk) => here.has(disk))
            : here.has(machine.game);
    return Object.values(read(KEPT, {}))
        .filter((game) => (game.favourite || game.plays > 0) && present(game))
        .sort((a, b) => (b.last ?? -1) - (a.last ?? -1));
}

/// `machine` a favourite, `on`, or not; how it was last started kept.
export async function favourite(machine, on) {
    const kept = read(KEPT, {});
    const key = gameKey(machine);
    kept[key] = { machine: keptOf(machine), plays: 0, seconds: 0, last: null, ...kept[key], favourite: on };
    write(KEPT, kept);
}

/// `machine` played, started at `started` (seconds since 1970) and for
/// `seconds`: once more, for longer, and how it was started kept.
export function played(machine, started, seconds) {
    const kept = read(KEPT, {});
    const key = gameKey(machine);
    const was = kept[key] ?? { favourite: false, plays: 0, seconds: 0 };
    kept[key] = {
        ...was,
        machine: keptOf(machine),
        plays: was.plays + 1,
        seconds: was.seconds + Math.round(seconds),
        last: Math.floor(started),
    };
    write(KEPT, kept);
}

/// What a game can have of its own, and what each can be. Quick resume's the
/// desktop app's only.
const CHOICES = {
    region: ['ntsc', 'pal'],
    console: ['nes', 'famicom'],
    port2: ['controller', 'zapper'],
    ram: ['ones', 'zeros', 'random'],
};

/// What's chosen of `machine`'s game for itself, an object of those it has.
export async function gameSettings(machine) {
    return read(SETTINGS, {})[gameKey(machine)] ?? {};
}

/// `settings` chosen of `machine`'s game: those it can have, as it can have
/// them; none, and it's as it's known to be.
export async function chooseSettings(machine, settings) {
    const chosen = Object.fromEntries(
        Object.entries(settings).filter(([name, value]) => CHOICES[name]?.includes(value)),
    );
    const all = read(SETTINGS, {});
    if (Object.keys(chosen).length) {
        all[gameKey(machine)] = chosen;
    } else {
        delete all[gameKey(machine)];
    }
    write(SETTINGS, all);
}

/// What's known of the NES games, by name: each's mapper, whether it's a
/// Famicom's and a Zapper's, and why it won't load, if it won't. Those the
/// emulator there is now hasn't looked at, not there.
export async function gameFacts() {
    const { emulator, games } = read(FACTS, { emulator: null, games: {} });
    return new Map(emulator === (await emulatorNow()) ? Object.entries(games) : []);
}

/// Which emulator there is, as what it says of games goes: its WebAssembly's
/// size and when it was put there, as the server says; asked once.
let emulatorAsked = null;
function emulatorNow() {
    emulatorAsked ??= fetch('pkg/cpu_emu_bg.wasm', { method: 'HEAD' })
        .then(({ headers }) => `${headers.get('content-length')} ${headers.get('last-modified')}`)
        .catch(() => 'unknown');
    return emulatorAsked;
}

/// The facts of the games named `gone` forgotten, as their files are new.
export function forgetFacts(gone) {
    const facts = read(FACTS, { emulator: null, games: {} });
    for (const name of gone) {
        delete facts.games[name];
    }
    write(FACTS, facts);
}

/// The NES games named `games` that nothing's known of learned, in the
/// background: each's `bytesOf` read, and what `factsOf` says of them, a line
/// as `cpu_emu facts` says it, kept a few at a time, and `told` each time.
/// Asked while it's at it, it looks again after.
let learning = false;
let again = false;
export async function learnFacts(games, bytesOf, factsOf, told) {
    if (learning) {
        again = true;
        return;
    }
    learning = true;
    try {
        do {
            again = false;
            const emulator = await emulatorNow();
            const known = read(FACTS, { emulator: null, games: {} });
            const unknown = games.filter((name) => known.emulator !== emulator || !known.games[name]);
            let learned = {};
            for (let at = 0; at < unknown.length; at++) {
                const name = unknown[at];
                const [crc, mapper, famicom, zapper, problem] = factsOf(name, await bytesOf(name)).split('\t');
                learned[name] = { crc, mapper, famicom: famicom === '1', zapper: zapper === '1', problem };
                if (at % 50 === 49 || at === unknown.length - 1) {
                    // Put with what's kept now, not as it was: some may have
                    // been forgotten since; an old emulator's, all of it.
                    const now = read(FACTS, { emulator, games: {} });
                    const stored = now.emulator === emulator ? now.games : {};
                    write(FACTS, { emulator, games: { ...stored, ...learned } });
                    learned = {};
                    told();
                }
            }
        } while (again);
    } finally {
        learning = false;
    }
}
