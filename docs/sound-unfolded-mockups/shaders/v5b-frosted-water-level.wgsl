const LAV = vec3<f32>(0.7059, 0.7451, 0.9961);
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
fn over(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb * (1.0 - a) + c * a, acc.a * (1.0 - a) + a);
}
fn add(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb + c * a, min(1.0, acc.a + a * 0.55));
}

fn shade(s: Shader) -> vec4<f32> {
    let p = s.pos;
    let W = max(s.size.x, 1.0);
    let H = max(s.size.y, 1.0);
    let u = clamp(p / vec2<f32>(W, H), vec2<f32>(0.0), vec2<f32>(1.0));
    let lv = clamp(s.a.z, 0.0, 1.0);
    let t = 1.7;
    let g = clamp((lv - 0.08) / 0.84, 0.0, 1.0);

    let base_a = mix(s.a.x, s.a.y, u.y);
    var acc = vec4<f32>(s.color.rgb * base_a, base_a);

    // Tank's water surface at the current volume level, gently softened behind unfolded rows
    let Y = max(6.0, H * (1.0 - lv));
    let ys = Y + wave(p.x, t);
    let depth = p.y - ys;
    let dz = max(depth, 0.0);
    let wet = clamp(depth * s.scale + 0.5, 0.0, 1.0) * 0.72;

    var frost = 0.0;
    if (wet > 0.0) {
        let lo = mix(0.60, 0.08, g);
        frost = smoothstep(lo, lo + 0.46, dens(p, t)) * (0.3 + 0.7 * g) * 0.5 * smoothstep(0.0, 14.0, dz);
        let deep = clamp(depth / 120.0, 0.0, 1.0);
        acc = over(acc, LAV, wet * (0.04 + 0.06 * deep));
        acc = over(acc, vec3<f32>(0.95, 0.93, 1.0), wet * frost * 0.24);
        acc = over(acc, vec3<f32>(1.0), wet * 0.08 * exp(-dz / 5.0));
    }

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
        let gc = stop(i32(n + fk * 2.0) % 5);
        let dx = p.x - gx;
        let dd = depth - gd;
        if (wet > 0.0) {
            let blob = exp(-(dx * dx / (2.0 * 46.0 * 46.0) + dd * dd / (2.0 * 30.0 * 30.0)));
            let sw = 11.0 + dz * 0.12;
            let shaft = exp(-pow(dx - dz * 0.18, 2.0) / (2.0 * sw * sw)) * exp(-dz / (gd * 1.3)) * 0.5;
            acc = add(acc, gc, wet * e * (blob * (0.5 + 0.9 * frost) + shaft * 0.7) * 0.16);
        }
        let la = e * exp(-dx * dx / (2.0 * 30.0 * 30.0));
        tintC += gc * la;
        tintA += la;
    }

    // Softened meniscus line so it adds depth without competing with the rows
    let d = abs(p.y - ys) - 0.65;
    let la = clamp(0.5 - d * s.scale, 0.0, 1.0);
    var lc = vec3<f32>(1.0);
    if (tintA > 0.0) {
        lc = mix(lc, tintC / max(tintA, 0.001), clamp(tintA, 0.0, 1.0) * 0.42);
    }
    acc = add(acc, lc, 0.11 * exp(-abs(p.y - ys) / 2.6));
    acc = over(acc, lc, la * 0.42);

    let a = max(acc.a, max(acc.r, max(acc.g, acc.b)));
    return vec4<f32>(acc.rgb / max(a, 0.0001), min(a, 1.0));
}
