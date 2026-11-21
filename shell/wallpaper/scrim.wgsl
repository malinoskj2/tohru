// The tile's shade: clear over the top third, darkening to a.y at the foot so its name reads on any picture.
fn shade(s: Shader) -> vec4<f32> {
    let k = smoothstep(0.35, 1.0, s.uv.y);
    return vec4<f32>(s.color.rgb, mix(s.a.x, s.a.y, k));
}
