// What's known of NES, Disk System and 2600 games without game lists beside them:
// their year, genre, developer and players, as libretro's database, RetroArch's
// own, has them, from its copy on GitHub. Its lists come whole and games are
// matched against them here, as covers are, so nothing's told of what's in
// the library. A game's matched by its ROMs' CRC, as the emulator has it,
// through No-Intro's list of names; else by its file's name; else by its title,
// as another region's. Asked for by the library itself, for those it hasn't
// looked up; those not found, looked up again after a while, the lists
// growing. In this browser's storage, or the desktop app's page's, by the
// games' names.

import { titleWords } from './art.js';

const LISTS = 'https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/';
const SETS = {
    nes: 'Nintendo - Nintendo Entertainment System',
    fds: 'Nintendo - Family Computer Disk System',
    atari2600: 'Atari - 2600',
};
/// Each detail's list, and what it's called in it.
const FIELDS = {
    year: ['releaseyear', 'releaseyear'],
    genre: ['genre', 'genre'],
    developer: ['developer', 'developer'],
    players: ['maxusers', 'users'],
};
const KEPT = 'game-details';
/// How long before those not found are looked up again.
const AGAIN = 30 * 24 * 60 * 60 * 1000;

function read() {
    try {
        return JSON.parse(localStorage.getItem(KEPT)) ?? { looked: 0, games: {} };
    } catch {
        return { looked: 0, games: {} };
    }
}

/// The details learned, by the games' names: those found.
export function learnedDetails() {
    return new Map(Object.entries(read().games).filter(([, details]) => Object.keys(details).length));
}

/// A list of the database's: its games, each a block of its fields.
async function entries(list, set) {
    const response = await fetch(`${LISTS}${list}/${encodeURIComponent(SETS[set])}.dat`);
    if (!response.ok) {
        throw new Error(`${response.status} from ${new URL(response.url).host}`);
    }
    return (await response.text()).split('\ngame (').slice(1);
}

/// What's known of `set`'s games: by the CRCs of their ROMs, with or without
/// a header, each's name; and by name, and by title, their details.
async function known(set) {
    const [names, ...fields] = await Promise.all([
        entries('no-intro', set),
        ...Object.values(FIELDS).map(([list]) => entries(list, set)),
    ]);
    const byCrc = new Map();
    for (const entry of names) {
        const name = /name "([^"]*)"/.exec(entry)?.[1];
        for (const [, crc] of entry.matchAll(/crc ([0-9A-F]{8})/g)) {
            if (name && !byCrc.has(crc)) {
                byCrc.set(crc, name);
            }
        }
    }
    const byName = new Map();
    Object.entries(FIELDS).forEach(([field, [, said]], at) => {
        const value = new RegExp(`^\\s*${said} "?([^"\\n]*)"?`, 'm');
        for (const entry of fields[at]) {
            const name = /comment "([^"]*)"/.exec(entry)?.[1];
            const found = value.exec(entry)?.[1].trim();
            if (name && found) {
                byName.set(name, { ...byName.get(name), [field]: found });
            }
        }
    });
    // Each title's details, of any of its releases, the USA's first.
    const byTitle = new Map();
    const usaFirst = [...byName].sort(([a], [b]) => b.includes('USA') - a.includes('USA'));
    for (const [name, details] of usaFirst) {
        const title = titleWords(name);
        byTitle.set(title, { ...details, ...byTitle.get(title) });
    }
    // The game's own, and what it hasn't, as its title's other releases have.
    return ({ name, crc }) => {
        const stem = name.replace(/^.*[/\\]/, '').replace(/\.[^.]*$/, '');
        const named = byCrc.get(crc) ?? stem;
        return { ...byTitle.get(titleWords(named)), ...byName.get(named) };
    };
}

/// The details of `games` learned, in the background, those not looked up
/// yet: each `{ name, crc, set }`, its CRC as the emulator's facts have it,
/// its set of SETS'.
/// `told` once they're kept. The lists fetched once, and kept while the page
/// is open, for games whose facts come later; unreachable, as offline, they're
/// tried again next time it's opened. One at a time, in turn.
let queue = Promise.resolve();
export function learnDetails(games, told) {
    queue = queue.then(() => learn(games, told));
    return queue;
}

const finding = new Map();
let failed = false;

async function learn(games, told) {
    const { looked, games: kept } = read();
    const stale = Date.now() - looked > AGAIN;
    const wanted = games.filter(({ name }) => !kept[name] || (stale && !Object.keys(kept[name]).length));
    if (failed || !wanted.length) {
        return;
    }
    try {
        const sets = [...new Set(wanted.map(({ set }) => set))];
        for (const set of sets) {
            finding.set(set, finding.get(set) ?? known(set));
        }
        const finders = Object.fromEntries(await Promise.all(sets.map(async (set) => [set, await finding.get(set)])));
        const now = read();
        for (const game of wanted) {
            now.games[game.name] = finders[game.set](game);
        }
        now.looked = Date.now();
        localStorage.setItem(KEPT, JSON.stringify(now));
        told();
    } catch (problem) {
        failed = true;
        console.warn("couldn't learn the games' details:", problem);
    }
}
