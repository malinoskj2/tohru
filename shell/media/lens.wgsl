// A group shader over the artwork. b.w picks what it draws:
// 0, the panel: a = fold, pk, paused dim, panel width; b.x empty, b.y has artwork, b.z clock (0 runs).
//    The artwork strip is a lens on the left; its mean colour bleeds into the frosted pane.
// 1, a player row's colour tab: a = opacity, pk, palette slot for a player without artwork, box width.
const INK_LIGHT = vec3<f32>(0.1098, 0.0941, 0.1490);

fn stop(i: i32) -> vec3<f32> {
    switch (i) {
        case 0: { return vec3<f32>(250.0, 179.0, 135.0) / 255.0; }
        case 1: { return vec3<f32>(249.0, 226.0, 175.0) / 255.0; }
        case 2: { return vec3<f32>(166.0, 227.0, 161.0) / 255.0; }
        case 3: { return vec3<f32>(180.0, 190.0, 254.0) / 255.0; }
        default: { return vec3<f32>(203.0, 166.0, 247.0) / 255.0; }
    }
}

fn hash(n: f32) -> f32 { return fract(sin(n * 127.1 + 3.7) * 43758.5453); }

fn h2(p: vec2<f32>) -> f32 {
    var h = bitcast<u32>(vec2<i32>(p).x) * 1597334677u ^ bitcast<u32>(vec2<i32>(p).y) * 3812015801u;
    h = h * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return f32(h) / 4294967295.0;
}

fn vn(p: vec2<f32>) -> f32 {
    let i = floor(p);
    var f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(h2(i), h2(i + vec2<f32>(1.0, 0.0)), f.x),
        mix(h2(i + vec2<f32>(0.0, 1.0)), h2(i + vec2<f32>(1.0, 1.0)), f.x),
        f.y
    );
}

fn dens(p: vec2<f32>, t: f32) -> f32 {
    var q = p / vec2<f32>(98.0, 72.0) + vec2<f32>(t * 0.085, -t * 0.05);
    q += 0.7 * vec2<f32>(vn(q * 1.3 + 7.0 + t * 0.06), vn(q * 1.3 - 3.0 - t * 0.05));
    return 0.68 * vn(q) + 0.32 * vn(q * 2.1 + 11.0);
}

fn over(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb * (1.0 - a) + c * a, acc.a * (1.0 - a) + a);
}

fn add(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb + c * a, min(1.0, acc.a + a * 0.55));
}

fn round_box(p: vec2<f32>, lo: vec2<f32>, size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p - lo - size * 0.5) - size * 0.5 + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn finish(acc: vec4<f32>) -> vec4<f32> {
    let a = max(acc.a, max(acc.r, max(acc.g, acc.b)));
    if (a <= 0.0001) { return vec4<f32>(0.0); }
    return vec4<f32>(acc.rgb / a, min(a, 1.0));
}

// The mean colour over a box of the group, weighted by how much of it is there; alpha says how much was.
fn mean(s: Shader, lo: vec2<f32>, size: vec2<f32>, pad: f32) -> vec4<f32> {
    var sum = vec4<f32>(0.0);
    for (var i = 0; i < 4; i++) {
        for (var j = 0; j < 6; j++) {
            let at = lo + size * vec2<f32>((f32(i) + 0.5) / 4.0, (f32(j) + 0.5) / 6.0);
            let c = inside(s, at + vec2<f32>(pad));
            sum += vec4<f32>(c.rgb * c.a, c.a);
        }
    }
    if (sum.a < 0.01) { return vec4<f32>(0.0); }
    var c = sum.rgb / sum.a;
    // A cover's mean is greyer than it looks: lift its saturation a little.
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    c = clamp(mix(vec3<f32>(l), c, 1.35), vec3<f32>(0.0), vec3<f32>(1.0));
    // And its light, not its shadows: a dark cover still lends a colour, at about the mock's brightness.
    c = c * (0.8 / max(max(c.r, max(c.g, c.b)), 0.25));
    return vec4<f32>(c, sum.a / 24.0);
}

fn tab(s: Shader) -> vec4<f32> {
    let pad = max(0.0, (s.size.x - s.a.w) * 0.5);
    let p = s.pos - vec2<f32>(pad);
    let pk = clamp(s.a.y, 0.0, 1.0);
    let art = mean(s, vec2<f32>(0.0), vec2<f32>(s.a.w), pad);
    let fallback = stop(i32(s.a.z) % 5);
    var c = mix(fallback, art.rgb, smoothstep(0.2, 0.6, art.a));
    c = mix(c, INK_LIGHT, 0.35 * pk);
    let edge = round_box(p, vec2<f32>(0.0), vec2<f32>(4.0, s.a.w), 2.0);
    return vec4<f32>(c, clamp(0.5 - edge * s.scale, 0.0, 1.0) * clamp(s.a.x, 0.0, 1.0));
}

fn shade(s: Shader) -> vec4<f32> {
    if (s.b.w > 0.5) { return tab(s); }
    let pad = max(0.0, (s.size.x - s.a.w) * 0.5);
    let p = s.pos - vec2<f32>(pad);
    let size = s.size - vec2<f32>(2.0 * pad);
    let W = size.x;
    let H = size.y;
    let fc = clamp(s.a.x, 0.0, 1.0);
    let pk = clamp(s.a.y, 0.0, 1.0);
    let dim = clamp(s.a.z, 0.0, 1.0);
    let empty = clamp(s.b.x, 0.0, 1.0);
    let has_art = clamp(s.b.y, 0.0, 1.0) * (1.0 - empty);
    let t = select(s.time, s.b.z, s.b.z > 0.0);
    let dpr = s.scale;
    let ink = mix(vec3<f32>(1.0), INK_LIGHT, pk);
    let quiet = max(dim, empty);

    let lo = vec2<f32>(8.0, 8.0);
    let strip = vec2<f32>(74.0 - 6.0 * fc, 150.0 - 38.0 * fc);
    let art = mean(s, lo, strip, pad);
    let art_k = has_art * smoothstep(0.2, 0.6, art.a);
    let edge = round_box(p, lo, strip, 10.0);
    let cover = clamp(0.5 - edge * dpr, 0.0, 1.0);

    var acc = vec4<f32>(0.0);

    // The pane: frost that thickens away from the lens, the cover's light, and quiet palette glints.
    if (cover < 1.0) {
        let clear = smoothstep(lo.x + strip.x, W, p.x);
        let f = smoothstep(0.32, 0.84, dens(p, t)) * mix(0.18, 0.15, quiet) * mix(0.85, 1.1, dim) * mix(0.55, 1.35, clear);
        acc = over(acc, mix(vec3<f32>(0.95, 0.93, 1.0), ink, pk), f);
        if (quiet < 0.99) {
            for (var k = 0; k < 5; k++) {
                let fk = f32(k);
                let ph = t / 3.7 + fk * 0.2;
                let n = floor(ph);
                let sn = sin(3.14159 * (ph - n));
                let e = sn * sn;
                let g = vec2<f32>(W * (0.12 + 0.76 * hash(n * 5.3 + fk * 17.3)), H * (0.16 + 0.68 * hash(n * 3.7 + fk * 9.1 + 2.0)));
                let d = p - g;
                let blob = exp(-(d.x * d.x / (2.0 * 54.0 * 54.0) + d.y * d.y / (2.0 * 42.0 * 42.0)));
                acc = add(acc, stop(i32(n + fk * 2.0) % 5), (1.0 - quiet) * e * blob * (0.45 + 2.25 * f) * 0.13);
            }
        }
        if (art_k > 0.0) {
            // The lens's light: wide and soft from its right edge, and a seam that hugs it.
            let o = vec2<f32>(lo.x + strip.x + 4.0, lo.y + strip.y * 0.5);
            let d = p - o;
            let bleed = exp(-(d.x * d.x / (2.0 * 118.0 * 118.0) + d.y * d.y / (2.0 * 82.0 * 82.0)));
            let seam = exp(-max(edge, 0.0) / 6.0) * smoothstep(lo.x + strip.x - 6.0, lo.x + strip.x + 2.0, p.x);
            acc = add(acc, art.rgb, art_k * (bleed * 0.184 + seam * 0.10) * (1.0 - 0.55 * dim));
        }
        acc *= 1.0 - cover;
    }

    // The strip: the cover itself, out of focus and greyed while paused, under a bevel and a sheen.
    if (cover > 0.0) {
        var c = vec3<f32>(0.0);
        var a = 0.0;
        if (art_k > 0.0) {
            let r = 5.0 * dim;
            var sum = inside(s, p + vec2<f32>(pad));
            if (r > 0.2) {
                let turn = fract(p.y * 0.618) * 0.7854;
                for (var k = 0; k < 8; k++) {
                    let ang = f32(k) * 0.7854 + turn;
                    sum += inside(s, p + vec2<f32>(pad) + r * vec2<f32>(cos(ang), sin(ang)));
                    sum += inside(s, p + vec2<f32>(pad) + 0.5 * r * vec2<f32>(cos(ang + 0.39), sin(ang + 0.39)));
                }
                sum /= 17.0;
            }
            let l = dot(sum.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
            c = mix(sum.rgb, vec3<f32>(l), 0.55 * dim) * (1.0 - 0.12 * dim);
            a = art_k;
            // The chamfer: a bevel of light down the right side.
            let bx = clamp((p.x - (lo.x + strip.x - 14.0)) / 14.0, 0.0, 1.0);
            let bevel = select(mix(0.0, mix(0.22, 0.42, pk), bx / 0.7), mix(mix(0.22, 0.42, pk), mix(0.34, 0.12, pk), (bx - 0.7) / 0.3), bx > 0.7);
            let bevel_c = select(vec3<f32>(1.0), mix(vec3<f32>(1.0), INK_LIGHT, step(0.7, bx) * pk), bx > 0.7);
            c = mix(c, bevel_c, bevel * step(0.0001, bx));
            let q = (p - lo) / strip;
            let sheen = clamp((q.x * strip.x * strip.x + q.y * strip.y * strip.y * 0.6) / (strip.x * strip.x + strip.y * strip.y * 0.36), 0.0, 1.0);
            c = select(mix(c, vec3<f32>(1.0), 0.22 * (1.0 - sheen * 2.0)), mix(c, vec3<f32>(0.0), 0.20 * (sheen * 2.0 - 1.0)), sheen > 0.5);
        }
        // Without a cover the lens is empty glass.
        let glass = vec4<f32>(ink * mix(0.05, 0.06, pk), mix(0.05, 0.06, pk));
        var lens = vec4<f32>(c * a, a) + glass * (1.0 - a);
        let rim = clamp(1.0 - abs(edge + 0.5) * dpr, 0.0, 1.0);
        lens = over(lens, ink, rim * mix(0.20, 0.14, pk));
        acc += lens * cover;
    }

    // The panel's own rounded corners.
    let outer = round_box(p, vec2<f32>(0.0), size, 15.0);
    acc *= clamp(0.5 - outer * dpr, 0.0, 1.0);
    return finish(acc);
}
