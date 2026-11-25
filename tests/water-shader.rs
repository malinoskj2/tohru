//! Compile the water shared by the Bluetooth popup and others against upstream Pleamar's eight-value
//! ABI, and check that the Bluetooth tank hands it all eight values.
#[test]
fn shared_water_shader_parses_and_validates() {
    let prelude = concat!(
        "struct Shader { pos: vec2<f32>, size: vec2<f32>, uv: vec2<f32>, time: f32, scale: f32, ",
        "pointer: vec2<f32>, hovered: f32, a: vec4<f32>, b: vec4<f32>, color: vec4<f32>, ",
        "color2: vec4<f32>, origin: vec2<f32>, layer: i32, };\n",
        "fn inside(s: Shader, p: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0, 0.0, 0.0, s.uv.x * p.x); }\n",
    );
    let source = format!("{prelude}{}", include_str!("../shell/water.wgsl"));
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("shared water shader validates against the upstream ABI");
    assert!(!source.contains("s.c.") && !source.contains("s.d."));
    assert!(
        !source.contains("behind("),
        "a group's shader cannot read behind the surface"
    );
}

#[test]
fn bluetooth_tank_hands_the_shared_water_eight_values() {
    for (scene, name) in [(
        include_str!("../shell/bluetooth/tank.plm"),
        "bt_water_shader",
    )] {
        assert!(
            scene.contains(&format!("shader {name} = file \"../water.wgsl\"")),
            "{name} is the shared water"
        );
        let use_line = scene
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("shader: {name},")))
            .unwrap_or_else(|| panic!("{name} is applied to a group"));
        let values = use_line.split_once(',').unwrap().1;
        let mut depth = 0;
        let commas = values
            .chars()
            .filter(|c| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                *c == ',' && depth == 0
            })
            .count();
        assert_eq!(
            commas + 1,
            8,
            "{name} takes fold, waterline, slosh, width, numeral width, pk, level + 2 quiet, clock"
        );
    }
    assert!(
        !std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../shell/network/water.wgsl"
        ))
        .exists()
    );
    assert!(
        !std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../shell/network/water.plm"
        ))
        .exists()
    );
}
