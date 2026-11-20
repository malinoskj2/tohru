//! Compile the real production shader against upstream Pleamar's eight-value ABI.
#[test]
fn production_shader_parses_and_validates_with_eight_inputs() {
    let prelude = "struct Shader { pos: vec2<f32>, size: vec2<f32>, uv: vec2<f32>, time: f32, scale: f32, pointer: vec2<f32>, hovered: f32, a: vec4<f32>, b: vec4<f32>, color: vec4<f32>, color2: vec4<f32>, origin: vec2<f32>, layer: i32, };\n";
    let source = format!("{prelude}{}", include_str!("../../shell/bar/ribbon.wgsl"));
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("real ribbon shader validates against the upstream ABI");
    assert!(!source.contains("s.c.") && !source.contains("s.d."));
    assert!(!source.contains("80.0"));
}

#[test]
fn ribbon_takes_its_track_ink_and_bead_presence_from_its_inputs() {
    let source = include_str!("../../shell/bar/ribbon.wgsl");
    assert!(source.contains("let track=s.color2.rgb;"));
    assert!(source.contains("select(0.0,1.0,s.a.z>=0.0)"));
    assert!(!source.contains("vec3<f32>(0.14*rail)") && !source.contains("s.color2.r;"));
    for scene in [
        include_str!("../../shell/bar/workspace.plm"),
        include_str!("../../shell/calendar/rail.plm"),
        include_str!("../../shell/sound/panel.plm"),
    ] {
        let call = scene
            .lines()
            .find(|line| line.contains("shader rail_shader"))
            .expect("every rail draws the shared ribbon");
        assert!(!call.contains("#000000"), "presence rides in the bead's sign, not a black ink: {call}");
    }
}
