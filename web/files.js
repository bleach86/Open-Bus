// The files kept in this browser, in IndexedDB: name to bytes, and the parts
// of them written over since, by name and where they start. A file's parts
// go on top of it when it's read, and go when it's replaced.
const open = () => new Promise((resolve, reject) => {
    const request = indexedDB.open('cpu_emu', 2);
    request.onupgradeneeded = () => {
        const stores = request.result.objectStoreNames;
        if (!stores.contains('files')) {
            request.result.createObjectStore('files');
        }
        if (!stores.contains('parts')) {
            request.result.createObjectStore('parts');
        }
    };
    request.onsuccess = () => {
        // Another tab, newer, wanting to upgrade it.
        request.result.onversionchange = () => request.result.close();
        resolve(request.result);
    };
    request.onerror = () => reject(request.error);
});

const database = open();

async function store(mode, use) {
    const db = await database;
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(['files', 'parts'], mode);
        const asked = use(transaction.objectStore('files'), transaction.objectStore('parts'));
        transaction.oncomplete = () => resolve(asked);
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error);
    });
}

/// All of a file's parts: its name then a number, up to its name then
/// anything that isn't, which sorts after every number.
const partsOf = (name) => IDBKeyRange.bound([name], [name, []]);

export const keep = (name, bytes) => store('readwrite', (files, parts) => {
    parts.delete(partsOf(name));
    files.put(bytes, name);
});

/// `changed`, [where it starts, its bytes] each, kept all together.
export const keepParts = (name, changed) => store('readwrite', (files, parts) => {
    for (const [offset, bytes] of changed) {
        parts.put(bytes, [name, offset]);
    }
});

export const forget = (name) => store('readwrite', (files, parts) => {
    parts.delete(partsOf(name));
    files.delete(name);
});

export const names = async () => (await store('readonly', (files) => files.getAllKeys())).result;

export async function bytes(name) {
    const [file, keys, parts] = await store('readonly', (files, kept) =>
        [files.get(name), kept.getAllKeys(partsOf(name)), kept.getAll(partsOf(name))]);
    const offsets = keys.result.map(([, offset]) => offset);
    const size = Math.max(file.result?.length ?? 0, ...offsets.map((offset, at) => offset + parts.result[at].length));
    const whole = new Uint8Array(size);
    whole.set(file.result ?? []);
    offsets.forEach((offset, at) => whole.set(parts.result[at], offset));
    return whole;
}
