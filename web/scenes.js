// The scenes behind the desktop app's library, drawn a pixel at a time in
// the NES's own colours (palette.rs's, by number), about as many lines tall
// as a TV of its showed, for the page to scale up by whole pixels.

const NES = {
    black: '#000000', // 0F
    navy: '#002a84', // 01
    blue: '#1614a2', // 02
    violet: '#3b029f', // 03
    plum: '#5a007b', // 04
    purple: '#9f1eca', // 14
    rose: '#b5217b', // 15
    pink: '#ff70cb', // 25
    orchid: '#ef6cff', // 24
    coral: '#ff8271', // 26
    amber: '#e79e26', // 27
    brown: '#974f00', // 17
    rust: '#b23322', // 16
    cream: '#f5d7a5', // 37
    peach: '#ffccc5', // 36
    white: '#ffffff', // 20
    sky: '#66afff', // 21
    lilac: '#9391ff', // 22
    lavender: '#e7c8ff', // 33
    pale: '#d3d2ff', // 32
    cyan: '#4bcbdc', // 2C
    grey: '#666666', // 00
    silver: '#aeaeae', // 10
    dark: '#4e4e4e', // 2D
};

/// Ordered dithering: whether `x`, `y` takes the second of two colours,
/// `t` of the way from the first.
const BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
const dithered = (x, y, t) => t * 16 > BAYER[y & 3][x & 3];

/// Numbers that look random, the same each time from the same seed.
function seeded(seed) {
    return () => {
        seed = (seed * 1103515245 + 12345) & 0x7fffffff;
        return seed / 0x7fffffff;
    };
}

/// Smooth noise, 0 to 1: a few octaves of a hashed grid.
function noise(seed) {
    const hash = (x, y) => {
        let n = (x * 374761393 + y * 668265263 + seed * 2147483647) | 0;
        n = Math.imul(n ^ (n >>> 13), 1274126177);
        return ((n ^ (n >>> 16)) >>> 0) / 4294967296;
    };
    const smooth = (t) => t * t * (3 - 2 * t);
    const once = (x, y) => {
        const [x0, y0] = [Math.floor(x), Math.floor(y)];
        const [tx, ty] = [smooth(x - x0), smooth(y - y0)];
        const top = hash(x0, y0) * (1 - tx) + hash(x0 + 1, y0) * tx;
        const bottom = hash(x0, y0 + 1) * (1 - tx) + hash(x0 + 1, y0 + 1) * tx;
        return top * (1 - ty) + bottom * ty;
    };
    return (x, y) => once(x, y) * 0.5 + once(x * 2, y * 2) * 0.25 + once(x * 4, y * 4) * 0.15 + once(x * 8, y * 8) * 0.1;
}

/// The canvas's pixels, to draw on and then show.
function pixels(canvas) {
    const context = canvas.getContext('2d');
    const [w, h] = [canvas.width, canvas.height];
    const image = context.createImageData(w, h);
    const data = new Uint32Array(image.data.buffer);
    const values = new Map();
    const value = (colour) => {
        if (!values.has(colour)) {
            const n = parseInt(colour.slice(1), 16);
            values.set(colour, (0xff000000 | ((n & 0xff) << 16) | (n & 0xff00) | ((n >> 16) & 0xff)) >>> 0);
        }
        return values.get(colour);
    };
    const inside = (x, y) => x >= 0 && x < w && y >= 0 && y < h;
    const set = (x, y, colour) => {
        [x, y] = [Math.round(x), Math.round(y)];
        if (inside(x, y)) {
            data[y * w + x] = value(colour);
        }
    };
    return {
        w,
        h,
        set,
        value,
        at: (x, y) => (inside(x, y) ? data[y * w + x] : 0),
        put(x, y, raw) {
            if (inside(x, y)) {
                data[y * w + x] = raw;
            }
        },
        rect(x, y, width, height, colour) {
            for (let yy = Math.round(y); yy < y + height; yy++) {
                for (let xx = Math.round(x); xx < x + width; xx++) {
                    set(xx, yy, colour);
                }
            }
        },
        show: () => context.putImageData(image, 0, 0),
    };
}

/// Bands of `colours` from `top` to `bottom`, as `weights` share it out, each
/// dithered into the next over its last few rows.
function bands(p, top, bottom, colours, weights) {
    const total = weights.reduce((sum, weight) => sum + weight, 0);
    let y = top;
    colours.forEach((colour, at) => {
        const end = at === colours.length - 1 ? bottom : y + Math.round(((bottom - top) * weights[at]) / total);
        const next = colours[at + 1];
        for (let yy = y; yy < end; yy++) {
            const into = next ? Math.max(0, (yy - (end - 4)) / 5) : 0;
            for (let x = 0; x < p.w; x++) {
                p.set(x, yy, into && dithered(x, yy, into) ? next : colour);
            }
        }
        y = end;
    });
}

/// `count` stars between `top` and `bottom`, a few of them bigger.
function stars(p, random, top, bottom, count) {
    for (let n = 0; n < count; n++) {
        const [x, y] = [Math.floor(random() * p.w), Math.floor(top + random() * (bottom - top))];
        p.set(x, y, [NES.white, NES.silver, NES.sky, NES.lilac, NES.pale][Math.floor(random() * 5)]);
        if (random() < 0.08) {
            for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
                p.set(x + dx, y + dy, NES.blue);
            }
        }
    }
}

/// A disc, each pixel the colour `colourAt` gives it, from where it is on
/// the disc, -1 to 1 across and down: none, left as it was.
function disc(p, cx, cy, r, colourAt) {
    for (let y = Math.floor(cy - r); y <= cy + r; y++) {
        for (let x = Math.floor(cx - r); x <= cx + r; x++) {
            const [dx, dy] = [x - cx, y - cy];
            if (dx * dx + dy * dy <= r * r) {
                const colour = colourAt(x, y, dx / r, dy / r);
                if (colour) {
                    p.set(x, y, colour);
                }
            }
        }
    }
}

/// A sun going down behind mountains, and a grid running away to it.
function sunset(p, centre) {
    const { w, h } = p;
    const horizon = Math.round(h * 0.62);
    const row = (y, from, to, colour) => p.rect(from, y, to - from, 1, colour);

    bands(p, 0, horizon, [NES.black, NES.navy, NES.blue, NES.violet, NES.plum, NES.purple, NES.rose, NES.pink], [5, 3, 3, 3, 3, 2, 2, 1]);
    stars(p, seeded(7), 0, horizon * 0.45, (w * h) / 900);

    // Warm at the top, striped at the bottom, the stripes wider as it goes.
    const r = Math.round(h * 0.21);
    const cy = horizon - Math.round(r * 0.45);
    const warm = [NES.cream, NES.amber, NES.coral, NES.pink, NES.orchid];
    const gaps = new Set();
    for (let g = 0, gy = cy - Math.round(r * 0.1), size = 1; gy < horizon; g++) {
        for (let k = 0; k < size; k++) {
            gaps.add(gy + k);
        }
        gy += size + Math.max(2, 6 - g);
        size = Math.min(4, 1 + Math.floor((g + 1) / 2));
    }
    for (let y = cy - r; y < horizon; y++) {
        if (!gaps.has(y)) {
            const half = Math.round(Math.sqrt(Math.max(0, r * r - (y - cy) ** 2)));
            const band = Math.min(warm.length - 1, Math.floor(((y - (cy - r)) / (2 * r)) * warm.length * 1.25));
            row(y, centre - half, centre + half, warm[band]);
        }
    }

    // Two ranges of mountains either side of it, edged in the sky's light.
    const ridge = (seed, rough) => {
        const random = seeded(seed);
        let height = 0.5;
        return Array.from({ length: w }, () => (height = Math.min(1, Math.max(0, height + (random() - 0.5) * rough))));
    };
    const range = (heights, tall, away, edge, fill) => {
        for (let x = 0; x < w; x++) {
            const top = horizon - Math.round(heights[x] * h * tall * away(Math.abs(x - centre)));
            if (top < horizon) {
                p.set(x, top, edge);
                p.rect(x, top + 1, 1, horizon - top - 1, fill);
            }
        }
    };
    range(ridge(3, 0.18), 0.11, (off) => Math.min(1, off / (r * 1.6)) ** 1.4, NES.purple, NES.plum);
    range(ridge(11, 0.25), 0.07, (off) => Math.min(1, Math.max(0, off - r * 1.2) / (r * 1.8)), NES.rose, NES.violet);

    // The floor: lines across, nearer together further off, until they'd run
    // together, then every other row, dim; lines away, to the vanishing point.
    p.rect(0, horizon, w, h - horizon, NES.black);
    const shade = (y) => {
        const t = (y - horizon) / (h - horizon);
        return t < 0.12 ? NES.plum : t < 0.4 ? NES.purple : NES.orchid;
    };
    let last = Infinity;
    for (let z = 1; ; z += 0.3) {
        const y = horizon + Math.round((h - horizon) / z);
        if (last - y < 3) {
            for (let far = last - 2; far > horizon; far -= 2) {
                row(far, 0, w, NES.plum);
            }
            break;
        }
        if (y < h) {
            row(y, 0, w, shade(y));
        }
        last = y;
    }
    const line = (x0, y0, x1, y1) => {
        const steps = Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0));
        for (let s = 0; s <= steps; s++) {
            const y = Math.round(y0 + ((y1 - y0) * s) / steps);
            if (y < h) {
                p.set(x0 + ((x1 - x0) * s) / steps, y, shade(y));
            }
        }
    };
    const SPACING = 40;
    const reach = (last - horizon) / (h - horizon);
    for (let i = -Math.ceil(w / SPACING) * 2; i <= Math.ceil(w / SPACING) * 2; i++) {
        const bottom = centre + i * SPACING;
        line(Math.round(centre + (bottom - centre) * reach), last, bottom, h);
    }
    row(horizon, 0, w, NES.pink);
}

/// A city at night by the water, the moon up.
function city(p, centre) {
    const { w, h } = p;
    const random = seeded(5);
    const water = Math.round(h * 0.72);

    bands(p, 0, water, [NES.black, NES.navy, NES.blue, NES.violet, NES.plum], [4, 4, 3, 3, 2]);
    stars(p, random, 0, water * 0.45, (w * h) / 1400);

    // The moon, with its seas.
    const [mx, my, mr] = [Math.round(centre + w * 0.2), h * 0.2, Math.round(h * 0.085)];
    disc(p, mx, my, mr + 2, (x, y) => (dithered(x, y, 0.5) ? NES.violet : null));
    disc(p, mx, my, mr, (x, y, u, v) => {
        const sea = (u + 0.3) ** 2 + (v + 0.2) ** 2 < 0.12 || (u - 0.35) ** 2 + (v - 0.3) ** 2 < 0.06 || (u + 0.1) ** 2 + (v - 0.45) ** 2 < 0.04;
        return sea ? NES.lavender : NES.peach;
    });

    // Buildings, the far ones then the near, taller towards the middle:
    // lit windows, some antennas, and on the near ones a few neon signs.
    const skyline = (colour, lit, unlit, low, high, widths, signs, seed) => {
        const r = seeded(seed);
        for (let x = -Math.round(r() * 10); x < w;) {
            const width = Math.round(widths[0] + r() * (widths[1] - widths[0]));
            const middle = 1 - Math.min(1, Math.abs(x + width / 2 - centre) / (w * 0.45));
            const height = Math.round(h * (low + (high - low) * (0.35 * r() + 0.65 * r() * (0.4 + 0.6 * middle))));
            const top = water - height;
            p.rect(x, top, width, height, colour);
            for (let y = top + 2; y < water - 2; y += 3) {
                for (let wx = x + 1; wx < x + width - 1; wx += 2) {
                    const on = r();
                    if (on < lit) {
                        p.set(wx, y, [NES.amber, NES.cream, NES.amber, NES.cyan][Math.floor(r() * 4)]);
                    } else if (unlit && on < lit + 0.25) {
                        p.set(wx, y, unlit);
                    }
                }
            }
            if (r() < 0.35) {
                const tall = 3 + Math.floor(r() * 7);
                const ax = Math.round(x + width / 2);
                p.rect(ax, top - tall, 1, tall, colour);
                p.set(ax, top - tall - 1, NES.rust);
            }
            if (signs && r() < 0.3 && height > 20) {
                const [sx, sy] = [Math.round(x + 1 + r() * (width - 6)), top + 4 + Math.floor(r() * (height / 3))];
                const neon = r() < 0.5 ? NES.pink : NES.cyan;
                p.rect(sx, sy, 4, 1, neon);
                p.rect(sx, sy + 2, 4, 1, neon);
                p.set(sx, sy + 1, neon);
                p.set(sx + 3, sy + 1, neon);
            }
            x += width + (r() < 0.3 ? 1 + Math.floor(r() * 3) : 0);
        }
    };
    skyline(NES.violet, 0.04, NES.plum, 0.12, 0.42, [7, 16], false, 21);
    skyline(NES.black, 0.22, null, 0.06, 0.32, [9, 22], true, 37);

    // The water: dark, rippled, the buildings' feet faintly in it, their lit
    // windows streaks on it, and the moon's light a broken column.
    const lights = [];
    const light = new Set([NES.amber, NES.cream, NES.cyan, NES.pink].map(p.value));
    for (let y = water - 1; y > water - h * 0.35; y--) {
        for (let x = 0; x < w; x++) {
            if (light.has(p.at(x, y))) {
                lights.push([x, y, p.at(x, y)]);
            }
        }
    }
    for (let y = water; y < h; y++) {
        p.rect(0, y, w, 1, (y - water) % 5 === 2 ? NES.navy : NES.black);
    }
    for (let d = 1; d < 6; d++) {
        for (let x = 0; x < w; x++) {
            if (p.at(x, water - d) !== p.at(x, 2) && dithered(x, water + d, 0.5 - d * 0.08)) {
                p.set(x, water + d, NES.violet);
            }
        }
    }
    for (const [x, y, colour] of lights) {
        const ry = water + Math.round((water - y) * 0.7);
        if (ry < h && random() < 0.5) {
            const shift = Math.round(Math.sin(ry * 1.3) * 2);
            for (let k = -1; k <= 1; k++) {
                p.put(x + shift + k, ry, colour);
            }
        }
    }
    const rows = h - water;
    for (let y = water + 2; y < h; y += 2 + (y % 3 === 0 ? 1 : 0)) {
        const t = (y - water) / rows;
        const half = Math.round(mr * (0.25 + 0.75 * t) * (0.5 + 0.5 * Math.abs(Math.sin(y * 2.3))));
        const shift = Math.round(Math.sin(y * 0.7) * 2);
        for (let x = mx - half + shift; x <= mx + half + shift; x++) {
            p.set(x, y, t < 0.3 || (x + y) % 3 ? NES.peach : NES.lavender);
        }
    }
    p.rect(0, water, w, 1, NES.plum);
}

/// Space: a nebula, a ringed planet, a moon far off.
function space(p, centre) {
    const { w, h } = p;
    const cloud = noise(4);
    const tint = noise(8);

    // The nebula, corner to corner, in steps dithered together: purples, and
    // blues in places.
    const purples = [NES.black, NES.navy, NES.violet, NES.plum, NES.purple, NES.rose];
    const blues = [NES.black, NES.navy, NES.navy, NES.blue, NES.blue, NES.violet];
    for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
            const across = (x - w * 0.5) * 0.45 + (y - h * 0.5);
            const band = Math.exp(-((across / (h * 0.28)) ** 2));
            const thick = Math.max(0, (cloud(x / 38, y / 38) * 1.4 - 0.45) * band - 0.05) * 1.1;
            const level = Math.min(purples.length - 1.001, thick * 6);
            const step = Math.floor(level);
            const steps = tint(x / 60, y / 60) > 0.55 ? blues : purples;
            p.set(x, y, dithered(x, y, level - step) ? steps[step + 1] : steps[step]);
        }
    }
    stars(p, seeded(9), 0, h, (w * h) / 500);

    // The moon, lit from the right.
    const [sx, sy, sr] = [centre + w * 0.27, h * 0.24, Math.round(h * 0.05)];
    disc(p, sx, sy, sr, (x, y, u, v) => {
        const lit = u * 0.8 - v * 0.3 + 0.35;
        if (lit < 0) {
            return dithered(x, y, 0.25) ? NES.dark : NES.black;
        }
        const crater = (u - 0.2) ** 2 + (v + 0.3) ** 2 < 0.05 || (u + 0.25) ** 2 + (v - 0.35) ** 2 < 0.03;
        return crater ? NES.grey : lit < 0.25 && dithered(x, y, 0.5) ? NES.grey : NES.silver;
    });

    // The planet, low on the left, banded, lit from the upper right; its rings
    // behind it at the top and in front at the bottom.
    const [px, py, pr] = [centre - w * 0.2, h * 0.82, Math.round(h * 0.25)];
    const [cos, sin] = [Math.cos(-0.28), Math.sin(-0.28)];
    const ring = (x, y) => {
        const [dx, dy] = [x - px, y - py];
        const [u, v] = [dx * cos + dy * sin, -dx * sin + dy * cos];
        const out = Math.sqrt((u / (pr * 2)) ** 2 + (v / (pr * 0.42)) ** 2);
        if (out < 0.66 || out > 1 || (out > 0.83 && out < 0.86)) {
            return null;
        }
        return { front: v > 0, colour: out < 0.83 ? NES.lavender : NES.lilac, inner: out < 0.72 };
    };
    const onPlanet = (x, y) => (x - px) ** 2 + (y - py) ** 2 <= pr * pr;
    const rings = (front) => {
        for (let y = Math.floor(py - pr * 1.2); y < h; y++) {
            for (let x = Math.floor(px - pr * 2.2); x < px + pr * 2.2; x++) {
                const there = ring(x, y);
                if (there && there.front === front && (front || !onPlanet(x, y))) {
                    p.set(x, y, there.inner && dithered(x, y, 0.5) ? NES.lilac : there.colour);
                }
            }
        }
    };
    rings(false);
    const belts = [NES.cream, NES.amber, NES.coral, NES.amber, NES.peach, NES.coral, NES.rose, NES.amber, NES.cream, NES.coral];
    const darker = { [NES.cream]: NES.amber, [NES.amber]: NES.brown, [NES.coral]: NES.rust, [NES.peach]: NES.coral, [NES.rose]: NES.plum };
    const swirl = noise(2);
    disc(p, px, py, pr, (x, y, u, v) => {
        const latitude = v + (swirl(x / 14, y / 5) - 0.5) * 0.18;
        const colour = belts[Math.max(0, Math.min(belts.length - 1, Math.floor(((latitude + 1) / 2) * belts.length)))];
        const z = Math.sqrt(Math.max(0, 1 - u * u - v * v));
        const lit = u * 0.55 - v * 0.45 + z * 0.7;
        if (lit < 0.05) {
            return dithered(x, y, lit + 0.3) ? NES.violet : NES.black;
        }
        if (lit < 0.35) {
            return dithered(x, y, (lit - 0.05) / 0.3) ? colour : darker[colour];
        }
        return colour;
    });
    rings(true);
}

const SCENES = { sunset, city, space };

/// The scene `name` drawn on `canvas`, centred on `centre` across it.
export function drawScene(name, canvas, centre) {
    const p = pixels(canvas);
    SCENES[name](p, Math.round(centre));
    p.show();
}
