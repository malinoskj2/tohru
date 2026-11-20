const LAV = vec3<f32>(0.7059, 0.7451, 0.9961);
const GOLDEN = 2.39996323;

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

// Integer hash matching shell/sound/tank.wgsl so adjacent value-noise cells stay seamless.
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
    var q = p / vec2<f32>(56.0, 62.0) + vec2<f32>(t * 0.085, -t * 0.055);
    q += 0.72 * vec2<f32>(vn(q * 1.3 + 7.0 + t * 0.06), vn(q * 1.3 - 3.0 - t * 0.05));
    return 0.68 * vn(q) + 0.32 * vn(q * 2.1 + 11.0);
}

fn wave(x: f32, t: f32) -> f32 {
    return 1.35 * sin(x * 0.065 + t * 1.25 + 1.0)
        + 0.60 * sin(x * 0.145 - t * 1.90 + 2.3)
        + 0.65 * sin(x * 0.032 + t * 0.60);
}

// The group holds the numeral and icon over a 1 % black sheet that only gives the group its box.
fn fg(s: Shader, at: vec2<f32>, pad: f32) -> f32 {
    return max(0.0, (inside(s, at + vec2<f32>(pad)).a - 0.01) / 0.99);
}

fn over(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb * (1.0 - a) + c * a, acc.a * (1.0 - a) + a);
}

fn add(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb + c * a, min(1.0, acc.a + a * 0.55));
}

// s.a: fill (0..1), slosh multiplier, inertia tilt (px), tank logical width (62).
// s.b: quiet (0..1), pk (0 dark .. 1 light), fixed clock override (>0), unused.
fn shade(s: Shader) -> vec4<f32> {
    let pad = max(0.0, (s.size.x - s.a.w) * 0.5);
    let p = s.pos - vec2<f32>(pad);
    let size = s.size - vec2<f32>(2.0 * pad);
    let t = select(s.time, s.b.z, s.b.z > 0.0);
    let dpr = s.scale;
    let lv = clamp(s.a.x, 0.0, 1.0);
    let slosh = s.a.y;
    let tilt = s.a.z;
    let quiet = clamp(s.b.x, 0.0, 1.0);
    let pk = clamp(s.b.y, 0.0, 1.0);
    let W = size.x;
    let H = size.y;
    let g = clamp((lv - 0.06) / 0.88, 0.0, 1.0);
    let ink = mix(vec3<f32>(1.0), vec3<f32>(0.1098, 0.0941, 0.1490), pk);

    let top_pad = 6.0;
    let bot_pad = 4.0;
    let usable = max(H - top_pad - bot_pad, 1.0);
    let yb = select(H - bot_pad - usable * lv, H + 10.0, lv <= 0.002);

    let nx = (p.x / max(W, 1.0)) * 2.0 - 1.0;
    let cap_edge = -0.7 * pow(abs(nx), 4.0) * smoothstep(0.0, 0.08, lv) * smoothstep(1.0, 0.94, lv);
    let wv = wave(p.x, t) * slosh * (1.0 - 0.55 * quiet);
    let ys = yb + wv + nx * tilt + cap_edge;
    let depth = p.y - ys;
    let dz = max(depth, 0.0);
    let has_water = smoothstep(0.0, 0.015, lv);
    let wet = clamp(depth * dpr + 0.5, 0.0, 1.0) * has_water;

    var frost = 0.0;
    if (wet > 0.0) {
        let lo = mix(0.56, 0.12, g);
        frost = smoothstep(lo, lo + 0.46, dens(p, t)) * (0.34 + 0.66 * g) * 0.54 * smoothstep(0.0, 12.0, dz);
    }

    var acc = vec4<f32>(0.0);

    // Subtle top glass optical sheen.
    let sheen = exp(-max(p.y, 0.0) / 14.0) * mix(0.055, 0.11, pk);
    acc = over(acc, vec3<f32>(1.0), sheen);

    // Refract, soften through a Vogel disc blur, and dim the static numeral and foot glyph beneath the water.
    var dp = p;
    if (wet > 0.01) {
        dp += wet * vec2<f32>(
            1.4 + 0.9 * sin(p.y * 0.18 + t * 1.25) + 0.35 * sin(p.x * 0.24 - t * 1.6),
            -1.9 - min(dz, 38.0) * 0.04
        );
    }
    let rad = frost * wet * 2.3;
    var dA = fg(s, dp, pad);
    if (rad > 0.15) {
        var sum = dA * 1.8;
        var wsum = 1.8;
        for (var k = 1; k <= 12; k++) {
            let fk = f32(k);
            let rr = sqrt(fk / 12.0);
            let ang = fk * GOLDEN;
            let wt = 1.0 - 0.35 * rr;
            sum += fg(s, dp + rad * rr * vec2<f32>(cos(ang), sin(ang)), pad) * wt;
            wsum += wt;
        }
        dA = sum / wsum;
    }
    let fg_alpha = dA * mix(0.94, 0.38, quiet) * mix(1.0, mix(0.78, 0.42, frost), wet);
    acc = over(acc, ink, fg_alpha);

    // Clear barely-tinted water body, organic satin frost, and subsurface shadow under the meniscus.
    let deep = clamp(dz / max(H * 0.65, 60.0), 0.0, 1.0);
    acc = over(acc, LAV, wet * (0.04 + 0.06 * deep) * (1.0 - 0.5 * pk));
    acc = over(acc, mix(vec3<f32>(0.95, 0.93, 1.0), ink, pk), wet * frost * 0.24);
    acc = over(acc, ink, wet * mix(0.11, 0.09, pk) * exp(-dz / 4.8));

    // Slow-breathing ambient light from the workspace ribbon's five-stop palette.
    let ga = (1.0 - quiet) * has_water;
    var tintA = 0.0;
    var tintC = vec3<f32>(0.0);
    if (ga > 0.001) {
        for (var k = 0; k < 5; k++) {
            let fk = f32(k);
            let ph = t / 3.7 + fk * 0.2;
            let n = floor(ph);
            let sn = sin(3.14159 * (ph - n));
            let e = sn * sn;
            let gx = W * (0.14 + 0.72 * hash(n * 5.3 + fk * 17.3));
            let gd = 14.0 + max(H - yb - 36.0, 0.0) * hash(n * 3.7 + fk * 9.1 + 2.0);
            let gc = stop(i32(n + fk * 2.0) % 5);
            let dx = p.x - gx;
            let dd = depth - gd;
            if (wet > 0.0) {
                let blob = exp(-(dx * dx / (2.0 * 34.0 * 34.0) + dd * dd / (2.0 * 28.0 * 28.0)));
                let sw = 9.0 + dz * 0.12;
                let shaft = exp(-pow(dx - dz * 0.16, 2.0) / (2.0 * sw * sw)) * exp(-dz / max(gd * 1.3, 20.0)) * 0.55;
                acc = add(acc, gc, wet * ga * e * (blob * (0.5 + 0.9 * frost) + shaft * 0.7) * mix(0.16, 0.11, pk));
            }
            let la = e * exp(-dx * dx / (2.0 * 24.0 * 24.0));
            tintC += gc * la;
            tintA += la;
        }
    }

    // Luminous meniscus hairline and soft glow, or a faint floor glint when muted/empty.
    if (lv > 0.01) {
        let m_dist = abs(p.y - ys);
        let hw = 0.72;
        let la = clamp(0.5 - (m_dist - hw) * dpr, 0.0, 1.0);
        var lc = ink;
        if (tintA > 0.001) {
            lc = mix(lc, tintC / max(tintA, 0.001), clamp(tintA, 0.0, 1.0) * 0.44 * ga);
        }
        let glow = 0.18 * exp(-m_dist / 2.8) * (0.6 + 0.4 * clamp(tintA, 0.0, 1.0) * ga) * mix(1.0, 0.45, quiet);
        acc = mix(add(acc, lc, glow), over(acc, lc, glow * 0.65), pk);
        acc = over(acc, lc, la * mix(0.84, 0.36, quiet));
    } else {
        let floor_y = H - bot_pad - 1.5;
        let f_dist = abs(p.y - floor_y);
        let fla = clamp(0.5 - (f_dist - 0.5) * dpr, 0.0, 1.0) * smoothstep(W * 0.46, W * 0.18, abs(p.x - W * 0.5));
        acc = over(acc, ink, fla * 0.18);
    }

    // Clip cleanly to the 15 px rounded vessel corners.
    let r = 15.0;
    let q = abs(p - size * 0.5) - size * 0.5 + vec2<f32>(r);
    let edge = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    acc *= clamp(0.5 - edge * dpr, 0.0, 1.0);

    let a = max(acc.a, max(acc.r, max(acc.g, acc.b)));
    if (a <= 0.0001) { return vec4<f32>(0.0); }
    return vec4<f32>(acc.rgb / a, min(a, 1.0));
}
