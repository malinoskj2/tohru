// Analytic reconstruction of the reference pixel-column profile.
float sq(float a) { return a*a; }
vec2 profile(float x) {
    x = clamp(x,0.0,80.0);
    float env = sqrt(max(0.0,sin(3.141592653589793*x/80.0)));
    float skew = min(1.0,values[1].x/values[2].w);
    float u=(x-values[1].z)*values[1].y;
    float front=9.0-6.0*skew, back=12.0+12.0*skew;
    float shape=u>0.0 ? exp(-sq(u/front)) : exp(-sq(u/back))-0.22*exp(-sq((u+back*1.3)/back));
    float amp=values[1].w;
    float o=env*(-amp*shape+0.12*sin(x*0.23+values[1].x*7.0+values[3].x)*amp);
    float d=env*(amp*0.3*max(0.0,shape)+values[2].x*exp(-sq((x-values[2].y)/max(3.0,values[2].z))));
    return vec2(o,d);
}
vec4 ribbon(float x) {
    vec2 p=profile(x);
    float m=min(x,80.0-x);
    float cap=m>=2.0 ? 1.0 : sqrt(max(0.0,1.0-sq(1.0-m/2.0)));
    float halfh=max(0.4,2.0+p.y)*cap;
    return vec4(p.x-halfh,p.x+halfh,min(0.55,abs(p.y)*0.35+abs(p.x)*0.25),0.0);
}
vec2 bead(float k) {
    float xx=mix(values[3].y,values[3].z,k/28.0);
    float x=values[0].x, hh=values[0].y, rounding=values[0].z;
    if(rounding<0.02 && abs(values[0].w)<1.0) return vec2(profile(floor(x+0.5)).x,hh);
    float u=(xx-x)/(xx<x ? values[4].x : values[4].y);
    float ex=mix(40.0,2.2,clamp(rounding,0.0,1.0));
    float h=hh*pow(max(0.0,1.0-pow(min(1.0,abs(u)),ex)),1.0/ex);
    return vec2(profile(floor(xx+0.5)).x,h);
}
const float BEAD_SEGMENTS = 28.0;
const float TIGHT_SIGMA = 1.75;

// ∫₀¹ clamp(a + b·t, 0, 1) dt: how much of a pixel row lies above an edge that crosses the
// pixel's width from a to a + b below the row's top.
float row_above(float a, float b) {
    float lo = min(a, a + b);
    float hi = max(a, a + b);
    if (lo >= 1.0) return 1.0;
    if (hi <= 0.0) return 0.0;
    if (abs(b) < 1e-3) return clamp(a + 0.5 * b, 0.0, 1.0);
    // Antiderivative of clamp(u, 0, 1).
    float g0 = a <= 0.0 ? 0.0 : (a < 1.0 ? 0.5 * a * a : a - 0.5);
    float c = a + b;
    float g1 = c <= 0.0 ? 0.0 : (c < 1.0 ? 0.5 * c * c : c - 0.5);
    return (g1 - g0) / b;
}

float erf_(float x) {
    // Abramowitz and Stegun 7.1.26.
    float s = sign(x);
    x = abs(x);
    float t = 1.0 / (1.0 + 0.3275911 * x);
    float y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
        + 0.254829592) * t * exp(-x * x);
    return s * y;
}

float phi(float z) {
    return 0.5 + 0.5 * erf_(z * 0.70710678);
}

vec4 over(vec4 src, vec4 dst) {
    return src + dst * (1.0 - src.a);
}

void main() {
    vec2 p = vec2((uv * vec2(104.0,42.0)).x - 12.0, (uv * vec2(104.0,42.0)).y - 21.0);
    float column = floor(p.x);
    float row = floor(p.y);

    vec4 a = vec4(0.0);
    vec4 b = vec4(0.0);
    vec4 stop0 = vec4(0.0);
    vec4 stop1 = vec4(0.0);
    float stop = floor(p.x / 4.0) * 4.0;
    for (int i = 0; i < 81; i++) {
        float fi = float(i);
        if (fi == column) a = ribbon(float(i));
        if (fi == column + 1.0) b = ribbon(float(i));
        if (fi == stop) stop0 = ribbon(float(i));
        if (fi == stop + 4.0) stop1 = ribbon(float(i));
    }
    float ribbon = 0.0;
    if (column >= 0.0 && column < 80.0) {
        ribbon = row_above(a.y - row, b.y - a.y) - row_above(a.x - row, b.x - a.x);
    }
    float tint = mix(stop0.z, stop1.z, clamp((p.x - stop) / 4.0, 0.0, 1.0));
    vec3 ink = values[5].rgb;

    vec4 frag = vec4(vec4(1.0,1.0,1.0,0.14).rgb * vec4(1.0,1.0,1.0,0.14).a, vec4(1.0,1.0,1.0,0.14).a) * ribbon;
    frag = over(vec4(ink, 1.0) * tint * ribbon, frag);

    if (true) {
        float left = values[3].yz.x;
        float step = (values[3].yz.y - left) / BEAD_SEGMENTS;
        // The canvas blurs the bead's pixel coverage, which adds a pixel's box filter to each blur.
        float wide = sqrt(values[3].w * values[3].w + 1.0 / 12.0);
        float tight = sqrt(TIGHT_SIGMA * TIGHT_SIGMA + 1.0 / 12.0);
        vec2 edge0 = vec2(phi((p.x - left) / wide), phi((p.x - left) / tight));
        float cover = 0.0;
        vec2 glow = vec2(0.0);
        for (int k = 0; k < 29 - 1; k++) {
            float x0 = left + step * float(k);
            float x1 = x0 + step;
            vec2 q0 = bead(float(k));
            vec2 q1 = bead(float(k + 1));
            vec2 edge1 = vec2(phi((p.x - x1) / wide), phi((p.x - x1) / tight));
            float lo = max(x0, column);
            float hi = min(x1, column + 1.0);
            if (hi > lo) {
                vec2 qa = mix(q0, q1, (lo - x0) / step);
                vec2 qb = mix(q0, q1, (hi - x0) / step);
                float bottom = row_above(qa.x + qa.y - row, (qb.x + qb.y) - (qa.x + qa.y));
                float top = row_above(qa.x - qa.y - row, (qb.x - qb.y) - (qa.x - qa.y));
                cover += (hi - lo) * (bottom - top);
            }
            vec2 q = (q0 + q1) * 0.5;
            float top = p.y - (q.x - q.y);
            float bottom = p.y - (q.x + q.y);
            vec2 across = vec2(phi(top / wide) - phi(bottom / wide),
                phi(top / tight) - phi(bottom / tight));
            glow += (edge0 - edge1) * across;
            edge0 = edge1;
        }
        vec4 bead = vec4(ink, 1.0);
        frag = over(bead * 0.75 * glow.x, frag);
        frag = over(bead * cover, frag);
        frag = over(bead * 0.9 * glow.y, frag);
        frag = over(bead * cover, frag);
    }
    color = frag.a > 0.0 ? vec4(frag.rgb / frag.a, frag.a) : vec4(0.0);
}
