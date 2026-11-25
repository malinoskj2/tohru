//! Every lock style's shaders compile against upstream Pleamar's eight-value ABI, and the plain
//! fallback the locker drops to when a style fails declares none that could fail the same way.
use std::path::Path;

const PRELUDE: &str = concat!(
    "struct Shader { pos: vec2<f32>, size: vec2<f32>, uv: vec2<f32>, time: f32, scale: f32, ",
    "pointer: vec2<f32>, hovered: f32, a: vec4<f32>, b: vec4<f32>, color: vec4<f32>, ",
    "color2: vec4<f32>, origin: vec2<f32>, layer: i32, };\n",
    "fn inside(s: Shader, p: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0, 0.0, 0.0, s.uv.x * p.x); }\n",
    "fn behind(s: Shader, p: vec2<f32>) -> vec4<f32> { return vec4<f32>(s.uv.y * p.y); }\n",
);

fn lock_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shell/lock"))
}

fn validate(path: &Path) {
    let code = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let source = format!("{PRELUDE}{code}");
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{}:\n{}", path.display(), error.emit_to_string(&source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{} does not validate: {e:?}", path.display()));
    assert!(
        !code.contains("s.c.") && !code.contains("s.d."),
        "{} reads past the ABI",
        path.display()
    );
}

/// The shader files a scene file declares, resolved against that file's folder.
fn declared_shaders(plm: &Path) -> Vec<std::path::PathBuf> {
    let text = std::fs::read_to_string(plm).unwrap();
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("shader ")?;
            let file = rest.split_once("= file \"")?.1.split('"').next()?;
            Some(plm.parent().unwrap().join(file))
        })
        .collect()
}

fn includes(plm: &Path) -> Vec<std::path::PathBuf> {
    let text = std::fs::read_to_string(plm).unwrap();
    text.lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("include \"")?
                .split('"')
                .next()
                .map(|f| plm.parent().unwrap().join(f))
        })
        .collect()
}

#[test]
fn every_lock_style_shader_parses_and_validates() {
    let styled = lock_dir().join("lock.plm");
    let mut shaders = Vec::new();
    for part in includes(&styled) {
        shaders.extend(declared_shaders(&part));
    }
    let names: Vec<String> = shaders
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for expected in [
        "tank.wgsl",
        "glass.wgsl",
        "veil.wgsl",
        "ribbon.wgsl",
        "fill.wgsl",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "lock.plm no longer declares {expected}: {names:?}"
        );
    }
    for shader in &shaders {
        validate(shader);
    }
}

#[test]
fn every_style_has_its_part_and_the_plain_fallback_has_no_shaders() {
    let styled = lock_dir().join("lock.plm");
    let parts = includes(&styled);
    for slug in tohru_shell::settings::LockStyle::ALL
        .iter()
        .map(|s| s.name())
    {
        let part = lock_dir().join("styles").join(format!("{slug}.plm"));
        assert!(
            parts.contains(&part),
            "lock.plm does not include {}",
            part.display()
        );
        let text = std::fs::read_to_string(&part).unwrap();
        let index = tohru_shell::settings::LockStyle::ALL
            .iter()
            .position(|s| s.name() == slug)
            .unwrap()
            + 1;
        assert!(
            text.contains(&format!("show: style == {index}")),
            "{slug} is not shown for style {index}"
        );
    }
    let plain = lock_dir().join("plain.plm");
    let mut files = vec![plain.clone()];
    files.extend(includes(&plain));
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            !text.contains(".wgsl") && !text.lines().any(|l| l.trim_start().starts_with("shader")),
            "{} brings a shader into the fallback",
            file.display()
        );
        assert!(
            !text.contains("include \"styles/common.plm\""),
            "{} pulls the styled shaders in",
            file.display()
        );
    }
}
