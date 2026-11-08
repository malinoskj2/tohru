// Port of j2bar src/shaders/corner.frag; see J2BAR-LICENSE.
// The centre points into the output: (1,1), (0,1), (0,0), (1,0).
fn shade(s: Shader) -> vec4<f32> {
    let n = abs(s.pos - s.a.xy * s.size) / max(s.size, vec2<f32>(1.0));
    let n2 = n * n;
    let shape = n2.x * n2.x + n2.y * n2.y - 1.0;
    // Match the source's linear, one-real-pixel antialias coverage.
    let pixel_width = max(length(vec2<f32>(dpdx(shape), dpdy(shape))), 0.000001);
    return vec4<f32>(0.0, 0.0, 0.0, clamp(0.5 + shape / pixel_width, 0.0, 1.0));
}
