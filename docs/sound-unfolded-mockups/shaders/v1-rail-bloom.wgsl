fn over(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb * (1.0 - a) + c * a, acc.a * (1.0 - a) + a);
}
fn shade(s: Shader) -> vec4<f32> {
    let u = s.pos / max(s.size, vec2<f32>(1.0));
    let base_a = mix(s.a.x, s.a.y, clamp(u.y, 0.0, 1.0));
    var acc = vec4<f32>(s.color.rgb * base_a, base_a);
    let rc = s.color2.rgb;
    let bx = s.a.z;
    let d1 = (s.pos - vec2<f32>(bx, 22.0)) / vec2<f32>(118.0, 96.0);
    let bloom1 = exp(-dot(d1, d1) * 0.9) * 0.22;
    let d2 = (s.pos - vec2<f32>(s.size.x - bx * 0.45, s.size.y * 0.86)) / vec2<f32>(140.0, 90.0);
    let bloom2 = exp(-dot(d2, d2) * 1.1) * 0.08;
    let glow_a = bloom1 + bloom2;
    acc = vec4<f32>(acc.rgb + rc * glow_a, min(1.0, acc.a + glow_a * 0.55));
    let a = max(acc.a, max(acc.r, max(acc.g, acc.b)));
    return vec4<f32>(acc.rgb / max(a, 0.0001), min(a, 1.0));
}
