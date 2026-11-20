fn shade(s: Shader) -> vec4<f32> {
    let u = clamp(s.pos / max(s.size, vec2<f32>(1.0)), vec2<f32>(0.0), vec2<f32>(1.0));
    let bg = behind_frosted(s, s.pos);
    let bg_rgb = bg.rgb / max(bg.a, 0.001);
    let lum = dot(bg_rgb, vec3<f32>(0.299, 0.587, 0.114));
    let chroma = max(bg_rgb.r, max(bg_rgb.g, bg_rgb.b)) - min(bg_rgb.r, min(bg_rgb.g, bg_rgb.b));
    let flat_dark = smoothstep(0.36, 0.10, lum) * smoothstep(0.22, 0.04, chroma) * bg.a;
    let base_a = mix(s.a.x, s.a.y, u.y);
    var rgb = s.color.rgb * base_a;
    var a = base_a;

    // 1. Adaptive warm plum floor over flat dark apps
    let wash = mix(vec3<f32>(0.22, 0.10, 0.24), vec3<f32>(0.34, 0.15, 0.26), u.y);
    let lift_a = flat_dark * mix(0.20, 0.30, u.y);
    rgb = rgb * (1.0 - lift_a) + wash * lift_a;
    a = a * (1.0 - lift_a) + lift_a;

    // 2. Soft volume-tinted bloom from master rail bead
    let rc = s.color2.rgb;
    let bx = s.a.z;
    let d1 = (s.pos - vec2<f32>(bx, 22.0)) / vec2<f32>(125.0, 96.0);
    let bloom = exp(-dot(d1, d1) * 0.9) * (0.10 + 0.08 * flat_dark);
    rgb = rgb + rc * bloom;
    a = min(1.0, a + bloom * 0.5);

    // 3. Restrained overhead sheen
    let sheen = exp(-s.pos.y / 22.0) * (0.06 + 0.03 * (1.0 - u.x));
    rgb = rgb + vec3<f32>(0.97, 0.95, 1.0) * sheen;
    a = min(1.0, a + sheen * 0.5);

    return vec4<f32>(rgb / max(a, 0.0001), a);
}
