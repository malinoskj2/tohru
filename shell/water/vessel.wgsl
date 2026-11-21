// A card-sized group shader: clear water over drifting frost below a meniscus
// that crosses the whole card, refracting and frost-blurring whatever the
// group holds beneath it (a thin numeral).
//
// s.a = (level 0..1 from the bottom, slosh, pk, quiet)
// s.b = (clock, tint -1 green .. 0 lavender .. +1 red, unfold 0..1, corner px)
// The group must hold a sizing box of the card's size at 1% opacity.

const LAV = vec3<f32>(0.7059, 0.7451, 0.9961);
const RED = vec3<f32>(0.9529, 0.5451, 0.6588);
const GREEN = vec3<f32>(0.6510, 0.8902, 0.6314);
const RED_L = vec3<f32>(0.8235, 0.0588, 0.2235);
const GREEN_L = vec3<f32>(0.2510, 0.6275, 0.1686);
const INK_L = vec3<f32>(0.1098, 0.0941, 0.1490);

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

fn wave(x: f32, t: f32) -> f32 {
    return 1.35 * sin(x * 0.045 + t * 1.15 + 1.0)
         + 0.65 * sin(x * 0.115 - t * 1.8 + 2.3)
         + 0.70 * sin(x * 0.021 + t * 0.55);
}

fn held(s: Shader, at: vec2<f32>) -> f32 {
    return max(0.0, (inside(s, at).a - 0.01) / 0.99);
}

fn over(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb * (1.0 - a) + c * a, acc.a * (1.0 - a) + a);
}

fn glow_in(acc: vec4<f32>, c: vec3<f32>, a: f32, pk: f32) -> vec4<f32> {
    let dark_res = vec4<f32>(acc.rgb + c * a, min(1.0, acc.a + a * 0.55));
    let light_res = over(acc, mix(c, INK_L, 0.30), a * 0.75);
    return mix(dark_res, light_res, pk);
}

// Lavender at rest, red after a wrong answer, green once let through.
fn tinted(c: vec3<f32>, tint: f32, pk: f32, amount: f32) -> vec3<f32> {
    let red = mix(RED, RED_L, pk);
    let green = mix(GREEN, GREEN_L, pk);
    return mix(mix(c, red, clamp(tint, 0.0, 1.0) * amount), green, clamp(-tint, 0.0, 1.0) * amount);
}

fn shade(s: Shader) -> vec4<f32> {
    let p = s.pos;
    let size = s.size;
    let W = size.x;
    let H = size.y;
    let dpr = s.scale;
    let lv = clamp(s.a.x, 0.04, 0.96);
    let slosh = s.a.y;
    let pk = clamp(s.a.z, 0.0, 1.0);
    let quiet = clamp(s.a.w, 0.0, 1.0);
    let t = select(s.time, s.b.x, s.b.x > 0.0);
    let tint = clamp(s.b.y, -1.0, 1.0);
    let unfold = clamp(s.b.z, 0.0, 1.0);
    let corner = max(s.b.w, 0.0);
    let ink = mix(vec3<f32>(1.0), INK_L, pk);
    let frost_c = mix(vec3<f32>(0.95, 0.93, 1.0), ink, pk);
    let tank = 1.0 - smoothstep(0.05, 0.75, unfold);

    var acc = vec4<f32>(0.0);

    let yb = H * (1.0 - lv);
    let ys = yb + wave(p.x, t) * slosh;
    let depth = p.y - ys;
    let dz = max(depth, 0.0);
    let wet = clamp(depth * dpr + 0.5, 0.0, 1.0) * tank;
    let g = clamp((lv - 0.08) / 0.84, 0.0, 1.0);

    // Barely-there frost above the water, so the dry glass still breathes.
    acc = over(acc, frost_c, (1.0 - wet) * tank * smoothstep(0.45, 0.88, dens(p, t)) * 0.14 * mix(0.14, 0.07, pk));

    var frost = 0.0;
    if (wet > 0.0) {
        let lo = mix(0.56, 0.12, g);
        frost = smoothstep(lo, lo + 0.46, dens(p, t)) * (0.35 + 0.65 * g) * 0.50 * smoothstep(0.0, 14.0, dz);
    }

    // What the group holds: refracted and frost-blurred under the water, crisp above it.
    var dp = p;
    if (depth > 0.0) {
        dp += wet * vec2<f32>(
            1.7 + 1.1 * sin(p.y * 0.23 + t * 1.4) + 0.4 * sin(p.x * 0.3 - t * 2.0),
            -2.2 - min(depth, 40.0) * 0.05
        );
    }
    let rad = frost * wet * 3.2;
    var dA = held(s, dp);
    if (rad > 0.15) {
        var sum = dA * 1.5;
        let spin = fract(p.y * 0.618) * 1.0472;
        for (var k = 0; k < 6; k++) {
            let a = f32(k) * 1.0472 + spin;
            sum += held(s, dp + rad * 0.7 * vec2<f32>(cos(a), sin(a)));
        }
        dA = sum / 7.5;
    }
    let held_c = inside(s, dp).rgb;
    acc = over(acc, held_c, dA * mix(1.0, mix(0.78, 0.40, frost), wet));

    // The water body, tinted by how the request stands.
    let deep = clamp(depth / 120.0, 0.0, 1.0);
    acc = over(acc, tinted(LAV, tint, pk, 0.55), wet * (0.04 + 0.06 * deep + 0.05 * abs(tint)) * mix(1.0, 0.7, pk));
    acc = over(acc, frost_c, wet * frost * mix(0.24, 0.14, pk));
    acc = over(acc, ink, wet * 0.09 * exp(-dz / 5.0));

    // Unfolded: the water stills into a soft frosted wash with light plumes over the whole card.
    let u_frost = smoothstep(0.16, 0.64, dens(p, t)) * 0.42 * unfold;
    if (unfold > 0.001) {
        acc = over(acc, LAV, unfold * mix(0.04, 0.025, pk));
        acc = over(acc, frost_c, u_frost * mix(0.20, 0.12, pk));
    }

    let ga = 1.0 - 0.7 * quiet;
    var tintA = 0.0;
    var tintC = vec3<f32>(0.0);
    for (var k = 0; k < 5; k++) {
        let fk = f32(k);
        let ph = t / 3.7 + fk * 0.2;
        let n = floor(ph);
        let sn = sin(3.14159 * (ph - n));
        let e = sn * sn;
        let gx = W * (0.12 + 0.76 * hash(n * 5.3 + fk * 17.3));
        let gc = tinted(stop(i32(n + fk * 2.0) % 5), tint, pk, 0.78);
        let dx = p.x - gx;
        if (wet > 0.0) {
            let gd = 16.0 + max(H - yb - 36.0, 8.0) * hash(n * 3.7 + fk * 9.1 + 2.0);
            let dd = depth - gd;
            let blob = exp(-(dx * dx / (2.0 * 48.0 * 48.0) + dd * dd / (2.0 * 32.0 * 32.0)));
            let sw = 12.0 + dz * 0.12;
            let shaft = exp(-pow(dx - dz * 0.18, 2.0) / (2.0 * sw * sw)) * exp(-dz / max(12.0, gd * 1.3)) * 0.5;
            acc = glow_in(acc, gc, wet * ga * e * (blob * (0.5 + 0.9 * frost) + shaft * 0.7) * mix(0.16, 0.11, pk), pk);
        }
        if (unfold > 0.001) {
            let gy = H * (0.20 + 0.66 * hash(n * 3.7 + fk * 9.1 + 2.0));
            let dy = p.y - gy;
            let ublob = exp(-(dx * dx / (2.0 * 54.0 * 54.0) + dy * dy / (2.0 * 38.0 * 38.0)));
            acc = glow_in(acc, gc, unfold * ga * e * ublob * (0.55 + 0.85 * u_frost) * 0.15, pk);
        }
        let la = e * exp(-dx * dx / (2.0 * 34.0 * 34.0));
        tintC += gc * la;
        tintA += la;
    }

    // The meniscus: a luminous hairline, coloured by the light passing under it.
    if (tank > 0.001) {
        let d_line = abs(p.y - ys);
        let la = clamp(0.85 - d_line * dpr, 0.0, 1.0) * tank;
        var lc = ink;
        if (tintA > 0.0) {
            lc = mix(lc, tintC / max(tintA, 0.001), clamp(tintA, 0.0, 1.0) * 0.45 * ga);
        }
        lc = tinted(lc, tint, pk, 0.78);
        let glow = tank * 0.18 * exp(-d_line / 2.8) * (0.65 + 0.35 * clamp(tintA, 0.0, 1.0) * ga) * mix(1.0, 0.45, quiet);
        acc = glow_in(acc, lc, glow, pk);
        acc = over(acc, lc, la * mix(mix(0.82, 0.32, quiet), 0.62, pk));
    }

    let q = abs(p - size * 0.5) - size * 0.5 + vec2<f32>(corner);
    let edge = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - corner;
    acc *= clamp(0.5 - edge * dpr, 0.0, 1.0);

    let a = max(acc.a, max(acc.r, max(acc.g, acc.b)));
    if (a <= 0.0001) { return vec4<f32>(0.0); }
    return vec4<f32>(acc.rgb / a, min(a, 1.0));
}
