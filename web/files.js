// The files kept in this browser, in IndexedDB: name to bytes, and the parts
// of them written over since, by name and where they start. A file's parts
// go on top of it when it's read, and go when it's replaced. And the games'
// art, by key: what art.js keeps. In the desktop app, all of it's on the
// disk instead: see desktop.js.
const desktop = window.__TAURI__ ? await import('./desktop.js') : null;

const open = () => new Promise((resolve, reject) => {
    const request = indexedDB.open('cpu_emu', 3);
    request.onupgradeneeded = () => {
        const stores = request.result.objectStoreNames;
        if (!stores.contains('files')) {
            request.result.createObjectStore('files');
        }
        if (!stores.contains('parts')) {
            request.result.createObjectStore('parts');
        }
        if (!stores.contains('art')) {
            request.result.createObjectStore('art');
        }
    };
    request.onsuccess = () => {
        // Another tab, newer, wanting to upgrade it.
        request.result.onversionchange = () => request.result.close();
        resolve(request.result);
    };
    request.onerror = () => reject(request.error);
});

const database = desktop ? null : open();

async function store(mode, use) {
    const db = await database;
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(['files', 'parts', 'art'], mode);
        const asked = use(transaction.objectStore('files'), transaction.objectStore('parts'), transaction.objectStore('art'));
        transaction.oncomplete = () => resolve(asked);
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error);
    });
}

/// All of a file's parts: its name then a number, up to its name then
/// anything that isn't, which sorts after every number.
const partsOf = (name) => IDBKeyRange.bound([name], [name, []]);

const keepHere = (name, bytes) => store('readwrite', (files, parts) => {
    parts.delete(partsOf(name));
    files.put(bytes, name);
});

/// `changed`, [where it starts, its bytes] each, kept all together.
const keepPartsHere = (name, changed) => store('readwrite', (files, parts) => {
    for (const [offset, bytes] of changed) {
        parts.put(bytes, [name, offset]);
    }
});

const forgetHere = (name) => store('readwrite', (files, parts) => {
    parts.delete(partsOf(name));
    files.delete(name);
});

const keepArtHere = (art) => store('readwrite', (files, parts, kept) => {
    for (const [key, value] of art) {
        kept.put(value, key);
    }
});

async function allArtHere() {
    const [keys, values] = await store('readonly', (files, parts, kept) => [kept.getAllKeys(), kept.getAll()]);
    return new Map(keys.result.map((key, at) => [key, values.result[at]]));
}

const namesHere = async () => (await store('readonly', (files) => files.getAllKeys())).result;

async function bytesHere(name) {
    const [file, keys, parts] = await store('readonly', (files, kept) =>
        [files.get(name), kept.getAllKeys(partsOf(name)), kept.getAll(partsOf(name))]);
    const offsets = keys.result.map(([, offset]) => offset);
    const size = Math.max(file.result?.length ?? 0, ...offsets.map((offset, at) => offset + parts.result[at].length));
    const whole = new Uint8Array(size);
    whole.set(file.result ?? []);
    offsets.forEach((offset, at) => whole.set(parts.result[at], offset));
    return whole;
}

/// The desktop app's own, if this is it: playing a game, for one.
export const native = desktop;
export const keep = desktop?.keep ?? keepHere;
export const keepParts = desktop?.keepParts ?? keepPartsHere;
export const forget = desktop?.forget ?? forgetHere;
export const keepArt = desktop?.keepArt ?? keepArtHere;
export const allArt = desktop?.allArt ?? allArtHere;
export const names = desktop?.names ?? namesHere;
export const bytes = desktop?.bytes ?? bytesHere;
