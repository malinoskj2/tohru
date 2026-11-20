// Three columns of j2bar's Fill track (field.frag) while waves run, each column moved by its offset
// and thickened by its swell. a, b: offset and swell at the slice's four vertices, the first swell
// carrying 32 × the slice's number. color: fill start / 1024, fill end / 1024, swell / 8; color2:
// rounding, where the fill is on the ramp, track length / 1024. The glows stay with the whole track.

fn sq(v: f32) -> f32 { return v * v; }

fn over(top: vec4<f32>, bottom: vec4<f32>) -> vec4<f32> { return top + bottom * (1.0 - top.a); }

fn row_above(a: f32, b: f32) -> f32 {
    let lo = min(a, a + b);
    let hi = max(a, a + b);
    if (lo >= 1.0) { return 1.0; }
    if (hi <= 0.0) { return 0.0; }
    if (abs(b) < 1e-3) { return clamp(a + 0.5 * b, 0.0, 1.0); }
    var g0 = a - 0.5;
    if (a <= 0.0) { g0 = 0.0; } else if (a < 1.0) { g0 = 0.5 * a * a; }
    let c = a + b;
    var g1 = c - 0.5;
    if (c <= 0.0) { g1 = 0.0; } else if (c < 1.0) { g1 = 0.5 * c * c; }
    return (g1 - g0) / b;
}

fn cap(m: f32) -> f32 {
    if (m >= 2.0) { return 1.0; }
    return sqrt(max(0.0, 1.0 - sq(1.0 - m / 2.0)));
}

fn ramp_stop(i: i32) -> vec3<f32> {
    switch (i) {
        case 0: { return vec3<f32>(250.0, 179.0, 135.0); }
        case 1: { return vec3<f32>(249.0, 226.0, 175.0); }
        case 2: { return vec3<f32>(166.0, 227.0, 161.0); }
        case 3: { return vec3<f32>(180.0, 190.0, 254.0); }
        default: { return vec3<f32>(203.0, 166.0, 247.0); }
    }
}

fn ramp(t: f32) -> vec3<f32> {
    let k = min(clamp(t, 0.0, 1.0) * 4.0, 3.99999);
    let i = i32(floor(k));
    return mix(ramp_stop(i), ramp_stop(i + 1), k - floor(k)) / 255.0;
}

fn ramp_at(t: f32) -> vec3<f32> {
    let k = clamp(t, 0.0, 1.0) * 4.0;
    let i = min(i32(floor(k)), 3);
    return floor(ramp_stop(i) + (ramp_stop(i + 1) - ramp_stop(i)) * (k - f32(i)) + 0.5) / 255.0;
}

fn vertex(s: Shader, j: i32) -> vec2<f32> {
    switch (clamp(j, 0, 3)) {
        case 0: { return vec2<f32>(s.a.x, s.a.y - 32.0 * floor(s.a.y / 32.0)); }
        case 1: { return s.a.zw; }
        case 2: { return s.b.xy; }
        default: { return s.b.zw; }
    }
}

fn fill_half(s: Shader, x: f32, first: f32) -> f32 {
    let start = s.color.r * 1024.0;
    let end = s.color.g * 1024.0;
    let rounding = clamp(s.color2.r, 0.0, 1.0);
    let len = 2.0 + 9.0 * rounding;
    let ex = 40.0 + (2.2 - 40.0) * rounding;
    var h = 2.0 + vertex(s, i32(floor(x + 0.5) - first)).y + s.color.b * 8.0 * exp(-sq((x - (end - 6.0)) / 9.0));
    let m = x - start;
    if (m < 2.0) { h *= cap(max(m, 0.0)); }
    if (x > end - len) { h *= pow(max(0.0, 1.0 - pow(min(1.0, abs((x - (end - len)) / len)), ex)), 1.0 / ex); }
    return h;
}

fn shade(s: Shader) -> vec4<f32> {
    let k = floor(s.a.y / 32.0);
    let lx = s.pos.x - 16.0;
    if (lx < 0.0 || lx >= 3.0) { return vec4<f32>(0.0); }
    let j = i32(floor(lx));
    let first = 3.0 * k;
    let column = first + f32(j);
    let row = floor(s.pos.y - 16.0);
    let n = s.color2.b * 1024.0;
    let a = vertex(s, j);
    let b = vertex(s, j + 1);
    var frag = vec4<f32>(0.0);
    if (column >= 0.0 && column < n) {
        let ha = max(0.4, 1.2 + a.y) * cap(min(column, n - column));
        let hb = max(0.4, 1.2 + b.y) * cap(min(column + 1.0, n - column - 1.0));
        let track = row_above(a.x + ha - row, (b.x + hb) - (a.x + ha)) - row_above(a.x - ha - row, (b.x - hb) - (a.x - ha));
        let ka = min(0.55, abs(a.y) * 0.35 + abs(a.x) * 0.25);
        let kb = min(0.55, abs(b.y) * 0.35 + abs(b.x) * 0.25);
        frag = vec4<f32>(0.16) * track;
        frag = over(vec4<f32>(ramp_at(s.color2.g), 1.0) * mix(ka, kb, fract(lx)) * track, frag);
    }
    let lo = max(column, s.color.r * 1024.0);
    let hi = min(column + 1.0, s.color.g * 1024.0);
    if (hi > lo) {
        let ya = vertex(s, i32(floor(lo + 0.5) - first)).x;
        let yb = vertex(s, i32(floor(hi + 0.5) - first)).x;
        let fa = fill_half(s, lo, first);
        let fb = fill_half(s, hi, first);
        let fill = (hi - lo) * (row_above(ya + fa - row, (yb + fb) - (ya + fa)) - row_above(ya - fa - row, (yb - fb) - (ya - fa)));
        let liquid = vec4<f32>(ramp((column + 0.5) / min(n, 260.0)), 1.0);
        frag = over(liquid * fill, frag);
        frag = over(liquid * fill, frag);
    }
    if (frag.a <= 0.0) { return vec4<f32>(0.0); }
    return vec4<f32>(frag.rgb / frag.a, frag.a);
}
