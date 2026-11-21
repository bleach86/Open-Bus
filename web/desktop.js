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
const GAMES = ['nes', 'fds', 'a26', 'a78', 'bin', 'zip', 'prg', ...DISKS];

/// Where each name's file is: the page has names, the disk paths.
const where = new Map();

/// A file kept in the library, and in what's been found, as it's named
/// there already: else what's found next, as a ROM's sorted, would have
/// only what was there before.
export async function keep(name, bytes) {
    const path = await invoke('keep_file', sendable(bytes), { headers: { name: encodeURIComponent(name) } });
    where.set(name, path);
    unfound(path);
    found?.library.push({ path, size: bytes.length });
}

/// What's been found, without the file at `path`.
function unfound(path) {
    if (found) {
        found.library = found.library.filter((file) => file.path !== path);
        found.folders = found.folders.filter((file) => file.path !== path);
    }
}

/// The running machines' parts of a hard disk: they aren't here, a game
/// being the native emulator's, which writes its own.
export const keepParts = async () => {};

/// Gone from the library: its own copy deleted, or one in a folder of yours
/// only hidden.
export async function forget(name) {
    const path = where.get(name);
    await invoke('forget_path', { path });
    where.delete(name);
    unfound(path);
}

/// What the library has, from what was found: the library's, then the games
/// in the folders added, named as if they'd been dropped on the page, a
/// game's disks together. Of two or more of a name, as another machine's
/// game of a name is, each by as much of the path to it as tells them
/// apart: "atari7800/Joust (USA).zip", "atari2600/Joust (USA).zip".
let found = null;

function take({ root, library, folders }) {
    found = { root, library, folders };
    where.clear();
    for (const { path } of library) {
        where.set(slashes(path).slice(slashes(root).length + 1), path);
    }
    const games = [...new Set(folders.map(({ path }) => path).filter((path) => GAMES.includes(extension(path))))];
    const stored = storedNames(games.map(slashes), (path) => DISKS.includes(extension(path)));
    const named = new Map();
    for (const path of games) {
        const name = stored.get(slashes(path));
        if (name) {
            named.set(name, [...(named.get(name) ?? []), path]);
        }
    }
    for (const [name, paths] of named) {
        const names = paths.length > 1 || where.has(name) ? apart(name, paths) : [name];
        paths.forEach((path, at) => where.set(names[at], path));
    }
}

/// `paths`, all of whose games are named `name`, each named by the end of
/// its path: as few of its folders as tells them apart, and from the
/// library's.
function apart(name, paths) {
    const parts = paths.map((path) => slashes(path).split('/'));
    for (let length = name.split('/').length + 1; ; length++) {
        const names = parts.map((part) => part.slice(-length).join('/'));
        const all = length >= Math.max(...parts.map((part) => part.length));
        if (all || (new Set(names).size === names.length && !names.some((name) => where.has(name)))) {
            return names;
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

/// The saves: where they're kept, whether that's been chosen, and of each
/// kind (save, state, resume, card, disk) how many and how big. Each kind's
/// cleared; and where they're kept changed (`null`, back to the default),
/// what's there moved. Each `{ folder, chosen, kinds }` after.
export const savesOverview = () => invoke('saves_overview');
export const clearSaves = (kinds) => invoke('clear_saves', { kinds });
export const setSavesFolder = (path) => invoke('set_saves_folder', { path });

/// A game's saves, each `{ kind, bytes, written, path }`; and those of
/// `kinds` deleted, what's left after.
export const gameSaves = (machine) => invoke('game_saves', { machine: pathed(machine) });
export const clearGameSaves = (machine, kinds) => invoke('clear_game_saves', { machine: pathed(machine), kinds });
export const folderFiles = (folder) => invoke('folder_files', { folder });
export const hide = (path) => invoke('forget_path', { path });

/// The folders, what's hidden, and where the app keeps its own.
export const settings = () => invoke('library_settings');
/// The app's version, as its Cargo.toml has it.
export const appVersion = () => window.__TAURI__.app.getVersion();
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
        await invoke('keep_art', sendable(picture), { headers });
    }
}

/// Bytes to send the app: as they are; on Android, whose webview sends only
/// JSON, as base64, a third bigger, not a number a byte.
function sendable(bytes) {
    if (!android) {
        return bytes;
    }
    const CHUNK = 0x8000;
    let text = '';
    for (let at = 0; at < bytes.length; at += CHUNK) {
        text += String.fromCharCode(...bytes.subarray(at, at + CHUNK));
    }
    return btoa(text);
}

/// A machine started on the native emulator, the app's window going as its
/// comes: `game`, for the NES; for the Apple, `model`, `roms`, `disks` and
/// `mockingboards`.
export const play = (machine) => invoke('play', { machine: pathed(machine) });

/// The menus' choosing and going back, as the emulator has them bound:
/// `{ accept, back }`, by its names for the buttons.
export const menuButtons = () => invoke('menu_buttons');

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
        if (['nes', 'atari2600', 'atari7800', 'x16'].includes(machine?.kind)) {
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

/// The X16's ROM, its release downloaded and sorted into the library, as
/// `sortRoms` has it.
export async function fetchX16Rom() {
    const sorted = await invoke('fetch_x16_rom');
    change('library', sorted.kept);
    return sorted;
}

/// Whether it's the app on Android: with no folder picker fit for a TV, the
/// page's own list of folders; and the gamepads the webview's.
export const android = /Android/.test(navigator.userAgent);

/// Whether the app tells the page what its gamepads hold: not on Android,
/// where the webview has them all, as a desktop's doesn't.
export const tellsPads = !android;

/// The folders in `path`, by name, and the folder above: `{ folders: [{ name,
/// path }], up }`; with no `path`, where folders are, to start.
export const foldersIn = (path) => invoke('folders_in', { path });

/// Whether the app may read folders, on Android; and that asked for, by its
/// settings opening.
export const storageAccess = () => invoke('storage_access');
export const askStorageAccess = () => invoke('ask_storage_access');

/// A folder chosen in the page's own list, remembered: its path.
export const addFolder = (path) => invoke('add_folder', { path });

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
