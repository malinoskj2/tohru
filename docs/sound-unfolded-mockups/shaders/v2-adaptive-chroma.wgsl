fn shade(s: Shader) -> vec4<f32> {
    let u = clamp(s.pos / max(s.size, vec2<f32>(1.0)), vec2<f32>(0.0), vec2<f32>(1.0));
    let bg = behind_frosted(s, s.pos);
    let bg_rgb = bg.rgb / max(bg.a, 0.001);
    let lum = dot(bg_rgb, vec3<f32>(0.299, 0.587, 0.114));
    let chroma = max(bg_rgb.r, max(bg_rgb.g, bg_rgb.b)) - min(bg_rgb.r, min(bg_rgb.g, bg_rgb.b));
    let flat_dark = smoothstep(0.36, 0.10, lum) * smoothstep(0.22, 0.04, chroma) * bg.a;
    let base_a = mix(s.a.x, s.a.y, u.y);
    // Warm plum / rose / mauve diagonal glass depth that emerges only over flat dark backdrops.
    let c_top = vec3<f32>(0.26, 0.11, 0.24);
    let c_bot = vec3<f32>(0.36, 0.16, 0.26);
    let c_side = vec3<f32>(0.24, 0.13, 0.34);
    let wash = mix(mix(c_top, c_side, u.x * 0.7), c_bot, smoothstep(0.15, 1.0, u.y));
    let lift_a = flat_dark * mix(0.28, 0.40, u.y);
    var rgb = s.color.rgb * base_a;
    var a = base_a;
    rgb = rgb * (1.0 - lift_a) + wash * lift_a;
    a = a * (1.0 - lift_a) + lift_a;
    return vec4<f32>(rgb / max(a, 0.0001), min(a, 1.0));
}
