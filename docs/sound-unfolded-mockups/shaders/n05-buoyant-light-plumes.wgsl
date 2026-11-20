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
    return 1.8 * sin(x * 0.045 + t * 1.15 + 1.0) + 0.8 * sin(x * 0.115 - t * 1.8 + 2.3) + 0.9 * sin(x * 0.021 + t * 0.55);
}
fn over(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb * (1.0 - a) + c * a, acc.a * (1.0 - a) + a);
}
fn add(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb + c * a, min(1.0, acc.a + a * 0.55));
}
fn sd_rrect(p: vec2<f32>, size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p - size * 0.5) - size * 0.5 + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}
fn base_5a(p: vec2<f32>, W: f32, H: f32, lv: f32, t: f32, a_top: f32, a_bot: f32, glass: vec3<f32>, frost_scale: f32, light_scale: f32) -> vec4<f32> {
    let u = clamp(p / vec2<f32>(W, H), vec2<f32>(0.0), vec2<f32>(1.0));
    let g = clamp((lv - 0.08) / 0.84, 0.0, 1.0);
    let base_a = mix(a_top, a_bot, u.y);
    var acc = vec4<f32>(glass * base_a, base_a);
    let lo = mix(0.56, 0.14, g);
    let frost = smoothstep(lo, lo + 0.46, dens(p, t)) * (0.35 + 0.65 * g) * 0.44 * frost_scale;
    acc = over(acc, LAV, (0.03 + 0.05 * u.y) * frost_scale);
    acc = over(acc, vec3<f32>(0.95, 0.93, 1.0), frost * 0.21);
    for (var k = 0; k < 5; k++) {
        let fk = f32(k);
        let ph = t / 3.7 + fk * 0.2;
        let n = floor(ph);
        let sn = sin(3.14159 * (ph - n));
        let e = sn * sn;
        let gx = W * (0.12 + 0.76 * hash(n * 5.3 + fk * 17.3));
        let gy = 26.0 + max(H - 56.0, 0.0) * hash(n * 3.7 + fk * 9.1 + 2.0);
        let gc = stop(i32(n + fk * 2.0) % 5);
        let dx = p.x - gx;
        let dy = p.y - gy;
        let blob = exp(-(dx * dx / (2.0 * 56.0 * 56.0) + dy * dy / (2.0 * 38.0 * 38.0)));
        acc = add(acc, gc, e * blob * (0.65 + 0.8 * frost) * 0.15 * light_scale);
    }
    return acc;
}
fn finish(acc: vec4<f32>) -> vec4<f32> {
    let a = max(acc.a, max(acc.r, max(acc.g, acc.b)));
    return vec4<f32>(acc.rgb / max(a, 0.0001), min(a, 1.0));
}

fn shade(s: Shader) -> vec4<f32> {
    let p = s.pos; let W = max(s.size.x, 1.0); let H = max(s.size.y, 1.0);
    let u = clamp(p / vec2<f32>(W, H), vec2<f32>(0.0), vec2<f32>(1.0));
    let lv = clamp(s.a.z, 0.0, 1.0); let t = 1.7;
    var acc = base_5a(p, W, H, lv, t, s.a.x, s.a.y, s.color.rgb, 0.9, 0.85);
    // Soft upward-drifting volumetric sunbeams / buoyant light shafts diffusing through frosted water (no caustic net).
    let depth_fade = smoothstep(0.0, 0.35, u.y) * smoothstep(1.0, 0.72, u.y * 0.85);
    for (var k = 0; k < 4; k++) {
        let fk = f32(k);
        let x0 = W * (0.16 + 0.23 * fk) + sin(t * 0.5 + fk * 2.1) * 14.0;
        let tilt_x = p.x - (x0 + p.y * 0.22);
        let w_shaft = 18.0 + p.y * 0.10;
        let shaft = exp(-(tilt_x * tilt_x) / (2.0 * w_shaft * w_shaft));
        let gc = stop(k + 1);
        acc = add(acc, mix(vec3<f32>(1.0), gc, 0.65), shaft * depth_fade * 0.085 * (0.5 + 0.5 * lv));
    }
    return finish(acc);
}
