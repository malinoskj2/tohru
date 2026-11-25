//! Compile the wallpaper popup's shaders against upstream Pleamar's eight-value ABI.
fn validate(name: &str, body: &str) {
    let prelude = concat!(
        "struct Shader { pos: vec2<f32>, size: vec2<f32>, uv: vec2<f32>, time: f32, scale: f32, ",
        "pointer: vec2<f32>, hovered: f32, a: vec4<f32>, b: vec4<f32>, color: vec4<f32>, ",
        "color2: vec4<f32>, origin: vec2<f32>, layer: i32, };\n",
        "fn inside(s: Shader, p: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0, 0.0, 0.0, s.uv.x * p.x); }\n",
    );
    let source = format!("{prelude}{body}");
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(&source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{name} does not validate: {error:?}"));
    assert!(
        module
            .functions
            .iter()
            .any(|(_, f)| f.name.as_deref() == Some("shade")),
        "{name} has no shade"
    );
    assert!(
        !body.contains("@group") && !body.contains("@binding") && !body.contains("var<"),
        "{name} declares bindings"
    );
}

#[test]
fn meniscus_shader_parses_and_validates() {
    validate(
        "meniscus.wgsl",
        include_str!("../../shell/wallpaper/meniscus.wgsl"),
    );
}

#[test]
fn tile_scrim_shader_parses_and_validates() {
    validate(
        "scrim.wgsl",
        include_str!("../../shell/wallpaper/scrim.wgsl"),
    );
}

// The sound tank and fill are reused as they are; the wallpaper tank feeds them its own values.
#[test]
fn reused_sound_shaders_still_validate() {
    validate("tank.wgsl", include_str!("../../shell/sound/tank.wgsl"));
    validate("fill.wgsl", include_str!("../../shell/sound/fill.wgsl"));
}
