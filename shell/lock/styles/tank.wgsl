// Flooded Monolith: shell/sound/tank.wgsl's clear water raised into a 320 × 436 vessel. The group
// holds the two numerals (over a 1 % sheet that only gives it its box); the water refracts and
// frosts the ones under it. a = level (0..1 from the bottom), slosh, wrong, vessel width;
// b = light glass, keystroke kick, drain (unlock), frozen clock.
const LAV = vec3<f32>(0.7059, 0.7451, 0.9961);

fn stop(i: i32) -> vec3<f32> {
    switch (((i % 5) + 5) % 5) {
        case 0: { return vec3<f32>(250.0, 179.0, 135.0) / 255.0; }
        case 1: { return vec3<f32>(249.0, 226.0, 175.0) / 255.0; }
        case 2: { return vec3<f32>(166.0, 227.0, 161.0) / 255.0; }
        case 3: { return vec3<f32>(180.0, 190.0, 254.0) / 255.0; }
        default: { return vec3<f32>(203.0, 166.0, 247.0) / 255.0; }
    }
}

fn hash(n: f32) -> f32 { return fract(sin(n * 127.1 + 3.7) * 43758.5453); }

fn h2(p: vec2<f32>) -> f32 {
    var h = bitcast<u32>(vec2<i32>(floor(p)).x) * 1597334677u ^ bitcast<u32>(vec2<i32>(floor(p)).y) * 3812015801u;
    h = h * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return f32(h) / 4294967295.0;
}

fn vn(p: vec2<f32>) -> f32 {
    let i = floor(p);
    var f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(h2(i), h2(i + vec2<f32>(1.0, 0.0)), f.x), mix(h2(i + vec2<f32>(0.0, 1.0)), h2(i + vec2<f32>(1.0, 1.0)), f.x), f.y);
}

fn dens(p: vec2<f32>, t: f32) -> f32 {
    var q = p / vec2<f32>(98.0, 72.0) + vec2<f32>(t * 0.085, -t * 0.05);
    q += 0.7 * vec2<f32>(vn(q * 1.3 + 7.0 + t * 0.06), vn(q * 1.3 - 3.0 - t * 0.05));
    return 0.68 * vn(q) + 0.32 * vn(q * 2.1 + 11.0);
}

fn wave(x: f32, t: f32) -> f32 {
    return 1.5 * sin(x * 0.045 + t * 1.15 + 1.0) + 0.7 * sin(x * 0.115 - t * 1.8 + 2.3) + 0.8 * sin(x * 0.021 + t * 0.55);
}

fn digit(s: Shader, at: vec2<f32>, pad: f32) -> f32 {
    return max(0.0, (inside(s, at + vec2<f32>(pad)).a - 0.01) / 0.99);
}

fn over(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb * (1.0 - a) + c * a, acc.a * (1.0 - a) + a);
}

fn add(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb + c * a, min(1.0, acc.a + a * 0.55));
}

fn shade(s: Shader) -> vec4<f32> {
    let pad = max(0.0, (s.size.x - s.a.w) * 0.5);
    let p = s.pos - vec2<f32>(pad);
    let size = s.size - vec2<f32>(2.0 * pad);
    let W = size.x;
    let H = size.y;
    let t = select(s.time, s.b.w, s.b.w > 0.0);
    let dpr = s.scale;
    let drain = clamp(s.b.z, 0.0, 1.0);
    let lv = clamp(s.a.x * (1.0 - drain * 1.15), 0.0, 1.0);
    let slosh = s.a.y + s.b.y * 0.35;
    let err = clamp(s.a.z, 0.0, 1.0);
    let pk = clamp(s.b.x, 0.0, 1.0);
    let g = clamp((lv - 0.08) / 0.84, 0.0, 1.0);
    let ink = mix(vec3<f32>(1.0), vec3<f32>(0.1098, 0.0941, 0.1490), pk);
    let errc = mix(vec3<f32>(0.953, 0.545, 0.659), vec3<f32>(0.824, 0.059, 0.224), pk);

    // Draining pulls the surface below the floor; past it there is no water left.
    let Y = mix(max(6.0, H * (1.0 - lv)), H + 10.0, smoothstep(0.75, 1.0, drain));
    let ys = Y + wave(p.x, t) * slosh;
    let depth = p.y - ys;
    let dz = max(depth, 0.0);
    let wet = clamp(depth * dpr + 0.5, 0.0, 1.0);

    var frost = 0.0;
    if (wet > 0.0) {
        let lo = mix(0.6, 0.08, g);
        frost = smoothstep(lo, lo + 0.46, dens(p, t)) * (0.3 + 0.7 * g) * 0.5 * smoothstep(0.0, 14.0, dz);
    }

    var acc = vec4<f32>(0.0);
    var dp = p;
    if (depth > 0.0) {
        dp += vec2<f32>(1.8 + 1.3 * sin(p.y * 0.23 + t * 1.4) + 0.5 * sin(p.x * 0.3 - t * 2.0), -2.4 - min(depth, 40.0) * 0.05);
    }
    let rad = frost * wet * 3.8;
    var dA = digit(s, dp, pad);
    if (rad > 0.15) {
        var sum = dA * 1.5;
        let spin = fract(p.y * 0.618) * 1.0472;
        for (var k = 0; k < 6; k++) {
            let a = f32(k) * 1.0472 + spin;
            sum += digit(s, dp + rad * 0.7 * vec2<f32>(cos(a), sin(a)), pad);
        }
        dA = sum / 7.5;
    }
    acc = over(acc, ink, dA * 0.94 * mix(1.0, mix(0.76, 0.38, frost), wet));

    let deep = clamp(depth / 140.0, 0.0, 1.0);
    acc = over(acc, LAV, wet * (0.04 + 0.06 * deep));
    acc = over(acc, mix(vec3<f32>(0.95, 0.93, 1.0), ink, pk), wet * frost * 0.24);
    acc = over(acc, ink, wet * 0.10 * exp(-dz / 5.0));

    var tintA = 0.0;
    var tintC = vec3<f32>(0.0);
    for (var k = 0; k < 5; k++) {
        let fk = f32(k);
        let ph = t / 3.7 + fk * 0.2;
        let n = floor(ph);
        let sn = sin(3.14159 * (ph - n));
        let e = sn * sn;
        let gx = W * (0.12 + 0.76 * hash(n * 5.3 + fk * 17.3));
        let gd = 16.0 + max(H - Y - 46.0, 0.0) * hash(n * 3.7 + fk * 9.1 + 2.0);
        let gc = mix(stop(i32(n + fk * 2.0)), errc, err * 0.85);
        let dx = p.x - gx;
        let dd = depth - gd;
        if (wet > 0.0) {
            let blob = exp(-(dx * dx / (2.0 * 48.0 * 48.0) + dd * dd / (2.0 * 32.0 * 32.0)));
            let sw = 11.0 + dz * 0.12;
            let shaft = exp(-pow(dx - dz * 0.18, 2.0) / (2.0 * sw * sw)) * exp(-dz / max(gd * 1.3, 12.0)) * 0.5;
            let v = wet * e * (blob * (0.5 + 0.9 * frost) + shaft * 0.7) * 0.16;
            acc = mix(add(acc, gc, v), over(acc, gc, v * 0.95), pk);
        }
        let la = e * exp(-dx * dx / (2.0 * 32.0 * 32.0));
        tintC += gc * la;
        tintA += la;
    }

    let line = clamp(0.5 - (abs(p.y - ys) - 0.75) * dpr, 0.0, 1.0) * (1.0 - smoothstep(0.85, 1.0, drain));
    var lc = ink;
    if (tintA > 0.0) {
        lc = mix(lc, tintC / max(tintA, 0.001), clamp(tintA, 0.0, 1.0) * 0.45);
    }
    lc = mix(lc, errc, err * 0.9);
    let glow = 0.18 * exp(-abs(p.y - ys) / 2.8) * (0.6 + 0.4 * clamp(tintA, 0.0, 1.0) + err * 0.6) * (1.0 - drain);
    acc = mix(add(acc, lc, glow), over(acc, lc, glow * 0.65), pk);
    acc = over(acc, lc, line * 0.88);
    if (err > 0.001 && wet > 0.0) {
        acc = over(acc, errc, wet * err * 0.14 * exp(-dz / 36.0));
    }

    let r = 15.0;
    let q = abs(p - size * 0.5) - size * 0.5 + vec2<f32>(r);
    let edge = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    acc *= clamp(0.5 - edge * dpr, 0.0, 1.0);

    let a = max(acc.a, max(acc.r, max(acc.g, acc.b)));
    if (a <= 0.0001) { return vec4<f32>(0.0); }
    return vec4<f32>(acc.rgb / a, min(a, 1.0));
}
