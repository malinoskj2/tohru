fn shade(s: Shader) -> vec4<f32> {
    let a = mix(s.a.x, s.a.y, clamp(s.pos.y / max(s.size.y, 1.0), 0.0, 1.0));
    return vec4<f32>(s.color.rgb, a);
}
