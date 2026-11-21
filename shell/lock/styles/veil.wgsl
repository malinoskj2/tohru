// A full-screen veil over the frozen screen: the group holds a lightly blurred copy of it, and
// the heavily blurred backdrop shows through wherever the veil frosts over.
// Mode 0 (Drifting Frost Veil): drifting frost clouds, a pocket behind the password, a thaw.
// Mode 1 (Horizon Meniscus): clear sky above a living waterline, frosted liquid below.
// a = melt (unlock), wrong, focus (mode 0) or horizon y (mode 1), light glass;
// b = focus x, focus y (mode 0) or slosh, keystroke kick (mode 1), mode * 100 + softening
// radius, frozen clock.
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

fn dens(p: vec2<f32>, t: f32, scale: vec2<f32>) -> f32 {
    var q = p / scale + vec2<f32>(t * 0.085, -t * 0.05);
    q += 0.7 * vec2<f32>(vn(q * 1.3 + 7.0 + t * 0.06), vn(q * 1.3 - 3.0 - t * 0.05));
    return 0.68 * vn(q) + 0.32 * vn(q * 2.1 + 11.0);
}

fn over(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb * (1.0 - a) + c * a, acc.a * (1.0 - a) + a);
}

fn add(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb + c * a, min(1.0, acc.a + a * 0.5));
}

// The copy, softened over a disc so that nothing on it reads.
fn soft(s: Shader, at: vec2<f32>) -> vec4<f32> {
    let r = s.b.z - 100.0 * step(50.0, s.b.z);
    if (r < 0.5) { return inside(s, at); }
    var sum = inside(s, at) * 2.0;
    for (var k = 0; k < 12; k++) {
        let a = f32(k) * 0.5236;
        let d = select(r, r * 0.5, k % 2 == 1);
        sum += inside(s, at + d * vec2<f32>(cos(a), sin(a)));
    }
    return sum / 14.0;
}

fn done(acc: vec4<f32>) -> vec4<f32> {
    let a = clamp(acc.a, 0.0, 1.0);
    if (a <= 0.0001) { return vec4<f32>(0.0); }
    return vec4<f32>(clamp(acc.rgb / a, vec3<f32>(0.0), vec3<f32>(1.0)), a);
}

fn frost_clouds(s: Shader, t: f32) -> vec4<f32> {
    let p = s.pos;
    let uv = p / max(s.size, vec2<f32>(1.0));
    let melt = clamp(s.a.x, 0.0, 1.0);
    let err = clamp(s.a.y, 0.0, 1.0);
    let focus = clamp(s.a.z, 0.0, 1.0);
    let pk = clamp(s.a.w, 0.0, 1.0);
    let ink = mix(vec3<f32>(1.0), vec3<f32>(0.1098, 0.0941, 0.1490), pk);
    let errc = mix(vec3<f32>(0.953, 0.545, 0.659), vec3<f32>(0.824, 0.059, 0.224), pk);
    let glass = mix(vec3<f32>(0.0784, 0.0392, 0.1176), vec3<f32>(0.9804, 0.9804, 0.9882), pk);

    let n = dens(p, t, vec2<f32>(280.0, 210.0));
    let dc = length((uv - vec2<f32>(0.5)) * vec2<f32>(1.5, 1.0));
    var thaw = smoothstep(melt * 1.45 - 0.22, melt * 1.45 + 0.18, dc + (n - 0.5) * 0.45);
    if (melt < 0.001) { thaw = 1.0; }
    let fp = (uv - s.b.xy) * vec2<f32>(1.7778, 1.0);
    let pocket = exp(-(fp.x * fp.x / (2.0 * 0.14 * 0.14) + fp.y * fp.y / (2.0 * 0.065 * 0.065))) * focus;
    let frost = clamp(smoothstep(0.24, 0.74, n) * 0.88 + pocket * 0.65, 0.0, 1.0) * thaw;

    let sharp = soft(s, p);
    var acc = vec4<f32>(sharp.rgb, 1.0) * sharp.a * (1.0 - frost);
    acc = over(acc, glass, frost * mix(0.26, 0.22, pk));
    acc = over(acc, mix(vec3<f32>(0.95, 0.93, 1.0), ink, pk * 0.15), frost * 0.08);

    for (var k = 0; k < 5; k++) {
        let fk = f32(k);
        let ph = t / 4.2 + fk * 0.2;
        let idx = floor(ph);
        let sn = sin(3.14159 * (ph - idx));
        let e = sn * sn;
        let gp = vec2<f32>(0.14 + 0.72 * hash(idx * 5.3 + fk * 17.3), 0.16 + 0.68 * hash(idx * 3.7 + fk * 9.1 + 2.0));
        let dp = (uv - gp) * vec2<f32>(1.7778, 1.0);
        let blob = exp(-dot(dp, dp) / (2.0 * 0.18 * 0.18));
        let gc = stop(i32(idx + fk * 2.0));
        let amt = e * blob * (0.45 + 0.8 * frost) * 0.14 * thaw;
        acc = mix(add(acc, gc, amt), over(acc, gc, amt * 1.1), step(0.5, pk));
    }

    if (err > 0.001) {
        let ring = exp(-pow(length(fp) - 0.14, 2.0) / (2.0 * 0.05 * 0.05));
        acc = over(acc, errc, (pocket * 0.22 + ring * 0.18) * err);
    }
    return done(acc);
}

fn meniscus(s: Shader, t: f32) -> vec4<f32> {
    let p = s.pos;
    let melt = clamp(s.a.x, 0.0, 1.0);
    let err = clamp(s.a.y, 0.0, 1.0);
    let pk = clamp(s.a.w, 0.0, 1.0);
    let slosh = s.b.x + s.b.y * 0.25;
    let ink = mix(vec3<f32>(1.0), vec3<f32>(0.1098, 0.0941, 0.1490), pk);
    let errc = mix(vec3<f32>(0.953, 0.545, 0.659), vec3<f32>(0.824, 0.059, 0.224), pk);
    let glass = mix(vec3<f32>(0.0784, 0.0392, 0.1176), vec3<f32>(0.9804, 0.9804, 0.9882), pk);

    let Y = s.a.z;
    let wv = (2.4 * sin(p.x * 0.011 + t * 1.05 + 0.7) + 1.2 * sin(p.x * 0.029 - t * 1.65 + 2.1) + 1.4 * sin(p.x * 0.0055 + t * 0.45)) * slosh;
    let ys = Y + wv;
    let depth = p.y - ys;
    let dz = max(depth, 0.0);
    let wet = clamp(depth * s.scale + 0.5, 0.0, 1.0);

    var at = p;
    if (depth > 0.0) {
        at += vec2<f32>(2.2 * sin(p.y * 0.08 + t * 1.2), -2.5 - min(depth, 80.0) * 0.03);
    }
    let sharp = soft(s, at);
    let n = dens(p, t, vec2<f32>(220.0, 150.0));
    let frost = smoothstep(0.18, 0.72, n) * smoothstep(0.0, 42.0, dz);
    // Above the line the clearer copy; below it mostly the frosted backdrop, tinted like glass.
    let clear = mix(1.0, 1.0 - (0.62 + 0.36 * frost), wet);
    var acc = vec4<f32>(sharp.rgb, 1.0) * sharp.a * clear;
    acc = over(acc, glass, wet * (mix(0.28, 0.24, pk) + 0.14 * frost));
    acc = over(acc, LAV, wet * (0.04 + 0.05 * clamp(dz / 300.0, 0.0, 1.0)));
    acc = over(acc, ink, wet * 0.07 * exp(-dz / 12.0));

    var tintA = 0.0;
    var tintC = vec3<f32>(0.0);
    for (var k = 0; k < 5; k++) {
        let fk = f32(k);
        let ph = t / 3.9 + fk * 0.2;
        let idx = floor(ph);
        let sn = sin(3.14159 * (ph - idx));
        let e = sn * sn;
        let gx = s.size.x * (0.10 + 0.80 * hash(idx * 5.3 + fk * 17.3));
        let gd = 30.0 + max(s.size.y - Y - 80.0, 40.0) * hash(idx * 3.7 + fk * 9.1 + 2.0);
        let gc = mix(stop(i32(idx + fk * 2.0)), errc, err * 0.85);
        let dx = p.x - gx;
        let dd = depth - gd;
        if (wet > 0.0) {
            let blob = exp(-(dx * dx / (2.0 * 140.0 * 140.0) + dd * dd / (2.0 * 90.0 * 90.0)));
            let sw = 32.0 + dz * 0.14;
            let shaft = exp(-pow(dx - dz * 0.16, 2.0) / (2.0 * sw * sw)) * exp(-dz / max(gd * 1.4, 40.0)) * 0.55;
            let v = wet * e * (blob * (0.45 + 0.75 * frost) + shaft * 0.65) * 0.15;
            acc = mix(add(acc, gc, v), over(acc, gc, v * 1.05), step(0.5, pk));
        }
        let la = e * exp(-dx * dx / (2.0 * 95.0 * 95.0));
        tintC += gc * la;
        tintA += la;
    }

    let fade = 1.0 - smoothstep(0.8, 1.0, melt);
    let la = clamp(0.5 - (abs(p.y - ys) - 0.85) * s.scale, 0.0, 1.0) * fade;
    var lc = ink;
    if (tintA > 0.0) { lc = mix(lc, tintC / max(tintA, 0.001), clamp(tintA, 0.0, 1.0) * 0.45); }
    lc = mix(lc, errc, err * 0.92);
    let glow = 0.22 * exp(-abs(p.y - ys) / 4.2) * (0.65 + 0.35 * clamp(tintA, 0.0, 1.0) + err * 0.7) * fade;
    acc = mix(add(acc, lc, glow), over(acc, lc, glow * 0.55), step(0.5, pk));
    acc = over(acc, lc, la * 0.88);
    if (err > 0.001 && wet > 0.0) {
        acc = over(acc, errc, wet * err * 0.14 * exp(-dz / 60.0));
    }
    return done(acc);
}

fn shade(s: Shader) -> vec4<f32> {
    let t = select(s.time, s.b.w, s.b.w > 0.0);
    if (s.b.z >= 50.0) { return meniscus(s, t); }
    return frost_clouds(s, t);
}
