// j2bar's LiquidWarp on one icon (launcher-tile.frag): the icon's contours flow through turbulence
// while it surges, it scales and squashes with its spring, and it glows in its own colour. Over
// the reveal it also blurs and rides the card's ripple.
// a: growth, its velocity, glow on, the card's centre x; b: the reveal's blur, ripple map radius,
// ripple strength, the card's centre y. The centre is in the surface's pixels.

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn gradient(cell: vec2<f32>, salt: f32) -> vec2<f32> {
    let angle = hash(cell + vec2<f32>(salt, salt * 1.7)) * 6.2831853;
    return vec2<f32>(cos(angle), sin(angle));
}

fn perlin(p: vec2<f32>, salt: f32) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = dot(gradient(i, salt), f);
    let b = dot(gradient(i + vec2<f32>(1.0, 0.0), salt), f - vec2<f32>(1.0, 0.0));
    let c = dot(gradient(i + vec2<f32>(0.0, 1.0), salt), f - vec2<f32>(0.0, 1.0));
    let d = dot(gradient(i + vec2<f32>(1.0, 1.0), salt), f - vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn icon(s: Shader, q: vec2<f32>) -> vec4<f32> {
    if (q.x < 0.0 || q.y < 0.0 || q.x > s.size.x || q.y > s.size.y) { return vec4<f32>(0.0); }
    let c = inside(s, q);
    return vec4<f32>(c.rgb * c.a, c.a);
}

// feDisplacementMap over feTurbulence: the icon's contours moved by up to 55% of its size times
// the growth, through noise whose frequency breathes with the phase.
fn warped(s: Shader, q: vec2<f32>, growth: f32, phase: f32, salt: f32) -> vec4<f32> {
    let frequency = 1.1 / 44.0 * vec2<f32>(1.0 + 0.45 * sin(phase), 1.0 + 0.45 * cos(phase * 1.3));
    let n = vec2<f32>(perlin(q * frequency, salt), perlin(q * frequency + vec2<f32>(19.3, 7.1), salt + 3.0)) * 0.5;
    return icon(s, q + n * 44.0 * 0.55 * growth);
}

// The icon's colour for its glow: saturated opaque pixels count, monochrome artwork takes the
// launcher's sky.
fn glow_color(s: Shader) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    var total = 0.0;
    for (var j = 0; j < 8; j++) {
        for (var i = 0; i < 8; i++) {
            let c = inside(s, (vec2<f32>(f32(i), f32(j)) + 0.5) / 8.0 * s.size);
            if (c.a > 0.5) {
                let saturation = max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
                let w = saturation * saturation * c.a;
                sum += c.rgb * w;
                total += w;
            }
        }
    }
    if (total < 0.01) { return vec3<f32>(137.0, 220.0, 235.0) / 255.0; }
    return sum / total;
}

fn halo(s: Shader, q: vec2<f32>, sigma: f32, growth: f32, phase: f32, salt: f32) -> f32 {
    var sum = 0.0;
    var weights = 0.0;
    for (var k = 0; k < 24; k++) {
        let f = (f32(k) + 0.5) / 24.0;
        let r = 2.5 * sigma * sqrt(f);
        let angle = f32(k) * 2.39996323;
        let w = exp(-0.5 * r * r / (sigma * sigma));
        sum += warped(s, q + vec2<f32>(cos(angle), sin(angle)) * r, growth, phase, salt).a * w;
        weights += w;
    }
    return sum / weights;
}

fn blurred(s: Shader, q: vec2<f32>, sigma: f32) -> vec4<f32> {
    var sum = vec4<f32>(0.0);
    var weights = 0.0;
    for (var k = 0; k < 32; k++) {
        let f = (f32(k) + 0.5) / 32.0;
        let r = 2.5 * sigma * sqrt(f);
        let angle = f32(k) * 2.39996323;
        let w = exp(-0.5 * r * r / (sigma * sigma));
        sum += icon(s, q + vec2<f32>(cos(angle), sin(angle)) * r) * w;
        weights += w;
    }
    return sum / weights;
}

fn ripple(p: vec2<f32>, centre: vec2<f32>, map: f32, strength: f32) -> vec2<f32> {
    if (strength <= 0.0 || map <= 0.0) { return vec2<f32>(0.0); }
    let d = (p - centre) / map;
    let r = length(d);
    if (r >= 1.0 || r <= 0.0) { return vec2<f32>(0.0); }
    let f = sin(r * 6.2831853 * 4.5) * exp(-pow((r - 0.72) / 0.17, 2.0)) * (1.0 - smoothstep(0.9, 1.0, r));
    return d / r * f * (127.0 / 255.0) * strength;
}

fn straight(c: vec4<f32>) -> vec4<f32> {
    if (c.a <= 0.0) { return vec4<f32>(0.0); }
    return vec4<f32>(c.rgb / c.a, c.a);
}

fn shade(s: Shader) -> vec4<f32> {
    let growth = s.a.x;
    let velocity = s.a.y;
    let centre = vec2<f32>(s.a.w, s.b.w) - s.origin;
    var p = s.pos + ripple(s.pos, centre, s.b.y, s.b.z);
    if (s.b.x > 0.3) {
        return straight(blurred(s, p, s.b.x));
    }
    let salt = hash(floor(s.origin / 8.0)) * 97.0;
    let phase = s.time * (3.2 + 1.4 * abs(velocity) + 5.0 * abs(growth)) + salt;
    let middle = s.size * 0.5;
    let scale = 1.0 + 0.1 * growth;
    let squash = clamp(velocity * 0.009, -0.18, 0.18);
    p = middle + (p - middle) / vec2<f32>(scale + squash, scale - squash);
    let art = warped(s, p, growth, phase, salt);
    let k = clamp(abs(growth) / 0.6, 0.0, 1.5);
    let near = max(0.01, 3.5 * k);
    let far = max(0.01, 9.0 * k + min(5.0, abs(velocity) * 0.4));
    let alpha = vec2<f32>(halo(s, p, near, growth, phase, salt), halo(s, p, far, growth, phase, salt)) * vec2<f32>(0.6, 0.5) * min(1.0, k);
    let glow = (1.0 - (1.0 - alpha.x) * (1.0 - alpha.y)) * s.a.z;
    return straight(art + vec4<f32>(glow_color(s) * glow, glow) * (1.0 - art.a));
}
