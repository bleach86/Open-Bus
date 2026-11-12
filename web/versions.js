// A game's versions, one tile on the shelf for all of them: its regions,
// revisions and dumps, as their files' names tell them apart. A name's
// brackets are its tags: those of regions, languages, revisions and how it
// was dumped say which version it is; any other, as "(Namco)" in Namco's
// Star Wars, not JVC's, says which game. And of the versions, the one shown
// first: one that loads; a good dump over an alternate, and far over a bad
// one, a hack or a pirate's; then the USA's, the world's, Europe's, Japan's.

import { plainTitle, titleWords } from './art.js';

/// A tag that's a version's, each of its parts, as "USA, Europe": a region,
/// GoodNES's letters for them too; a language; a revision, a PRG, a beta or
/// a prototype. Not "Unl": an unlicensed game of a title is another game,
/// as Tengen's Tetris is, or Wisdom Tree's King of Kings, not Namco's.
const REGION =
    /^(usa|japan|europe|world|asia|australia|brazil|canada|china|france|germany|hong kong|italy|korea|netherlands|russia|spain|sweden|taiwan|uk|[ujeawkfgsc]|ju|ue|jue|ch|hk)$/i;
const VERSION =
    /^(rev\b.*|v\d.*|prg ?\d|beta.*|proto.*|sample|demo|alt.*|en|fr|de|es|it|nl|pt|sv|no|da|fi|ja|ko|zh|ru|pl)$/i;
const versionTag = (tag) => tag.split(',').every((part) => REGION.test(part.trim()) || VERSION.test(part.trim()));

const fileName = (name) => name.split('/').pop();
const parenthesized = (name) => [...fileName(name).matchAll(/\(([^)]*)\)/g)].map((tag) => tag[1].trim());
const bracketed = (name) => [...fileName(name).matchAll(/\[([^\]]*)\]/g)].map((tag) => tag[1].trim());

/// Which game a file's a version of: its title's words, and its tags that
/// aren't a version's.
export function versionKey(name) {
    const own = parenthesized(name)
        .filter((tag) => !versionTag(tag))
        .map((tag) => tag.toLowerCase());
    return [titleWords(plainTitle(name)), ...own].join('|');
}

/// GoodNES's square brackets, each how much worse than a good dump it is:
/// [b] bad, [h] a hack, [p] a pirate's, [o] overdumped, [t] a trainer, [f]
/// fixed, [T] translated, [a] an alternate; [!] verified, a little better.
const DUMPS = [
    [/^b\d*$/, 100],
    [/^[hp]/, 60],
    [/^o\d*$/, 40],
    [/^t\d*$/, 40],
    [/^T[+-]/, 30],
    [/^f\d*$/, 20],
    [/^a\d*$/, 5],
    [/^!$/, -1],
];
/// The regions, each worse than the one before.
const REGIONS = [/\b(usa|u|world|w|ju|ue|jue)\b/i, /\b(europe|e|uk)\b/i, /\b(japan|j)\b/i];

/// How far from the best version a file's name says it is: 0, the best.
export function versionRank(name) {
    const dump = bracketed(name).reduce(
        (worse, tag) => worse + (DUMPS.find(([pattern]) => pattern.test(tag))?.[1] ?? 0),
        0,
    );
    const tags = parenthesized(name).join(', ');
    const early = /\b(beta|proto|sample|demo)/i.test(tags) ? 40 : 0;
    const region = REGIONS.findIndex((pattern) => pattern.test(tags));
    return dump + early + 10 * (region < 0 ? REGIONS.length : region);
}

/// What tells a version from the others, to choose it by: its tags, or its
/// name, if it has none.
export const versionName = (name) =>
    [...parenthesized(name), ...bracketed(name)].join(', ') || fileName(name).replace(/\.[^.]*$/, '');
