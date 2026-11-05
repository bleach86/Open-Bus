// The files kept in this browser, in IndexedDB: name to bytes.
const open = () => new Promise((resolve, reject) => {
    const request = indexedDB.open('cpu_emu', 1);
    request.onupgradeneeded = () => request.result.createObjectStore('files');
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
});

const database = open();

async function store(mode, use) {
    const db = await database;
    return new Promise((resolve, reject) => {
        const transaction = db.transaction('files', mode);
        const request = use(transaction.objectStore('files'));
        transaction.oncomplete = () => resolve(request.result);
        transaction.onerror = () => reject(transaction.error);
    });
}

export const keep = (name, bytes) => store('readwrite', (files) => files.put(bytes, name));
export const forget = (name) => store('readwrite', (files) => files.delete(name));
export const names = () => store('readonly', (files) => files.getAllKeys());
export const bytes = (name) => store('readonly', (files) => files.get(name));
