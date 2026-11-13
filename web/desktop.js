// The files kept on the disk, in the desktop app, where the page's own
// storage would be in a browser: what files.js does, by the app's commands.

import { storedNames } from './disks.js';

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const asBytes = (data) => new Uint8Array(data);
const slashes = (path) => path.replaceAll('\\', '/');
const baseName = (path) => slashes(path).split('/').pop();
const extension = (path) => path.split('.').pop().toLowerCase();
/// What's a game in a folder, and a disk.
const DISKS = ['dsk', 'do', 'po', 'nib', 'woz', 'hdv', '2mg'];
const GAMES = ['nes', 'fds', 'a26', 'a78', 'bin', 'zip', ...DISKS];

/// Where each name's file is: the page has names, the disk paths.
const where = new Map();

export async function keep(name, bytes) {
    where.set(name, await invoke('keep_file', bytes, { headers: { name: encodeURIComponent(name) } }));
}

/// The running machines' parts of a hard disk: they aren't here, a game
/// being the native emulator's, which writes its own.
export const keepParts = async () => {};

/// Gone from the library: its own copy deleted, or one in a folder of yours
/// only hidden.
export async function forget(name) {
    await invoke('forget_path', { path: where.get(name) });
    where.delete(name);
}

/// What the library has, from what was found: the library's, then the games
/// in the folders added, named as if they'd been dropped on the page, a
/// game's disks together. Of two of a name, the first.
let found = null;

function take({ root, library, folders }) {
    found = { root, library, folders };
    where.clear();
    for (const { path } of library) {
        where.set(slashes(path).slice(slashes(root).length + 1), path);
    }
    const games = folders.map(({ path }) => path).filter((path) => GAMES.includes(extension(path)));
    const stored = storedNames(games.map(slashes), (path) => DISKS.includes(extension(path)));
    for (const path of games) {
        const name = stored.get(slashes(path));
        if (name && !where.has(name)) {
            where.set(name, path);
        }
    }
}

let started = false;

/// The names of all there is: at first as they were last found, at once.
/// After that it's `scan` that changes them.
export async function names() {
    if (!started) {
        started = true;
        const known = await invoke('known_files');
        if (known) {
            take(known);
        }
    }
    return [...where.keys()];
}

/// The folders looked through again: how many games came, and went.
export async function scan() {
    const games = () => [...where.keys()].filter((name) => GAMES.includes(extension(name)));
    const before = games();
    take(await invoke('all_files'));
    const now = games();
    return {
        came: now.filter((name) => !before.includes(name)).length,
        went: before.filter((name) => !now.includes(name)).length,
    };
}

export const bytes = async (name) => asBytes(await invoke('file_bytes', { path: where.get(name) }));

/// A folder chosen: its path. Its files and sizes. A file of one left out.
export const pickFolder = () => invoke('pick_folder');
export const folderFiles = (folder) => invoke('folder_files', { folder });
export const hide = (path) => invoke('forget_path', { path });

/// The folders, what's hidden, and where the app keeps its own.
export const settings = () => invoke('library_settings');
/// A folder no longer looked in, and its games gone from the library.
export async function removeFolder(path) {
    await invoke('remove_folder', { path });
    const inside = `${slashes(path).replace(/\/$/, '')}/`;
    change('folders', [], (file) => slashes(file).startsWith(inside));
}
/// What's known changed by what's just been done, not by looking through
/// every folder again: files come into `section`, the library's or the
/// folders'; or those `gone`.
function change(section, came, gone = () => false) {
    if (found) {
        take({ ...found, [section]: [...found[section].filter(({ path }) => !gone(path)), ...came] });
    }
}

/// Whether the file `name` is the app's own copy, in its library, which
/// forgetting deletes; not a folder's, which forgetting only hides.
export const isOwn = (name) => Boolean(found) && slashes(where.get(name) ?? '').startsWith(`${slashes(found.root)}/`);

/// `question` asked, and whether the answer's yes: by the system's own dialog,
/// as a webview's `confirm` doesn't wait for one.
export const ask = (question) => window.__TAURI__.dialog.ask(question, { title: 'Open Bus', kind: 'warning' });

/// `told` when games have come or gone from the folders, as a watcher saw,
/// of how many: what's been found, as it is now, taken.
export const whenFilesChange = (told) =>
    listen('files-changed', async () => {
        const games = () => [...where.keys()].filter((name) => GAMES.includes(extension(name)));
        const before = games();
        const known = await invoke('known_files');
        if (known) {
            take(known);
        }
        const now = games();
        told({
            came: now.filter((name) => !before.includes(name)).length,
            went: before.filter((name) => !now.includes(name)).length,
        });
    });

/// Hidden games, back in the library at once.
export async function restore(paths) {
    change('folders', await invoke('restore_hidden', { paths }));
}

/// A folder's files, as `folderFiles` found them, with the rest.
export async function addFolderFiles(entries) {
    change('folders', await invoke('add_folder_files', { entries }));
}

/// How many games there are in `folder`.
export const gamesIn = (folder) =>
    [...where.values()].filter((path) => slashes(path).startsWith(`${slashes(folder)}/`)).length;

/// A file at `path` as the page reads one, not read until it's wanted.
export function laterFile(path) {
    const read = async () => asBytes(await invoke('file_bytes', { path }));
    return {
        name: baseName(path),
        read,
        text: async () => new TextDecoder().decode(await read()),
        load: async () => new Blob([await read()]),
    };
}

/// A file at `path`, read.
export const fileAt = async (path) => new File([await asBytes(await invoke('file_bytes', { path }))], baseName(path));

/// `told` of paths dropped on the window; the folders among them remembered.
export const whenDropped = (told) =>
    listen('tauri://drag-drop', async ({ payload }) => {
        told(await invoke('take_dropped', { paths: payload.paths }));
    });

export async function keepArt(art) {
    for (const [key, { cover, ...known }] of art) {
        const picture = cover ? new Uint8Array(await cover.arrayBuffer()) : new Uint8Array();
        const headers = { key: encodeURIComponent(key), known: encodeURIComponent(JSON.stringify(known)) };
        await invoke('keep_art', picture, { headers });
    }
}

/// A machine started on the native emulator, the app's window going as its
/// comes: `game`, for the NES; for the Apple, `model`, `roms`, `disks` and
/// `mockingboards`.
export const play = (machine) => invoke('play', { machine: pathed(machine) });

/// A machine as the page has one, by names, as the app has it, by paths.
const pathed = (machine) => ({
    ...machine,
    game: where.get(machine.game),
    roms: where.get(machine.roms),
    disks: machine.disks?.map((disk) => where.get(disk)),
});

/// The games the app keeps of, favourites and those played, by the page's
/// names: those whose files aren't in the library now left out.
export async function keptGames() {
    const names = new Map([...where].map(([name, path]) => [path, name]));
    return (await invoke('kept_games')).flatMap(({ machine, ...kept }) => {
        if (['nes', 'atari2600', 'atari7800'].includes(machine?.kind)) {
            const game = names.get(machine.game);
            return game ? [{ ...kept, machine: { kind: machine.kind, game } }] : [];
        }
        const disks = machine?.kind === 'apple' ? machine.disks.map((disk) => names.get(disk)) : [];
        if (!disks.length || !disks.every(Boolean)) {
            return [];
        }
        return [{ ...kept, machine: { ...machine, disks, roms: names.get(machine.roms) } }];
    });
}

/// What's known of the NES games, by the page's names: each's mapper,
/// whether it's a Famicom's and a Zapper's, and why it won't load, if it
/// won't. Those not looked at yet, not there.
export async function gameFacts() {
    const names = new Map([...where].map(([name, path]) => [path, name]));
    const facts = await invoke('game_facts');
    return new Map(facts.flatMap(({ path, ...known }) => (names.has(path) ? [[names.get(path), known]] : [])));
}

/// `told` when more's known of the games.
export const whenFactsChange = (told) => listen('facts-changed', () => told());

/// What's chosen of `machine`'s game, by the page's names, for itself: its
/// own settings, an object of those it has. And those chosen of it.
export const gameSettings = (machine) => invoke('game_settings', { machine: pathed(machine) });
export const chooseSettings = (machine, settings) => invoke('choose_settings', { machine: pathed(machine), settings });

/// `machine`, by the page's names, a favourite, `on`, or not.
export const favourite = (machine, on) => invoke('favourite', { machine: pathed(machine), on });

/// The machines' ROMs among the files at `paths`, put into the library by
/// the emulator: `held`, those with any, `roms`, and the II+'s `chips`.
export async function sortRoms(paths) {
    const sorted = await invoke('sort_roms', { paths });
    change('library', sorted.kept);
    return sorted;
}

/// `told` of what the gamepads hold for moving around the library, by name,
/// each time that changes; and whose the first player's pad is, `maker`:
/// xbox, playstation or nintendo.
export const whenPads = (told) =>
    listen('pads', ({ payload }) => {
        const words = payload.split(' ').filter(Boolean);
        const maker = words.find((word) => word.startsWith('maker:'))?.slice('maker:'.length) ?? 'xbox';
        told(Object.fromEntries(words.filter((word) => !word.startsWith('maker:')).map((name) => [name, true])), maker);
    });

/// The window fullscreen, or back as it was.
export const fullscreen = (on) => invoke('fullscreen', { on });

/// The app closed.
export const quit = () => invoke('quit');

/// `told` when a game's over, of what went wrong, if anything did.
export const whenGameEnds = (told) => listen('game-ended', ({ payload }) => told(payload));

/// All the art: each file a length, the entry's text, and the cover.
export async function allArt() {
    const all = await invoke('all_art');
    const view = new DataView(all);
    const art = new Map();
    for (let at = 0; at < all.byteLength; ) {
        const fileLength = view.getUint32(at, true);
        const textLength = view.getUint32(at + 4, true);
        const text = new TextDecoder().decode(new Uint8Array(all, at + 8, textLength));
        const { key, known } = JSON.parse(text);
        const coverStart = at + 8 + textLength;
        const coverEnd = at + 4 + fileLength;
        art.set(
            key,
            coverEnd > coverStart
                ? { ...known, cover: new Blob([all.slice(coverStart, coverEnd)], { type: 'image/jpeg' }) }
                : known,
        );
        at = coverEnd;
    }
    return art;
}
