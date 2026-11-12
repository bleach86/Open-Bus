// Covers from online, when they're asked for. A source's catalogue comes
// whole and games are matched against it here, so that all it's told of
// what's in the library is which pictures to send. NES and 2600 boxes are
// libretro's, RetroArch's own, from their copy on GitHub, as their own
// server's closed to pages. It has no Apple II, so those are title screens from the Internet
// Archive. A game's title has to match: a floppy's better than a wrong cover.

import { keepArt } from './files.js';
import { shrink, tagsOf, titleWords } from './art.js';

const NES_BOXES = 'Nintendo_-_Nintendo_Entertainment_System';
const FDS_BOXES = 'Nintendo_-_Family_Computer_Disk_System';
const ATARI_2600_BOXES = 'Atari_-_2600';
const boxTrees = (set) => `https://api.github.com/repos/libretro-thumbnails/${set}/git/trees/`;
const boxFiles = (set) => `https://raw.githubusercontent.com/libretro-thumbnails/${set}/master/Named_Boxarts/`;
/// Git's mode of a symbolic link: the file's just the name of another.
const LINK = '120000';
const SCREENS =
    'https://archive.org/advancedsearch.php?q=collection:(wozaday OR softwarelibrary_apple_games)' +
    '&fl[]=identifier,title&rows=10000&output=json';
const ITEMS = 'https://archive.org/download/';
/// A box's regions, the best first.
const REGIONS = ['USA', 'World', 'Europe', 'Japan'];
/// How many are fetched at a time.
const AT_ONCE = 4;

/// A source that's said no: `reason`, to tell of.
class Refused extends Error {}

async function fetched(url) {
    const response = await fetch(url);
    if (response.status === 403 || response.status === 429) {
        throw new Refused(`${new URL(url).host} is asking us to slow down`);
    }
    return response;
}

const json = async (url) => {
    const response = await fetched(url);
    if (!response.ok) {
        throw new Error(`${response.status} from ${new URL(url).host}`);
    }
    return response.json();
};

/// A file of a box, as a name's kept: RetroArch's rule for its characters.
const safe = (name) => name.replace(/[&*/:`<>?\\|]/g, '_');

/// How good a box is for one of any region; none, if not the game as sold.
function regionRank(name) {
    const tags = tagsOf(name);
    if (/\b(Beta|Proto|Demo|Sample|Pirate)\b/.test(tags)) {
        return null;
    }
    const rank = REGIONS.findIndex((region) => tags.includes(region));
    return rank < 0 ? REGIONS.length : rank;
}

/// The boxes of `set`: what's looked up by a game's file.
async function boxes(set) {
    const { tree } = await json(`${boxTrees(set)}master`);
    const folder = tree.find((entry) => entry.path === 'Named_Boxarts');
    const files = (await json(`${boxTrees(set)}${folder.sha}`)).tree;
    const exact = new Map();
    const byTitle = new Map();
    for (const { path, mode } of files) {
        const stem = path.replace(/\.png$/, '');
        const box = { url: boxFiles(set) + encodeURIComponent(path), link: mode === LINK };
        exact.set(stem, box);
        const title = titleWords(stem);
        const rank = regionRank(stem);
        if (rank !== null && (byTitle.get(title)?.rank ?? Infinity) > rank) {
            byTitle.set(title, { box, rank });
        }
    }
    return ({ title, file }) => {
        const box = exact.get(safe(file.replace(/\.[^.]*$/, ''))) ?? byTitle.get(titleWords(title))?.box;
        return box ? [box] : [];
    };
}

/// A title's words, what's after a colon gone.
const words = (title) => titleWords(title.split(':')[0]);

/// A disk's title without the version, or the year and who cracked it.
const bareTitle = (title) => title.replace(/\s+v?\d+(\.\d+)+.*$/i, '').replace(/\s+19\d\d\b.*$/, '');

/// The Apple's title screens: what's looked up by a game's title.
async function screens() {
    const items = (await json(SCREENS)).response.docs.filter(
        ({ identifier, title }) => !/II\s?gs/i.test(`${identifier} ${title}`),
    );
    const byTitle = new Map();
    // The woz-a-day's first, whose pictures are the title screens.
    for (const { identifier, title } of items.sort(
        (a, b) => b.identifier.startsWith('wozaday') - a.identifier.startsWith('wozaday'),
    )) {
        const key = words(bareTitle(title));
        if (key && !byTitle.has(key)) {
            byTitle.set(key, identifier);
        }
    }
    return ({ title }) => {
        const item = byTitle.get(words(title)) ?? byTitle.get(words(bareTitle(title)));
        return item
            ? ['00playable_screenshot.png', '__ia_thumb.jpg'].map((file) => ({ url: `${ITEMS}${item}/${file}` }))
            : [];
    };
}

const SOURCES = {
    nes: () => boxes(NES_BOXES),
    fds: () => boxes(FDS_BOXES),
    atari2600: () => boxes(ATARI_2600_BOXES),
    apple: screens,
};

/// A picture, if it's there: a link's followed.
async function picture({ url, link }) {
    const response = await fetched(url);
    if (!response.ok) {
        return null;
    }
    if (link) {
        const target = await response.text();
        return picture({ url: new URL(encodeURIComponent(target.trim()), url).href });
    }
    return response.blob();
}

/// Covers for `wanted`, each `{key, machine, title, file}`, taken into `art`
/// and kept as they come: `told` after each, of how far it's got. What was
/// found, what wasn't, and why it stopped, if it did.
export async function findCovers(wanted, art, told) {
    const lookups = {};
    const problems = [];
    for (const machine of new Set(wanted.map((game) => game.machine))) {
        try {
            lookups[machine] = await SOURCES[machine]();
        } catch (error) {
            problems.push(error.message);
        }
    }
    const queue = wanted.filter((game) => lookups[game.machine]);
    const total = queue.length;
    let found = 0;
    let missed = 0;
    let stopped = false;
    const fetchNext = async () => {
        while (queue.length && !stopped) {
            const game = queue.shift();
            try {
                let blob = null;
                for (const place of lookups[game.machine](game)) {
                    blob = blob ?? (await picture(place));
                }
                if (blob) {
                    const kept = { ...art.get(game.key), cover: await shrink(blob), machine: game.machine };
                    art.set(game.key, kept);
                    await keepArt(new Map([[game.key, kept]]));
                    found++;
                } else {
                    missed++;
                }
            } catch (error) {
                missed++;
                if (error instanceof Refused) {
                    stopped = true;
                    problems.push(error.message);
                }
            }
            await told(found + missed, total, found);
        }
    };
    await Promise.all(Array.from({ length: AT_ONCE }, fetchNext));
    return { found, missed, problems };
}
