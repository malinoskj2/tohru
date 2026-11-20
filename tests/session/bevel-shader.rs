//! Compile the notification toasts' capillary bevel against upstream Pleamar's eight-value ABI,
//! and check where the light falls by evaluating `shade` on the CPU.
const PRELUDE: &str = concat!(
    "struct Shader { pos: vec2<f32>, size: vec2<f32>, uv: vec2<f32>, time: f32, scale: f32, ",
    "pointer: vec2<f32>, hovered: f32, a: vec4<f32>, b: vec4<f32>, color: vec4<f32>, ",
    "color2: vec4<f32>, origin: vec2<f32>, layer: i32, };\n",
);
const SOURCE: &str = include_str!("../../shell/notifications/bevel.wgsl");

#[test]
fn capillary_bevel_parses_and_validates() {
    let source = format!("{PRELUDE}{SOURCE}");
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("capillary bevel validates against the upstream ABI");
    assert!(!SOURCE.contains("s.c.") && !SOURCE.contains("s.d.") && !SOURCE.contains("s.time"));
}

/// Mirrors the shader's alpha for the capillary (not critical) branch, so the path order is pinned.
fn capillary_alpha(p: (f32, f32), size: (f32, f32), left: f32) -> f32 {
    let r = 15.0_f32;
    let (w, h) = size;
    let edge = h - r;
    let total = edge + std::f32::consts::FRAC_PI_2 * r + (w - r);
    let (at, inset) = if p.0 < r && p.1 > edge {
        let d = (p.0 - r, p.1 - edge);
        (edge + r * d.1.max(0.0).atan2((-d.0).max(0.0001)), r - (d.0 * d.0 + d.1 * d.1).sqrt())
    } else if p.0 < h - p.1 {
        (p.1, p.0)
    } else {
        (edge + std::f32::consts::FRAC_PI_2 * r + (p.0 - r), h - p.1)
    };
    let near = (-((inset.max(0.0) - 1.2) / 0.85).powi(2)).exp() + (-inset.max(0.0) / 5.0).exp() * 0.32;
    let head = (1.0 - left) * total;
    let t = ((at - (head - 1.5)) / 7.5).clamp(0.0, 1.0);
    near * t * t * (3.0 - 2.0 * t)
}

#[test]
fn the_light_drains_down_the_left_edge_then_along_the_bottom() {
    for needle in [
        "let head = (1.0 - left) * total;",
        "let near = exp(-pow((inset - 1.2) / 0.85, 2.0)) + exp(-inset / 5.0) * 0.32;",
        "let lit = smoothstep(head - 1.5, head + 6.0, at.x)",
        "return vec2<f32>(p.y, p.x);",
    ] {
        assert!(SOURCE.contains(needle), "the mirror is out of date: {needle}");
    }
    let size = (374.0, 104.0);
    let (top_left, low_left, bottom_mid, bottom_right) = ((1.2, 10.0), (1.2, 80.0), (187.0, 102.8), (360.0, 102.8));
    let full = |p| capillary_alpha(p, size, 1.0);
    assert!(full(top_left) > 0.8 && full(bottom_mid) > 0.8 && full(bottom_right) > 0.8);
    // A tenth gone: the top of the left edge is dark, its foot still lit.
    let early = |p| capillary_alpha(p, size, 0.9);
    assert!(early(top_left) < 0.05 && early(low_left) > 0.8 && early(bottom_mid) > 0.8);
    // A quarter gone: the left edge is spent and the bottom still lit.
    let quarter = |p| capillary_alpha(p, size, 0.75);
    assert!(quarter(low_left) < 0.05 && quarter(bottom_mid) > 0.8);
    // Most of it gone: only the end of the bottom bevel is left.
    let late = |p| capillary_alpha(p, size, 0.1);
    assert!(late(low_left) < 0.05 && late(bottom_mid) < 0.05 && late(bottom_right) > 0.8);
    // Nothing on the top edge or the right edge, ever.
    assert!(full((187.0, 1.2)) < 0.01 && full((372.8, 50.0)) < 0.01);
}
