//! Compile the real production shader against the reduced, pinned ABI proposal.
#[test]
fn production_shader_parses_and_validates_with_sixteen_shape_inputs() {
    let prelude = "struct Shader { pos: vec2<f32>, size: vec2<f32>, uv: vec2<f32>, time: f32, scale: f32, pointer: vec2<f32>, hovered: f32, a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32>, color: vec4<f32>, color2: vec4<f32>, origin: vec2<f32>, layer: i32, };\n";
    let source = format!(
        "{prelude}{}",
        include_str!("../../shell/bar/ribbon.wgsl")
    );
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("real ribbon shader validates against the proposed native ABI");
    assert!(!source.contains("s.e"));
    assert!(!source.contains("80.0"));
}
