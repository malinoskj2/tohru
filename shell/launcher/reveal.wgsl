// The reveal on what the card holds (view.rs `Reveal`): blurred by `a.x` and displaced by the ripple
// spreading from the card's centre. a: blur, ripple map radius, ripple strength, the centre's x;
// b.x the centre's y, in the surface's pixels.

fn ripple(p: vec2<f32>, centre: vec2<f32>, map: f32, strength: f32) -> vec2<f32> {
    if (strength <= 0.0 || map <= 0.0) { return vec2<f32>(0.0); }
    let d = (p - centre) / map;
    let r = length(d);
    if (r >= 1.0 || r <= 0.0) { return vec2<f32>(0.0); }
    let f = sin(r * 6.2831853 * 4.5) * exp(-pow((r - 0.72) / 0.17, 2.0)) * (1.0 - smoothstep(0.9, 1.0, r));
    return d / r * f * (127.0 / 255.0) * strength;
}

fn held(s: Shader, q: vec2<f32>) -> vec4<f32> {
    if (q.x < 0.0 || q.y < 0.0 || q.x > s.size.x || q.y > s.size.y) { return vec4<f32>(0.0); }
    let c = inside(s, q);
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn shade(s: Shader) -> vec4<f32> {
    let p = s.pos + ripple(s.pos, vec2<f32>(s.a.w, s.b.x) - s.origin, s.a.y, s.a.z);
    var c = held(s, p);
    let sigma = s.a.x;
    if (sigma > 0.3) {
        var sum = vec4<f32>(0.0);
        var weights = 0.0;
        for (var k = 0; k < 32; k++) {
            let f = (f32(k) + 0.5) / 32.0;
            let r = 2.5 * sigma * sqrt(f);
            let angle = f32(k) * 2.39996323;
            let w = exp(-0.5 * r * r / (sigma * sigma));
            sum += held(s, p + vec2<f32>(cos(angle), sin(angle)) * r) * w;
            weights += w;
        }
        c = sum / weights;
    }
    if (c.a <= 0.0) { return vec4<f32>(0.0); }
    return vec4<f32>(c.rgb / c.a, c.a);
}
