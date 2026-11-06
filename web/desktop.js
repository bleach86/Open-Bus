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
const GAMES = ['nes', 'fds', 'zip', ...DISKS];

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
function take({ root, library, folders }) {
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
export const removeFolder = (path) => invoke('remove_folder', { path });
export const restore = (path) => invoke('restore_hidden', { path });

/// How many games there are in `folder`.
export const gamesIn = (folder) => [...where.values()].filter((path) => slashes(path).startsWith(`${slashes(folder)}/`)).length;

/// `told` of what's chosen on the app's menu.
export const whenMenu = (told) => listen('menu', ({ payload }) => told(payload));

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
export const whenDropped = (told) => listen('tauri://drag-drop', async ({ payload }) => {
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
export const play = (machine) => invoke('play', {
    machine: {
        ...machine,
        game: where.get(machine.game),
        roms: where.get(machine.roms),
        disks: machine.disks?.map((disk) => where.get(disk)),
    },
});

/// `told` when a game's over, of what went wrong, if anything did.
export const whenGameEnds = (told) => listen('game-ended', ({ payload }) => told(payload));

/// All the art: each file a length, the entry's text, and the cover.
export async function allArt() {
    const all = await invoke('all_art');
    const view = new DataView(all);
    const art = new Map();
    for (let at = 0; at < all.byteLength;) {
        const fileLength = view.getUint32(at, true);
        const textLength = view.getUint32(at + 4, true);
        const text = new TextDecoder().decode(new Uint8Array(all, at + 8, textLength));
        const { key, known } = JSON.parse(text);
        const coverStart = at + 8 + textLength;
        const coverEnd = at + 4 + fileLength;
        art.set(key, coverEnd > coverStart ? { ...known, cover: new Blob([all.slice(coverStart, coverEnd)], { type: 'image/jpeg' }) } : known);
        at = coverEnd;
    }
    return art;
}
