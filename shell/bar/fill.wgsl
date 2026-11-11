// j2bar's bar tint: 18% at its top edge, which sits one pixel above the screen, down to 6% at
// the bottom. s.a is that top, that bottom, the rows of the box above the bar's top edge, and
// what the glass body underneath already tints.
fn shade(s: Shader) -> vec4<f32> {
 let want = mix(s.a.x, s.a.y, clamp((s.pos.y - s.a.z) / 32.0, 0.0, 1.0));
 return vec4<f32>(s.color.rgb, (want - s.a.w) / (1.0 - s.a.w));
}
