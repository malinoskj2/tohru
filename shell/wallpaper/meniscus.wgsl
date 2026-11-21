// A clear meniscus that falls from the bar to the bottom edge. Above it the new wallpaper (mpvpaper,
// under this surface) shows as it is; below it the group's picture of the old one stays, bent and
// brightened along the waterline. Without an old picture it is the waterline alone, over a light veil.
// a: progress 0..1, whether the old picture is drawn, clock (0 reads s.time), unused.
const LAV = vec3<f32>(0.7059, 0.7451, 0.9961);

fn stop(i: i32) -> vec3<f32> {
    switch (i) {
        case 0: { return vec3<f32>(250.0, 179.0, 135.0) / 255.0; }
        case 1: { return vec3<f32>(249.0, 226.0, 175.0) / 255.0; }
        case 2: { return vec3<f32>(166.0, 227.0, 161.0) / 255.0; }
        case 3: { return vec3<f32>(180.0, 190.0, 254.0) / 255.0; }
        default: { return vec3<f32>(203.0, 166.0, 247.0) / 255.0; }
    }
}

// Where the surface stands at x: it starts above the top edge and leaves past the bottom one, eased so
// it leaves the bar gently and lands softly, with a swell that is tallest mid-screen.
fn surface_y(x: f32, k: f32, size: vec2<f32>, t: f32) -> f32 {
    let e = k * k * (3.0 - 2.0 * k);
    let swell = sin(3.14159 * k);
    let base = -60.0 + e * (size.y + 120.0);
    let wave = 7.0 * sin(x * 0.0061 + t * 2.1 + 0.8)
             + 3.5 * sin(x * 0.0173 - t * 3.2 + 2.0)
             + 2.0 * sin(x * 0.041 + t * 4.6);
    // The middle runs a little ahead of the sides, as water poured from the bar would.
    let lead = 26.0 * swell * (1.0 - pow(abs(x / max(size.x, 1.0) - 0.5) * 2.0, 2.0));
    return base + lead + wave * (0.35 + swell);
}

fn shade(s: Shader) -> vec4<f32> {
    let k = clamp(s.a.x, 0.0, 1.0);
    let old = s.a.y;
    let t = select(s.time, s.a.z, s.a.z > 0.0);
    let p = s.pos;
    if (k >= 1.0) { return vec4<f32>(0.0); }
    let ys = surface_y(p.x, k, s.size, t);
    let depth = p.y - ys;

    // Above the surface is the new wallpaper: only the lip of the water, clear, fading in 14 px.
    if (depth < 0.0) {
        let lip = exp(depth / 5.0) * 0.16 + exp(depth / 22.0) * 0.05;
        return vec4<f32>(vec3<f32>(1.0), lip * (1.0 - smoothstep(0.92, 1.0, k)));
    }

    // Below it the old picture, bent up towards the surface in the first 40 px as through a lens.
    let band = exp(-depth / 16.0);
    let bend = vec2<f32>(2.5 * sin(p.y * 0.05 + t * 2.0), -band * 18.0);
    var base = vec4<f32>(0.0);
    if (old > 0.5) {
        base = inside(s, p + bend);
        base = vec4<f32>(base.rgb, 1.0);
    } else {
        base = vec4<f32>(mix(vec3<f32>(0.08, 0.06, 0.12), LAV, 0.25), 0.10 * (1.0 - smoothstep(0.85, 1.0, k)));
    }

    // The palette's light caught in the waterline, drifting along it.
    let hue = fract(p.x / max(s.size.x, 1.0) * 1.3 + t * 0.06) * 5.0;
    let i = i32(floor(hue));
    let tint = mix(stop(i % 5), stop((i + 1) % 5), fract(hue));

    var rgb = base.rgb;
    var a = base.a;
    // Brighter just under the surface, as light focused by the curve of it.
    rgb = mix(rgb, mix(vec3<f32>(1.0), tint, 0.35), band * 0.22);
    a = max(a, band * 0.30);
    // The surface itself: a bright hairline over a darker one, which is what reads as a meniscus.
    let line = exp(-depth * depth / (2.0 * 0.9 * 0.9));
    let under = exp(-pow(depth - 3.5, 2.0) / (2.0 * 1.6 * 1.6));
    rgb = mix(rgb, vec3<f32>(0.03, 0.02, 0.06), under * 0.28 * old);
    rgb = mix(rgb, mix(vec3<f32>(1.0), tint, 0.25), line * 0.85);
    a = max(a, line * 0.9);
    return vec4<f32>(rgb, clamp(a, 0.0, 1.0));
}
