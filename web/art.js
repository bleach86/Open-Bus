// Covers and what's known of a game: from the art beside a game, as
// EmulationStation keeps it, "images/Game-thumb.png" and a gamelist.xml, or
// any picture named after one. A cover's shrunk as it's taken: a box's 500K
// is 15K at the size it's shown, and a library of them fits in a browser.
// Nothing's fetched: only what's added.

import { keepArt } from './files.js';

/// How wide a cover's kept.
const WIDTH = 300;
/// How many are made between keeping them.
const KEPT_AT_ONCE = 40;

const isImage = (name) => /\.(png|jpe?g|webp)$/i.test(name);
const isGamelist = (name) => /^gamelist\.xml$/i.test(name);

/// What goes with the games, not being one: `isMedia`, of what's added.
export const isMedia = (name) => isImage(name) || isGamelist(name) || /\.(mp4|mkv|webm)$/i.test(name);

export const keyOf = (stem) => stem.trim().toLowerCase();

/// What a game's art is kept under: its file's name, without the ending.
export const artKey = (name) => keyOf(name.split('/').pop().replace(/\.[^.]*$/, ''));

/// A game's name from its file, its brackets gone.
export const plainTitle = (name) => name.split('/').pop().replace(/\.[^.]*$/, '').replace(/\s*[([][^)\]]*[)\]]/g, '')
    .replace(/_ /g, ', ').replace(/[_\s]+/g, ' ').trim();

/// A title as nothing but its letters and digits, without its "the" and "a"
/// and whatever's in brackets, for what's named a little differently: a
/// download's "Zelda_ The" for "Zelda, The", or "The Zelda".
export const titleWords = (title) => title.replace(/\([^)]*\)|\[[^\]]*\]/g, '').toLowerCase().replace(/['’]/g, '')
    .replace(/[^a-z0-9]+/g, ' ').replace(/\b(the|a)\b/g, '').replace(/ /g, '');

/// What's in a name's brackets, as what tells two of a title apart.
export const tagsOf = (name) => [...name.matchAll(/[([]([^)\]]*)[)\]]/g)].map((tag) => tag[1]).join(', ');

/// The game a picture's of, and how good a picture of it: a box, then a
/// screen; marquees and the like not at all.
function pictureOf(name) {
    const stem = name.replace(/\.[^.]*$/, '');
    const suffix = /-(thumb|image|marquee|video|screenshot|fanart|wheel|boxart|cover)$/i.exec(stem);
    if (!suffix) {
        return { key: keyOf(stem), rank: 1 };
    }
    const rank = { thumb: 0, boxart: 0, cover: 0, image: 2 }[suffix[1].toLowerCase()];
    return rank === undefined ? null : { key: keyOf(stem.slice(0, suffix.index)), rank };
}

const text = (game, tag) => game.querySelector(`:scope > ${tag}`)?.textContent.trim() || undefined;

/// What gamelist.xml's `xml` says, by game.
function parseGamelist(xml) {
    const known = new Map();
    for (const game of new DOMParser().parseFromString(xml, 'text/xml').querySelectorAll('game')) {
        const path = text(game, 'path');
        if (path) {
            known.set(artKey(path), {
                title: text(game, 'name'),
                year: text(game, 'releasedate')?.slice(0, 4),
                genre: text(game, 'genre')?.split(/[/,]/)[0].trim(),
                players: text(game, 'players'),
                developer: text(game, 'developer'),
            });
        }
    }
    return known;
}

/// `file` shrunk: a JPEG of `WIDTH` across.
export async function shrink(file) {
    const picture = await createImageBitmap(file);
    const canvas = document.createElement('canvas');
    canvas.width = Math.min(WIDTH, picture.width);
    canvas.height = Math.round(picture.height * canvas.width / picture.width);
    const drawing = canvas.getContext('2d');
    drawing.imageSmoothingQuality = 'high';
    drawing.drawImage(picture, 0, 0, canvas.width, canvas.height);
    picture.close();
    return new Promise((resolve) => canvas.toBlob(resolve, 'image/jpeg', 0.82));
}

/// The art in `files` taken into `art`, the map of what's kept, and kept a
/// few at a time, so that a tab closed half way loses little: how many covers
/// were made, `told` of how far through it is.
export async function takeArt(files, art, told) {
    const known = new Map();
    for (const file of files.filter((file) => isGamelist(file.name))) {
        for (const [key, game] of parseGamelist(await file.text())) {
            known.set(key, { ...art.get(key), ...game });
        }
    }
    for (const [key, game] of known) {
        art.set(key, game);
    }
    await keepArt(known);
    // The best picture of each that hasn't one.
    const best = new Map();
    for (const file of files.filter((file) => isImage(file.name))) {
        const picture = pictureOf(file.name);
        if (picture && !art.get(picture.key)?.cover && (best.get(picture.key)?.rank ?? 9) > picture.rank) {
            best.set(picture.key, { file, rank: picture.rank });
        }
    }
    let done = 0;
    let made = 0;
    let unkept = new Map();
    for (const [key, { file }] of best) {
        told(++done, best.size);
        try {
            const game = { ...art.get(key), cover: await shrink(file) };
            art.set(key, game);
            unkept.set(key, game);
            made++;
        } catch {
            // Not a picture after all.
        }
        if (unkept.size === KEPT_AT_ONCE) {
            await keepArt(unkept);
            unkept = new Map();
        }
    }
    await keepArt(unkept);
    return made;
}
