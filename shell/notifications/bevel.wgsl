// s.a: time left (1 → 0), pk, critical, counting. s.b.x: held under the pointer.
// The capillary runs from the top-left corner down the left edge, round the corner and along the
// bottom; what is lit is the time left, so its head drains down the left edge and then along the
// bottom bevel to the bottom-right corner. Colour stays on the rim.
const R = 15.0;

fn stop(i: i32) -> vec3<f32> {
    switch (i) {
        case 0: { return vec3<f32>(250.0, 179.0, 135.0) / 255.0; }
        case 1: { return vec3<f32>(249.0, 226.0, 175.0) / 255.0; }
        case 2: { return vec3<f32>(166.0, 227.0, 161.0) / 255.0; }
        case 3: { return vec3<f32>(180.0, 190.0, 254.0) / 255.0; }
        default: { return vec3<f32>(203.0, 166.0, 247.0) / 255.0; }
    }
}

fn ribbon(u: f32) -> vec3<f32> {
    let x = clamp(u, 0.0, 1.0) * 4.0;
    let i = min(i32(floor(x)), 3);
    return mix(stop(i), stop(i + 1), x - f32(i));
}

// Signed distance to the card's rounded rectangle, negative inside.
fn card(p: vec2<f32>, size: vec2<f32>) -> f32 {
    let h = size * 0.5;
    let q = abs(p - h) - (h - vec2<f32>(R));
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - R;
}

// Where along the capillary a point is (px from its start) and how far in from it.
fn along(p: vec2<f32>, size: vec2<f32>) -> vec2<f32> {
    let corner = vec2<f32>(R, size.y - R);
    let edge = size.y - R;
    if (p.x < R && p.y > edge) {
        let d = p - corner;
        let turn = atan2(max(d.y, 0.0), max(-d.x, 0.0001));
        return vec2<f32>(edge + R * turn, R - length(d));
    }
    if (p.x < size.y - p.y) {
        return vec2<f32>(p.y, p.x);
    }
    return vec2<f32>(edge + 1.5707963 * R + (p.x - R), size.y - p.y);
}

fn length_of(size: vec2<f32>) -> f32 {
    return (size.y - R) + 1.5707963 * R + (size.x - R);
}

fn shade(s: Shader) -> vec4<f32> {
    let p = s.pos;
    let size = max(s.size, vec2<f32>(2.0 * R + 1.0));
    let left = clamp(s.a.x, 0.0, 1.0);
    let pk = clamp(s.a.y, 0.0, 1.0);
    let critical = clamp(s.a.z, 0.0, 1.0);
    let counting = clamp(s.a.w, 0.0, 1.0);
    let held = clamp(s.b.x, 0.0, 1.0);
    let d = card(p, size);
    let cover = clamp(0.5 - d, 0.0, 1.0);
    if (cover <= 0.0) { return vec4<f32>(0.0); }
    let ink = mix(vec3<f32>(1.0), vec3<f32>(0.1098, 0.0941, 0.1490), pk);
    var rgb = vec3<f32>(0.0);
    var a = 0.0;

    if (critical > 0.001) {
        // The whole bevel refracts rose into peach, brightest at the top-left.
        let inset = max(-d, 0.0);
        let rose = mix(vec3<f32>(0.953, 0.545, 0.659), vec3<f32>(0.824, 0.059, 0.224), pk);
        let peach = mix(vec3<f32>(0.980, 0.702, 0.529), vec3<f32>(0.996, 0.392, 0.043), pk);
        let u = clamp(p.x / size.x * 0.65 + p.y / size.y * 0.35, 0.0, 1.0);
        let c = mix(rose, peach, u);
        let rim = exp(-inset / 1.6) * 0.85 + exp(-inset / 6.0) * 0.22 + exp(-inset / 18.0) * 0.07;
        let ca = rim * critical * mix(0.9, 0.7, pk);
        rgb = c * ca;
        a = ca;
    } else if (counting > 0.001) {
        let total = length_of(size);
        let at = along(p, size);
        let inset = max(at.y, 0.0);
        let near = exp(-pow((inset - 1.2) / 0.85, 2.0)) + exp(-inset / 5.0) * 0.32;
        // A faint empty channel, then the light that is left.
        let channel = near * 0.10;
        let head = (1.0 - left) * total;
        let lit = smoothstep(head - 1.5, head + 6.0, at.x) * step(0.002, left);
        let meniscus = exp(-pow((at.x - head - 4.0) / 5.0, 2.0)) * step(0.002, left) * 0.55;
        let c = mix(ribbon(at.x / total * 0.6 + (1.0 - left) * 0.4), ink, 0.35 * pk);
        let glow = min(1.0, near * (lit + meniscus) * mix(1.0, 0.85, pk) * (1.0 + 0.15 * held) * counting);
        rgb = ink * channel * mix(1.0, 0.6, pk) + c * glow;
        a = min(1.0, channel * mix(1.0, 0.6, pk) + glow);
    }
    a = a * cover;
    rgb = rgb * cover;
    if (a <= 0.0001) { return vec4<f32>(0.0); }
    return vec4<f32>(min(rgb / a, vec3<f32>(1.0)), min(a, 1.0));
}
