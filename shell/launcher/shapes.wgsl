// j2bar's drawing kit (src/shaders/kit.glsl, glass.frag, shape.frag, field.frag) for the launcher's
// shapes. Boxes are vec4(x, y, w, h) in the shader's pixels; colours are premultiplied until
// `straight` hands them to Pleamar.
fn kit_erf(v: f32) -> f32 {
    let s = sign(v);
    let x = abs(v);
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-x * x);
    return s * y;
}

fn kit_phi(z: f32) -> f32 {
    return 0.5 + 0.5 * kit_erf(z * 0.70710678);
}

fn kit_over(src: vec4<f32>, dst: vec4<f32>) -> vec4<f32> {
    return src + dst * (1.0 - src.a);
}

fn kit_tint(c: vec4<f32>, coverage: f32) -> vec4<f32> {
    let a = c.a * coverage;
    return vec4<f32>(c.rgb * a, a);
}

fn kit_radius(b: vec4<f32>, r: f32) -> f32 {
    return clamp(r, 0.0, min(b.z, b.w) * 0.5);
}

fn kit_distance(p: vec2<f32>, b: vec4<f32>, radius: f32) -> f32 {
    let r = kit_radius(b, radius);
    let h = b.zw * 0.5;
    let q = abs(p - b.xy - h) - h + r;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn kit_cover(p: vec2<f32>, b: vec4<f32>, r: f32) -> f32 {
    if (b.z <= 0.0 || b.w <= 0.0) { return 0.0; }
    return clamp(0.5 - kit_distance(p, b, r), 0.0, 1.0);
}

// The rounded box's coverage under a Gaussian of `sigma`, as CSS blurs a box-shadow.
fn kit_blurred(at: vec2<f32>, b: vec4<f32>, radius: f32, sigma: f32) -> f32 {
    if (sigma < 0.05) { return kit_cover(at, b, radius); }
    if (b.z <= 0.0 || b.w <= 0.0) { return 0.0; }
    let r = kit_radius(b, radius);
    let h = b.zw * 0.5;
    let p = at - (b.xy + h);
    if (r < 0.01) {
        return (kit_phi((p.x + h.x) / sigma) - kit_phi((p.x - h.x) / sigma))
            * (kit_phi((p.y + h.y) / sigma) - kit_phi((p.y - h.y) / sigma));
    }
    let low = p.y - h.y;
    let high = p.y + h.y;
    let start = clamp(-3.0 * sigma, low, high);
    let end = clamp(3.0 * sigma, low, high);
    let dy = (end - start) / 8.0;
    var y = start + dy * 0.5;
    var value = 0.0;
    for (var i = 0; i < 8; i++) {
        let row = p.y - y;
        let delta = min(h.y - r - abs(row), 0.0);
        let curved = h.x - r + sqrt(max(0.0, r * r - delta * delta));
        let across = kit_phi((p.x + curved) / sigma) - kit_phi((p.x - curved) / sigma);
        value += across * exp(-0.5 * y * y / (sigma * sigma)) * dy;
        y += dy;
    }
    return value / (2.50662827 * sigma);
}

fn straight(c: vec4<f32>) -> vec4<f32> {
    if (c.a <= 0.0) { return vec4<f32>(0.0); }
    return vec4<f32>(c.rgb / c.a, c.a);
}

// The reveal's ripple (view.rs `Reveal`): rings packed toward 72% of the radius `map`, each
// pushing outward or inward by up to `strength` × 127/255.
fn ripple(p: vec2<f32>, centre: vec2<f32>, map: f32, strength: f32) -> vec2<f32> {
    if (strength <= 0.0 || map <= 0.0) { return vec2<f32>(0.0); }
    let d = (p - centre) / map;
    let r = length(d);
    if (r >= 1.0 || r <= 0.0) { return vec2<f32>(0.0); }
    let f = sin(r * 6.2831853 * 4.5) * exp(-pow((r - 0.72) / 0.17, 2.0)) * (1.0 - smoothstep(0.9, 1.0, r));
    return d / r * f * (127.0 / 255.0) * strength;
}

// glass.frag's 160° rim: white .55 → .14 → .04 → .10 → .30 across the box.
fn rim_alpha(p: vec2<f32>, b: vec4<f32>) -> f32 {
    let angle = radians(160.0);
    let dir = vec2<f32>(sin(angle), -cos(angle));
    let len = abs(b.z * dir.x) + abs(b.w * dir.y);
    let t = clamp(dot(p - b.xy - b.zw * 0.5, dir) / len + 0.5, 0.0, 1.0);
    if (t < 0.22) { return mix(0.55, 0.14, t / 0.22); }
    if (t < 0.55) { return mix(0.14, 0.04, (t - 0.22) / 0.33); }
    if (t < 0.85) { return mix(0.04, 0.10, (t - 0.55) / 0.30); }
    return mix(0.10, 0.30, (t - 0.85) / 0.15);
}

// The light an inset box-shadow throws inside the box: everything outside its hole.
fn inset(p: vec2<f32>, b: vec4<f32>, radius: f32, grow: f32, dy: f32, sigma: f32) -> f32 {
    let hole = vec4<f32>(b.xy - grow + vec2<f32>(0.0, dy), b.zw + 2.0 * grow);
    return 1.0 - kit_blurred(p, hole, radius + grow, sigma);
}

fn grown(sigma: f32, blur: f32) -> f32 {
    return sqrt(sigma * sigma + blur * blur);
}

// j2bar's dark capsule, `a`: margin around the box, radius, pass (0 the shadows and tint under
// the content, 1 the inner light and rim over it), the reveal's blur; `b`: the reveal's ripple
// map radius and strength.
fn capsule(s: Shader) -> vec4<f32> {
    let m = s.a.x;
    let r = s.a.y;
    let blur = s.a.w;
    let b = vec4<f32>(m, m, s.size.x - 2.0 * m, s.size.y - 2.0 * m);
    let p = s.pos + ripple(s.pos, b.xy + b.zw * 0.5, s.b.x, s.b.y);
    let cover = kit_blurred(p, b, r, blur);
    if (s.a.z < 0.5) {
        let far = kit_tint(vec4<f32>(0.0, 0.0, 0.0, 0.30), kit_blurred(p, b + vec4<f32>(0.0, 14.0, 0.0, 0.0), r, grown(20.0, blur)));
        let near = kit_tint(vec4<f32>(0.0, 0.0, 0.0, 0.28), kit_blurred(p, b + vec4<f32>(0.0, 1.0, 0.0, 0.0), r, grown(0.75, blur)));
        var frag = kit_over(near, far) * (1.0 - cover);
        let y = clamp((p.y - b.y) / b.w, 0.0, 1.0);
        frag = kit_over(kit_tint(vec4<f32>(22.0 / 255.0, 16.0 / 255.0, 34.0 / 255.0, mix(0.30, 0.16, y)), cover), frag);
        return straight(frag);
    }
    let hole = kit_blurred(p, vec4<f32>(b.xy + 0.5, b.zw - 1.0), r - 0.5, blur);
    var frag = kit_tint(vec4<f32>(1.0, 1.0, 1.0, 0.08), max(cover - hole, 0.0));
    frag = kit_over(kit_tint(vec4<f32>(0.0, 0.0, 0.0, 0.22), inset(p, b, r, 12.0, -12.0, grown(9.0, blur)) * cover), frag);
    frag = kit_over(kit_tint(vec4<f32>(1.0, 1.0, 1.0, 0.32), inset(p, b, r, 10.0, 10.0, grown(7.0, blur)) * cover), frag);
    let content = kit_blurred(p, vec4<f32>(b.xy + 1.0, b.zw - 2.0), r - 1.0, blur);
    frag = kit_over(kit_tint(vec4<f32>(1.0, 1.0, 1.0, rim_alpha(p, b)), max(cover - content, 0.0)), frag);
    return straight(frag);
}

// A raised box as j2bar's shape.frag paints one: `a` margin, radius, ring width, the `inset 0 1px 0`
// highlight's white alpha; `b` the fill's alpha (colour), the ring's alpha (colour2) and how much
// of the hover pill's `0 2px 4σ` black .18 shadow it casts.
fn raised(s: Shader) -> vec4<f32> {
    let m = s.a.x;
    let r = s.a.y;
    let w = s.a.z;
    let b = vec4<f32>(m, m, s.size.x - 2.0 * m, s.size.y - 2.0 * m);
    let p = s.pos;
    let cover = kit_cover(p, b, r);
    var frag = kit_tint(vec4<f32>(0.0, 0.0, 0.0, 0.18 * s.b.z), kit_blurred(p, b + vec4<f32>(0.0, 2.0, 0.0, 0.0), r, 4.0)) * (1.0 - cover);
    frag = kit_over(kit_tint(vec4<f32>(s.color.rgb, s.b.x), cover), frag);
    if (s.a.w > 0.0) {
        let below = kit_cover(p - vec2<f32>(0.0, 1.0), b, r);
        frag = kit_over(kit_tint(vec4<f32>(1.0, 1.0, 1.0, s.a.w), cover * (1.0 - below)), frag);
    }
    if (s.b.y > 0.0 && w > 0.0) {
        let hole = kit_cover(p, vec4<f32>(b.xy + w, b.zw - 2.0 * w), r - w);
        frag = kit_over(kit_tint(vec4<f32>(s.color2.rgb, s.b.y), max(cover - hole, 0.0)), frag);
    }
    return straight(frag);
}

// The lit segment (segment.rs): an accent bar glowing `0 0 3.5px` at 90% and `0 0 9px+` at 75%.
// `a` margin, radius, the outer glow's sigma, how lit it is.
fn segment(s: Shader) -> vec4<f32> {
    let m = s.a.x;
    let r = s.a.y;
    let b = vec4<f32>(m, m, s.size.x - 2.0 * m, s.size.y - 2.0 * m);
    let p = s.pos;
    let cover = kit_cover(p, b, r);
    let outer = kit_tint(vec4<f32>(s.color.rgb, 0.75 * s.a.w), kit_blurred(p, b, r, s.a.z));
    let inner = kit_tint(vec4<f32>(s.color.rgb, 0.9 * s.a.w), kit_blurred(p, b, r, 1.75));
    var frag = kit_over(inner, outer) * (1.0 - cover);
    frag = kit_over(kit_tint(vec4<f32>(s.color.rgb, 1.0), cover), frag);
    return straight(frag);
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

// The workspace ramp, peach to mauve, as the track's liquid runs across it.
fn ramp(t: f32) -> vec3<f32> {
    let k = min(clamp(t, 0.0, 1.0) * 4.0, 3.99999);
    let i = i32(floor(k));
    return mix(ramp_stop(i), ramp_stop(i + 1), k - floor(k)) / 255.0;
}

// The ramp blended in whole 0–255 steps, as tokens.rs `ramp_at` does for the glow.
fn ramp_at(t: f32) -> vec3<f32> {
    let k = clamp(t, 0.0, 1.0) * 4.0;
    let i = min(i32(floor(k)), 3);
    let f = k - f32(i);
    return floor(ramp_stop(i) + (ramp_stop(i + 1) - ramp_stop(i)) * f + 0.5) / 255.0;
}

fn cap(m: f32) -> f32 {
    if (m >= 2.0) { return 1.0; }
    return sqrt(max(0.0, 1.0 - (1.0 - m / 2.0) * (1.0 - m / 2.0)));
}

fn fill_half(x: f32, start: f32, end: f32, swell: f32, len: f32, ex: f32) -> f32 {
    var h = 2.0 + swell * exp(-pow((x - (end - 6.0)) / 9.0, 2.0));
    if (x - start < 2.0) { h *= cap(max(x - start, 0.0)); }
    if (x > end - len) { h *= pow(max(0.0, 1.0 - pow(min(1.0, abs((x - (end - len)) / len)), ex)), 1.0 / ex); }
    return h;
}

// NeonField's Fill track (field.frag) without its waves: the track in 16% white and the liquid
// up to the caret over its two glows. `a` margin, fill start, fill end, swell above rest; `b` the
// wide glow's sigma, how round the leading edge is, the track's length, and 5 instead of 3 for the
// glows alone, while slice.wgsl draws the waving track and liquid.
fn track(s: Shader) -> vec4<f32> {
    let m = s.a.x;
    let start = s.a.y;
    let end = s.a.z;
    let n = s.b.z;
    let span = min(n, 260.0);
    let x = s.pos.x - m;
    let y = s.pos.y - m;
    let glows_only = s.b.w > 4.0;
    var frag = vec4<f32>(0.0);
    if (x >= 0.0 && x <= n && !glows_only) {
        let h = 1.2 * cap(min(x, n - x));
        frag = vec4<f32>(0.16) * clamp(h - abs(y) + 0.5, 0.0, 1.0);
    }
    let rounding = clamp(s.b.y, 0.0, 1.0);
    let len = 2.0 + 9.0 * rounding;
    let ex = 40.0 + (2.2 - 40.0) * rounding;
    let fh = fill_half(x, start, end, s.a.w, len, ex);
    let across = clamp(min(x - start, end - x) + 0.5, 0.0, 1.0);
    let fill = across * clamp(fh - abs(y) + 0.5, 0.0, 1.0) * select(1.0, 0.0, glows_only);
    let wide = sqrt(s.b.x * s.b.x + 1.0 / 12.0);
    let tight = sqrt(1.75 * 1.75 + 1.0 / 12.0);
    var gw = 0.0;
    var gt = 0.0;
    let column = floor(x);
    for (var k = -24; k <= 24; k++) {
        let j = column + f32(k);
        let l = max(j, start);
        let r = min(j + 1.0, end);
        if (r <= l) { continue; }
        let h = fill_half((l + r) * 0.5, start, end, s.a.w, len, ex);
        gw += (kit_phi((x - l) / wide) - kit_phi((x - r) / wide)) * (kit_phi((y + h) / wide) - kit_phi((y - h) / wide));
        gt += (kit_phi((x - l) / tight) - kit_phi((x - r) / tight)) * (kit_phi((y + h) / tight) - kit_phi((y - h) / tight));
    }
    let glow = vec4<f32>(ramp_at(end / span), 1.0);
    let liquid = vec4<f32>(ramp(x / span), 1.0);
    frag = kit_over(glow * 0.75 * gw, frag);
    frag = kit_over(liquid * fill, frag);
    frag = kit_over(glow * 0.9 * gt, frag);
    frag = kit_over(liquid * fill, frag);
    return straight(frag);
}

// One file for the launcher's drawn shapes: the last value says which.
fn shade(s: Shader) -> vec4<f32> {
    let mode = i32(round(s.b.w));
    if (mode == 0) { return capsule(s); }
    if (mode == 1) { return raised(s); }
    if (mode == 2) { return segment(s); }
    return track(s);
}
