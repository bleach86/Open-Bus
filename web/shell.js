// The page's own files, all it needs to open: the page's loader (index.html)
// asks after each as it opens, and the service worker (sw.js) keeps them for
// it to open offline. The emulator's snippets, named by wasm-bindgen as it
// builds, are found from pkg/cpu_emu.js's imports, not named here.
self.SHELL = [
    'style.css',
    'app.js',
    'art.js',
    'controls.js',
    'covers.js',
    'desktop.js',
    'details.js',
    'versions.js',
    'disks.js',
    'files.js',
    'kept.js',
    'keys.js',
    'scenes.js',
    'sound.js',
    'touch.js',
    'pkg/cpu_emu.js',
    'pkg/cpu_emu_bg.wasm',
    'fonts/Orbitron.ttf',
    'images/x16.png',
    'images/busterm.png',
    'manifest.webmanifest',
    'icons/icon-192.png',
    'icons/icon-512.png',
    'icons/maskable-512.png',
    'icons/apple-touch-icon.png',
    'icons/favicon-32.png',
];
