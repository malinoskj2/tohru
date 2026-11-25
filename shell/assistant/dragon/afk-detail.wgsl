// Native-resolution closeup part articulation; a = art kind, action, phase, fog.
fn local_part(uv: vec2<f32>, center: vec2<f32>, radius: vec2<f32>) -> f32 {
    let p=(uv-center)/radius;
    return exp(-dot(p,p));
}
fn shade(s: Shader) -> vec4<f32> {
    let uv = s.pos / max(s.size,vec2<f32>(1.0));
    var delta = vec2<f32>(0.0);
    if (s.a.y > 0.5 && s.a.y < 1.5 && s.a.x < 1.5) {
        let span = max(1.0-smoothstep(0.14,0.40,uv.x),smoothstep(0.63,0.82,uv.x));
        let wing = exp(-pow((uv.y-mix(0.50,0.76,s.a.x))/0.075,2.0));
        delta.y = 0.05 * span * wing * sin(s.a.z*6.2831853);
    }
    if (s.a.y > 1.5 && s.a.x < 1.5) {
        let center = mix(vec2<f32>(0.53,0.27),vec2<f32>(0.50,0.40),s.a.x);
        let face = exp(-pow((uv.x-center.x)/0.25,4.0)-pow((uv.y-center.y)/0.22,4.0));
        delta.x = 0.007 * sin(s.a.z*6.2831853) * face;
        delta.y = 0.004 * sin(s.a.z*3.14159265) * face;
        if (s.a.y > 2.5) {
            let paw = exp(-pow((uv.x-0.30)/0.13,2.0)-pow((uv.y-0.87)/0.12,2.0));
            delta.x = delta.x + 0.016 * sin(s.a.z*6.2831853) * paw;
        }
    }
    if (s.a.x > 1.5) {
        let pulse=sin(s.a.z*6.2831853);
        if (s.a.y > 3.5 && s.a.y < 4.5) {
            let paw=local_part(uv,vec2<f32>(.66,.49),vec2<f32>(.15,.09));
            delta=vec2<f32>(.035*pulse,.012*sin(s.a.z*6.2831853+.5))*paw;
            delta.y=delta.y+.009*pulse*local_part(uv,vec2<f32>(.79,.51),vec2<f32>(.12,.10));
        } else if (s.a.y > 4.5 && s.a.y < 5.5) {
            delta=vec2<f32>(.017,.025)*pulse*local_part(uv,vec2<f32>(.59,.23),vec2<f32>(.095,.12));
        } else if (s.a.y > 5.5 && s.a.y < 6.5) {
            delta=vec2<f32>(.035*pulse,.015*(1.0-cos(s.a.z*6.2831853)))*local_part(uv,vec2<f32>(.43,.45),vec2<f32>(.09,.13));
        } else if (s.a.y > 6.5 && s.a.y < 7.5) {
            delta.y=.006*pulse*local_part(uv,vec2<f32>(.48,.55),vec2<f32>(.23,.12));
            delta.x=.004*sin(s.a.z*12.5663706)*local_part(uv,vec2<f32>(.67,.42),vec2<f32>(.16,.10));
        } else if (s.a.y > 7.5 && s.a.y < 8.5) {
            delta.x=.018*pulse*local_part(uv,vec2<f32>(.54,.72),vec2<f32>(.10,.25));
        } else if (s.a.y > 8.5 && s.a.y < 10.5) {
            let cx=select(.78,.24,s.a.y>9.5);
            delta=vec2<f32>(.028*pulse,.022*cos(s.a.z*6.2831853))*local_part(uv,vec2<f32>(cx,.60),vec2<f32>(.18,.22));
            delta.y=delta.y+.010*pulse*local_part(uv,vec2<f32>(.50,.27),vec2<f32>(.23,.18));
        } else if (s.a.y > 10.5) {
            delta.x=.008*pulse*local_part(uv,vec2<f32>(.48,.30),vec2<f32>(.23,.20));
        }
    }
    let c = inside(s,s.pos-delta*s.size);
    let fog = 0.16 * max(0.0,s.a.w) * exp(-pow((uv.x-0.50)/0.13,2.0)-pow((uv.y-0.73)/0.055,2.0));
    return vec4<f32>(mix(c.rgb,vec3<f32>(0.91,0.96,1.0),fog),max(c.a,fog));
}
