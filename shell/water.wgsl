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

fn hash(n: f32) -> f32 { return fract(sin(n * 127.1 + 3.7) * 43758.5453); }

fn h2(p: vec2<f32>) -> f32 {
    var h = bitcast<u32>(vec2<i32>(p).x) * 1597334677u ^ bitcast<u32>(vec2<i32>(p).y) * 3812015801u;
    h = h * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return f32(h) / 4294967295.0;
}

fn vn(p: vec2<f32>) -> f32 {
    let i = floor(p);
    var f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(h2(i), h2(i + vec2<f32>(1.0, 0.0)), f.x),
        mix(h2(i + vec2<f32>(0.0, 1.0)), h2(i + vec2<f32>(1.0, 1.0)), f.x),
        f.y
    );
}

fn dens(p: vec2<f32>, t: f32) -> f32 {
    var q = p / vec2<f32>(98.0, 72.0) + vec2<f32>(t * 0.085, -t * 0.05);
    q += 0.7 * vec2<f32>(vn(q * 1.3 + 7.0 + t * 0.06), vn(q * 1.3 - 3.0 - t * 0.05));
    return 0.68 * vn(q) + 0.32 * vn(q * 2.1 + 11.0);
}

fn wave(x: f32, t: f32) -> f32 {
    return 1.35 * sin(x * 0.045 + t * 1.15 + 1.0)
         + 0.65 * sin(x * 0.115 - t * 1.8 + 2.3)
         + 0.70 * sin(x * 0.021 + t * 0.55);
}

// The group holds the numeral over a 1 % sheet that only gives the group its box.
fn digit(s: Shader, at: vec2<f32>, pad: f32) -> f32 {
    return max(0.0, (inside(s, at + vec2<f32>(pad)).a - 0.01) / 0.99);
}

fn over(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(acc.rgb * (1.0 - a) + c * a, acc.a * (1.0 - a) + a);
}

fn add_plume(acc: vec4<f32>, c: vec3<f32>, a: f32, pk: f32) -> vec4<f32> {
    let dark_res = vec4<f32>(acc.rgb + c * a, min(1.0, acc.a + a * 0.55));
    let c_light = mix(c, vec3<f32>(0.1098, 0.0941, 0.1490), 0.30);
    let light_res = over(acc, c_light, a * 0.75);
    return mix(dark_res, light_res, pk);
}

// s.a: fold (0 the tank, 1 the unfolded panel), waterline y in px, slosh, the panel's width.
// s.b: the numeral's width, pk, level (0..1) + 2 * quiet (0 or 1), a fixed clock above 0.
// Folded, water fills the panel below the line and refracts the numeral the group holds; unfolded, light
// plumes drift behind the rows, and a line still on the panel is a horizon with frosted water under it.
fn shade(s: Shader) -> vec4<f32> {
    let pad = max(0.0, (s.size.x - s.a.w) * 0.5);
    let p = s.pos - vec2<f32>(pad);
    let size = s.size - vec2<f32>(2.0 * pad);
    let t = select(s.time, s.b.w, s.b.w > 0.0);
    let dpr = s.scale;
    let m = s.a.x;
    let yb = s.a.y;
    let slosh = s.a.z;
    let wn = s.b.x;
    let pk = clamp(s.b.y, 0.0, 1.0);
    let quiet = clamp(floor(s.b.z * 0.5), 0.0, 1.0);
    let lv = clamp(s.b.z - 2.0 * quiet, 0.0, 1.0);
    let W = size.x;
    let H = size.y;
    let mc = clamp(m, 0.0, 1.0);
    let ink = mix(vec3<f32>(1.0), vec3<f32>(0.1098, 0.0941, 0.1490), pk);

    var acc = vec4<f32>(0.0);

    let compact_a = 1.0 - smoothstep(0.05, 0.75, mc);
    let digit_a = 1.0 - smoothstep(0.0, 0.45, mc);
    let unfold_a = smoothstep(0.20, 0.90, mc);
    let g = clamp((lv - 0.08) / 0.84, 0.0, 1.0);
    let ga = (1.0 - quiet);

    let wv = wave(p.x, t) * slosh;
    let ys = yb + wv;
    let depth = p.y - ys;
    let dz = max(depth, 0.0);
    let wet = clamp(depth * dpr + 0.5, 0.0, 1.0) * mix(1.0, 0.55, 1.0 - compact_a);

    var frost = 0.0;
    if (wet > 0.0) {
        let lo = mix(0.60, 0.10, g);
        frost = smoothstep(lo, lo + 0.46, dens(p, t)) * (0.3 + 0.7 * g) * 0.50 * smoothstep(0.0, 14.0, dz);
    }

    // Refract and frost-blur the stationary thin numeral beneath the waterline.
    if (digit_a > 0.0) {
        var dp = p;
        if (depth > 0.0) {
            dp += compact_a * vec2<f32>(
                1.7 + 1.1 * sin(p.y * 0.23 + t * 1.4) + 0.4 * sin(p.x * 0.3 - t * 2.0),
                -2.2 - min(depth, 40.0) * 0.05
            );
        }
        let rad = frost * wet * 3.2;
        if (dp.x > 10.0 - rad && dp.x < 24.0 + max(wn, 64.0) + rad) {
            var dA = digit(s, dp, pad);
            if (rad > 0.15) {
                var sum = dA * 1.5;
                let spin = fract(p.y * 0.618) * 1.0472;
                for (var k = 0; k < 6; k++) {
                    let a = f32(k) * 1.0472 + spin;
                    sum += digit(s, dp + rad * 0.7 * vec2<f32>(cos(a), sin(a)), pad);
                }
                dA = sum / 7.5;
            }
            acc = over(acc, ink, dA * digit_a * mix(0.94, 0.38, quiet) * mix(1.0, mix(0.76, 0.38, frost), wet));
        }
    }

    // Clear, partly frosted water.
    let deep = clamp(depth / 100.0, 0.0, 1.0);
    acc = over(acc, LAV, wet * (0.04 + 0.06 * deep));
    acc = over(acc, mix(vec3<f32>(0.95, 0.93, 1.0), ink, pk), wet * frost * 0.24);
    acc = over(acc, ink, wet * 0.10 * exp(-dz / 5.0));

    // The unfolded panel's drifting clear-to-frosted backdrop.
    let u_frost = smoothstep(0.16, 0.64, dens(p, t)) * 0.42 * unfold_a;
    if (unfold_a > 0.001) {
        acc = over(acc, LAV, unfold_a * mix(0.04, 0.025, pk));
        acc = over(acc, mix(vec3<f32>(0.95, 0.93, 1.0), ink, pk), u_frost * mix(0.20, 0.12, pk));
    }

    // Light in the workspace ribbon's palette: under the water, and behind the unfolded rows.
    var tintA = 0.0;
    var tintC = vec3<f32>(0.0);
    if (ga > 0.0) {
        for (var k = 0; k < 5; k++) {
            let fk = f32(k);
            let ph = t / 3.7 + fk * 0.2;
            let n = floor(ph);
            let sn = sin(3.14159 * (ph - n));
            let e = sn * sn;
            let gx = W * (0.12 + 0.76 * hash(n * 5.3 + fk * 17.3));
            let gc = stop(i32(n + fk * 2.0) % 5);
            let dx = p.x - gx;

            if (wet > 0.0) {
                let gd = 14.0 + max(H - yb - 28.0, 0.0) * hash(n * 3.7 + fk * 9.1 + 2.0);
                let dd = depth - gd;
                let blob = exp(-(dx * dx / (2.0 * 44.0 * 44.0) + dd * dd / (2.0 * 26.0 * 26.0)));
                let sw = 11.0 + dz * 0.12;
                let shaft = exp(-pow(dx - dz * 0.18, 2.0) / (2.0 * sw * sw)) * exp(-dz / max(gd * 1.3, 8.0)) * 0.5;
                acc = add_plume(acc, gc, wet * ga * e * (blob * (0.5 + 0.9 * frost) + shaft * 0.7) * 0.16, pk);
            }
            if (unfold_a > 0.001) {
                let gy = H * (0.20 + 0.66 * hash(n * 3.7 + fk * 9.1 + 2.0));
                let dy = p.y - gy;
                let ublob = exp(-(dx * dx / (2.0 * 54.0 * 54.0) + dy * dy / (2.0 * 38.0 * 38.0)));
                acc = add_plume(acc, gc, unfold_a * ga * e * ublob * (0.55 + 0.85 * u_frost) * 0.15, pk);
            }
            let la = e * exp(-dx * dx / (2.0 * 30.0 * 30.0));
            tintC += gc * la;
            tintA += la;
        }
    }

    // The meniscus, tinted by passing light. An empty tank draws none; a horizon always does.
    let line_a = max(compact_a * step(0.01, lv), 1.0 - compact_a);
    if (line_a > 0.001) {
        let d_line = abs(p.y - ys);
        let la = clamp(0.85 - d_line * dpr, 0.0, 1.0) * line_a;
        var lc = ink;
        if (tintA > 0.0) {
            lc = mix(lc, tintC / max(tintA, 0.001), clamp(tintA, 0.0, 1.0) * 0.45 * ga);
        }
        let glow = line_a * 0.16 * exp(-d_line / 2.8) * (0.6 + 0.4 * clamp(tintA, 0.0, 1.0) * ga) * mix(1.0, 0.45, quiet);
        acc = add_plume(acc, lc, glow, pk);
        acc = over(acc, lc, la * mix(mix(0.86, 0.38, quiet), 0.62, pk));
    }

    // Clip strictly to the panel's 15 px rounded rectangle.
    let r = 15.0;
    let q = abs(p - size * 0.5) - size * 0.5 + vec2<f32>(r);
    let edge = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    acc *= clamp(0.5 - edge * dpr, 0.0, 1.0);

    let a = max(acc.a, max(acc.r, max(acc.g, acc.b)));
    if (a <= 0.0001) { return vec4<f32>(0.0); }
    return vec4<f32>(acc.rgb / a, min(a, 1.0));
}
