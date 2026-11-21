//! Compile the media popup lens shader against upstream Pleamar's eight-value ABI.
#[test]
fn media_lens_shader_parses_and_validates() {
    let prelude = concat!(
        "struct Shader { pos: vec2<f32>, size: vec2<f32>, uv: vec2<f32>, time: f32, scale: f32, ",
        "pointer: vec2<f32>, hovered: f32, a: vec4<f32>, b: vec4<f32>, color: vec4<f32>, ",
        "color2: vec4<f32>, origin: vec2<f32>, layer: i32, };\n",
        "fn inside(s: Shader, p: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0, 0.0, 0.0, s.uv.x * p.x); }\n",
    );
    let source = format!(
        "{prelude}{}",
        include_str!("../../shell/media/lens.wgsl")
    );
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("media lens shader validates against the upstream ABI");
    assert!(!source.contains("s.c.") && !source.contains("s.d."));
}
