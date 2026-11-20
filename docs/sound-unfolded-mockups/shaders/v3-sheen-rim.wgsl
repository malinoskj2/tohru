fn shade(s: Shader) -> vec4<f32> {
    let u = clamp(s.pos / max(s.size, vec2<f32>(1.0)), vec2<f32>(0.0), vec2<f32>(1.0));
    let base_a = mix(s.a.x, s.a.y, u.y);
    var rgb = s.color.rgb * base_a;
    var a = base_a;
    // Soft overhead specular sheen + subtle diagonal catchlight + lower rim bounce.
    let top_sheen = exp(-s.pos.y / 26.0) * (0.095 + 0.045 * (1.0 - u.x));
    let diag = exp(-pow((u.x * 0.8 + u.y * 0.6 - 0.35) / 0.32, 2.0)) * 0.035;
    let bot_bounce = exp(-(s.size.y - s.pos.y) / 18.0) * 0.045;
    let hi = top_sheen + diag + bot_bounce;
    rgb = rgb + vec3<f32>(0.96, 0.94, 1.0) * hi;
    a = min(1.0, a + hi * 0.6);
    return vec4<f32>(rgb / max(a, 0.0001), a);
}
