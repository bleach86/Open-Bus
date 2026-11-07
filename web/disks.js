// Disks as games. A game of several disks is known by the folder it came in,
// kept in the name it's stored under, "Ultima IV/side A.dsk", which keeps two
// games' "disk 1.dsk" apart too; or added without one, by its disks' names
// saying what they are, "Ultima IV side A", "Ultima IV side B". A hard disk's
// volume is a game of its own, wherever it's from.

/// Names in the order a person would put them: disk 2 before disk 10.
export const natural = (a, b) => a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' });

export const isVolume = (name) => /\.(hdv|2mg)$/i.test(name);
const base = (path) => path.split('/').pop();
const stem = (name) =>
    base(name)
        .replace(/\.[^.]*$/, '')
        .toLowerCase();

/// A folder's name as a title: what's in brackets is where it's from.
const title = (name) =>
    name
        .replace(/\s*\([^)]*\)/g, '')
        .replace(/_/g, ' ')
        .trim();

/// What a disk's name says is its game, if it says: all before its side or
/// disk, or a last part that's a number and a side.
function namedGame(name) {
    const name_ = stem(name);
    const original = base(name).replace(/\.[^.]*$/, '');
    const said =
        /[\s_.-]*(\b(side|disk|disc|vol|volume|part)\s*\d*[a-d]?\b|\s-\s.*\bdisk$|[\s_]\d+[ab]$|[\s_]d\d(_s\d)?$|_d\d$).*$/i.exec(
            original,
        );
    if (!said || said.index === 0) {
        return null;
    }
    return { key: name_.slice(0, said.index), title: title(original.slice(0, said.index)) };
}

/// `names`, disks by the name each's stored under, as games: each with its
/// title and its disks in order. Those of one disk have only that.
export function games(names) {
    const groups = new Map();
    const alone = [];
    for (const name of names) {
        const folder = name.includes('/') ? name.split('/')[0] : null;
        const game = folder ? { key: `/${folder}`, title: title(folder) } : namedGame(name);
        if (!game || isVolume(name)) {
            alone.push(name);
        } else if (groups.has(game.key)) {
            groups.get(game.key).disks.push(name);
        } else {
            groups.set(game.key, { title: game.title, disks: [name] });
        }
    }
    const single = (name) => ({ title: base(name), disks: [name] });
    const all = [...alone.map(single)];
    for (const group of groups.values()) {
        all.push(...(group.disks.length > 1 ? [group] : group.disks.map(single)));
    }
    for (const game of all) {
        game.disks.sort(natural);
    }
    return all.sort((a, b) => natural(a.title, b.title));
}

export const shortName = base;

/// How files from dropped folders are to be stored. `paths`, as the browser
/// gives them, "/Roms/Ultima IV/side A.dsk", and `isDisk` to know a disk:
/// a Map of each path to its name, those in a folder of a game with it, or
/// null for a copy of another's in a folder under a game's, as "old" is.
export function storedNames(paths, isDisk) {
    const dirOf = (path) => path.slice(0, path.lastIndexOf('/'));
    const disks = new Map();
    for (const path of paths.filter(isDisk)) {
        const dir = dirOf(path);
        disks.set(dir, [...(disks.get(dir) ?? []), path]);
    }
    const within = (dir) => [...disks.keys()].filter((other) => other.startsWith(`${dir}/`));
    const stems = (dir) => new Set((disks.get(dir) ?? []).map(stem));
    // A folder under one with the same disks is only copies of them.
    const copyOf = (dir) =>
        [...disks.keys()].find(
            (other) => dir.startsWith(`${other}/`) && disks.get(dir).some((path) => stems(other).has(stem(path))),
        );
    const isGame = (dir) =>
        (disks.get(dir) ?? []).filter((path) => !isVolume(path)).length >= 2 &&
        !copyOf(dir) &&
        within(dir).every(copyOf);
    return new Map(
        paths.map((path) => {
            const dir = dirOf(path);
            if (disks.has(dir) && copyOf(dir)) {
                return [path, null];
            }
            return [path, isGame(dir) ? `${base(dir)}/${base(path)}` : base(path)];
        }),
    );
}
