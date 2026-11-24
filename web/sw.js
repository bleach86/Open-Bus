// The web page's service worker: the page and its files kept, for it to open
// with no network, and for a browser to offer to install it as an app. Each
// is still asked after first, as the page's loader asks (a page of old files
// and new doesn't work): what's kept is only for when there's no answer.
// Only the page's own files: covers and the like, from elsewhere, are the
// browser's to keep as it does.
importScripts('shell.js');

const KEPT = 'open-bus';

/** Asked after afresh: the answer from the browser's cache only if it's unchanged. */
const ask = (url) => fetch(url, { cache: 'no-cache' });

self.addEventListener('install', (event) => {
    event.waitUntil(
        (async () => {
            const kept = await caches.open(KEPT);
            await kept.addAll(['./', 'shell.js', ...self.SHELL].map((url) => new Request(url, { cache: 'no-cache' })));
            // The emulator's snippets, which its glue imports by names that
            // change as it's built.
            const glue = await (await ask('pkg/cpu_emu.js')).text();
            const snippets = [...glue.matchAll(/from "\.\/(snippets\/[^"]+)"/g)].map(([, path]) => `pkg/${path}`);
            await kept.addAll(snippets.map((url) => new Request(url, { cache: 'no-cache' })));
            // The last builds' snippets, under names no longer asked for, gone.
            for (const request of await kept.keys()) {
                const path = new URL(request.url).pathname;
                if (path.includes('/pkg/snippets/') && !snippets.some((snippet) => path.endsWith(`/${snippet}`))) {
                    await kept.delete(request);
                }
            }
            await self.skipWaiting();
        })(),
    );
});

// The page that's open taken over at once, its first visit's files kept too.
// What's kept is under one name, each file replaced as it's fetched again.
self.addEventListener('activate', (event) => {
    event.waitUntil(self.clients.claim());
});

self.addEventListener('fetch', (event) => {
    const { request } = event;
    if (request.method !== 'GET' || new URL(request.url).origin !== self.location.origin) {
        return;
    }
    event.respondWith(fresh(event, request));
});

/**
 * `request` asked after, its answer kept; with no answer, as it was last
 * kept. The page itself, opened at any address in its scope, is the one
 * kept at its start.
 */
async function fresh(event, request) {
    const kept = await caches.open(KEPT);
    const navigating = request.mode === 'navigate';
    try {
        // A page's own request can't be sent again as it is: its address, then.
        const answer = await ask(navigating ? request.url : request);
        if (answer.ok && answer.status === 200) {
            event.waitUntil(kept.put(request, answer.clone()));
        }
        return answer;
    } catch (offline) {
        const was = await kept.match(request, { ignoreSearch: navigating });
        return was ?? (navigating && (await kept.match('./'))) ?? Response.error();
    }
}
