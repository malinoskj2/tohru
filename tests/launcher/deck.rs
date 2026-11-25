//! The launcher deck's shaders and the parts of its scene that only work in one place.
use std::path::{Path, PathBuf};

const PRELUDE: &str = concat!(
    "struct Shader { pos: vec2<f32>, size: vec2<f32>, uv: vec2<f32>, time: f32, scale: f32, ",
    "pointer: vec2<f32>, hovered: f32, a: vec4<f32>, b: vec4<f32>, color: vec4<f32>, ",
    "color2: vec4<f32>, origin: vec2<f32>, layer: i32, };\n",
    "fn inside(s: Shader, p: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0, 0.0, 0.0, s.uv.x * p.x); }\n",
);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

// Every `shader name = file "…"` the scene and its parts declare, resolved from the file that declares it.
fn declared_shaders() -> Vec<PathBuf> {
    let launcher = root().join("shell/launcher");
    let mut files = vec![launcher.join("entry.plm")];
    let mut shaders = Vec::new();
    while let Some(file) = files.pop() {
        let text = read(&file);
        let folder = file.parent().unwrap().to_path_buf();
        for line in text.lines().map(str::trim) {
            let quoted = |line: &str| line.split('"').nth(1).map(|path| folder.join(path));
            if line.starts_with("include ") {
                files.extend(quoted(line));
            } else if line.starts_with("shader ") && line.contains("= file ") {
                shaders.extend(quoted(line));
            }
        }
    }
    shaders
}

#[test]
fn every_deck_shader_parses_and_validates_with_eight_inputs() {
    let shaders = declared_shaders();
    let names: Vec<_> = shaders
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for expected in ["ribbon.wgsl", "water.wgsl", "tile.wgsl", "reveal.wgsl"] {
        assert!(
            names.iter().any(|name| name == expected),
            "the deck declares {expected}: {names:?}"
        );
    }
    for path in shaders {
        let source = format!("{PRELUDE}{}", read(&path));
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|error| {
            panic!("{}: {}", path.display(), error.emit_to_string(&source))
        });
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{}: {error:?}", path.display()));
        assert!(
            !source.contains("s.c.") && !source.contains("s.d."),
            "{} reads past the eight values",
            path.display()
        );
    }
}

// The launcher's own surface is a 1 px bootstrap that is never shown, and loose rules belong to its
// copies, so the rail's and the ink's rules only run inside the layer that shows them.
#[test]
fn rules_that_must_run_live_inside_the_launcher_layer() {
    let scene = read(&root().join("shell/launcher/entry.plm"));
    let layer = scene
        .find("surface launcher_layer {")
        .expect("the launcher layer");
    for include in [
        "include \"../panel-ink.plm\"",
        "include \"../bar/rail-rules.plm\"",
    ] {
        let at = scene
            .find(include)
            .unwrap_or_else(|| panic!("the scene has {include}"));
        assert!(at > layer, "{include} is inside launcher_layer");
    }
    let springs = scene
        .find("include \"../bar/rail-springs.plm\"")
        .expect("the rail's springs");
    assert!(
        springs < layer,
        "the rail's arrays are declared at the scene's level"
    );
    assert!(
        scene.contains("bright: panel_bright"),
        "the deck's glass reads how bright the desktop behind it is"
    );
    assert!(
        read(&root().join("shell/bar/rail-state.plm")).contains("include \"rail-rules.plm\""),
        "the bar still runs its rails' rules"
    );
}

// The sidebar's rail shares the bar's driver, which Luau can only require from the scene's own folder.
#[test]
fn the_launcher_rail_is_the_bar_rail() {
    let launcher = root().join("shell/launcher/rail.luau");
    let target = std::fs::read_link(&launcher).expect("shell/launcher/rail.luau is a link");
    assert_eq!(target, Path::new("../bar/rail.luau"));
    assert_eq!(read(&launcher), read(&root().join("shell/bar/rail.luau")));
}
