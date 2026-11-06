// The files kept on the disk, in the desktop app, where the page's own
// storage would be in a browser: what files.js does, by the app's commands.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const asBytes = (data) => new Uint8Array(data);

export const keep = (name, bytes) => invoke('keep_file', bytes, { headers: { name: encodeURIComponent(name) } });

/// The running machines' parts of a hard disk: they aren't here, a game
/// being the native emulator's, which writes its own.
export const keepParts = async () => {};

export const forget = (name) => invoke('forget_file', { name });

export const names = () => invoke('file_names');

export const bytes = async (name) => asBytes(await invoke('file_bytes', { name }));

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
export const play = (machine) => invoke('play', { machine });

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
