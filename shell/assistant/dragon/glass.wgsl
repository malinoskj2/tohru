// Oklab, so the hue turns without the lightness moving.
fn to_lab(c: vec3<f32>) -> vec3<f32> {
    let l = pow(0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b, 1.0 / 3.0);
    let m = pow(0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b, 1.0 / 3.0);
    let s = pow(0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b, 1.0 / 3.0);
    return vec3<f32>(0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
                     1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
                     0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s);
}

fn to_rgb(c: vec3<f32>) -> vec3<f32> {
    let l = pow(c.x + 0.3963377774 * c.y + 0.2158037573 * c.z, 3.0);
    let m = pow(c.x - 0.1055613458 * c.y - 0.0638541728 * c.z, 3.0);
    let s = pow(c.x - 0.0894841775 * c.y - 1.2914855480 * c.z, 3.0);
    return vec3<f32>(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
                     -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
                     -0.0041960863 * l - 0.7034186168 * m + 1.7076147010 * s);
}

// s.a.x: how much of it (0 gives the sheet back as drawn). A 7 degree hue wave crosses the body every 7 s;
// a soft highlight band crosses it in 1.8 s, every 9 s.
fn shade(s: Shader) -> vec4<f32> {
    let c = inside(s, s.pos);
    if (c.a <= 0.001 || s.a.x <= 0.0) { return c; }
    let k = s.uv * 640.0;
    var lab = to_lab(pow(max(c.rgb, vec3<f32>(0.0)), vec3<f32>(2.2)));
    let th = radians(7.0) * s.a.x * sin(6.2831853 * ((k.x * 0.8 + k.y) / 760.0 - s.time / 7.0));
    let ab = vec2<f32>(lab.y * cos(th) - lab.z * sin(th), lab.y * sin(th) + lab.z * cos(th));
    lab = vec3<f32>(lab.x, ab);
    let cyc = (s.time % 9.0) / 1.8;
    if (cyc < 1.0) {
        let d = ((k.x * 0.6 + k.y * 0.8) - (-150.0 + cyc * 1100.0)) / 45.0;
        lab.x = lab.x + 0.07 * s.a.x * exp(-d * d) * sin(3.14159265 * cyc) * step(0.3, c.a);
    }
    return vec4<f32>(pow(clamp(to_rgb(lab), vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2)), c.a);
}
