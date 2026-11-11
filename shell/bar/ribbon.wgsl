// s.a: bead x, bead velocity, bead half-height + 8 * round(rounding * 255), wave crest.
// s.b: swell, wash, wash centre, direction * (1 + time since the switch / wave life).
// The bead's extent and glow, the wash's width and the chop's clock are derived from these.
fn sq(x: f32) -> f32 { return x*x; }
fn cap(s: Shader, x: f32) -> f32 {
 let m = min(x, (s.size.x-24.0)-x);
 if m >= 2.0 { return 1.0; }
 return sqrt(max(0.0, 1.0-sq(1.0-m/2.0)));
}
fn rounding(s: Shader) -> f32 { return floor(s.a.z/8.0)/255.0; }
fn half(s: Shader) -> f32 { return s.a.z-8.0*floor(s.a.z/8.0); }
fn phase(s: Shader) -> f32 { return abs(s.b.w)-1.0; }
fn wave(s: Shader, x: f32) -> vec2<f32> {
 let i = clamp(floor(x+0.5),0.0,s.size.x-24.0);
 let dir = sign(s.b.w);
 let skew = min(phase(s), 1.0);
 // j2bar's wash widens by (18 + 10 * gain) px/s and its chop turns at 7 rad/s: at a typical
 // gain and life, 15 px and 3.4 rad per wave life.
 let wash_w = 4.0+15.0*phase(s);
 let u = (i-s.a.w)*dir;
 let front = 9.0-6.0*skew; let back = 12.0+12.0*skew;
 var shape = exp(-sq(u/front));
 if u <= 0.0 { shape = exp(-sq(u/back))-0.22*exp(-sq((u+back*1.3)/back)); }
 let env = sqrt(max(0.0,sin(3.141592653589793*i/(s.size.x-24.0))));
 let chop = 0.12*sin(i*0.23+3.4*phase(s))*s.b.x;
 let offset = env*(-s.b.x*shape+chop);
 let thick = env*(s.b.x*0.3*max(0.0,shape)+s.b.y*exp(-sq((i-s.b.z)/max(3.0,wash_w))));
 return vec2<f32>(offset, thick);
}
// Left and right ends, then the reach behind and ahead: the bead stretches behind as it speeds up.
fn span(s: Shader) -> vec4<f32> {
 let sp = abs(s.a.y);
 let front = 8.0+min(2.0, sp*0.01);
 let back = 8.0+min(9.0, sp*0.045);
 if s.a.y >= 0.0 { return vec4<f32>(s.a.x-back, s.a.x+front, back, front); }
 return vec4<f32>(s.a.x-front, s.a.x+back, front, back);
}
fn bead(s: Shader, e: vec4<f32>, xx: f32) -> vec2<f32> {
 let r = rounding(s);
 let resting = r < 0.02 && abs(s.a.y) < 1.0;
 if resting { return vec2<f32>(wave(s,s.a.x).x,half(s)*cap(s,xx)); }
 var u = (xx-s.a.x)/e.w;
 if xx < s.a.x { u = (xx-s.a.x)/e.z; }
 let ex = 40.0+(2.2-40.0)*r;
 let h = half(s)*pow(max(0.0,1.0-pow(min(1.0,abs(u)),ex)),1.0/ex)*cap(s,xx);
 return vec2<f32>(wave(s,xx).x,h);
}
fn over(top: vec4<f32>, bottom: vec4<f32>) -> vec4<f32> {
 return top+bottom*(1.0-top.a);
}
fn row_above(a: f32,b: f32) -> f32 {
 let lo=min(a,a+b); let hi=max(a,a+b);
 if lo>=1.0 { return 1.0; } if hi<=0.0 { return 0.0; }
 if abs(b)<0.001 { return clamp(a+0.5*b,0.0,1.0); }
 var g0=a-0.5; if a<=0.0 { g0=0.0; } else if a<1.0 { g0=0.5*a*a; }
 let c=a+b;
 var g1=c-0.5; if c<=0.0 { g1=0.0; } else if c<1.0 { g1=0.5*c*c; }
 return (g1-g0)/b;
}
fn erf_approx(input: f32) -> f32 {
 let x=abs(input); let t=1.0/(1.0+0.3275911*x);
 let y=1.0-(((((1.061405429*t-1.453152027)*t)+1.421413741)*t-0.284496736)*t+0.254829592)*t*exp(-x*x);
 return sign(input)*y;
}
fn phi(z: f32) -> f32 { return 0.5+0.5*erf_approx(z*0.70710678); }
fn shade(s: Shader) -> vec4<f32> {
 let p=s.pos-vec2<f32>(12.0,21.0);
 let column=floor(p.x); let row=floor(p.y);
 let color=s.color.xyz;
 let w0=wave(s,column); let w1=wave(s,column+1.0);
 let h0=max(0.4,2.0+w0.y)*cap(s,column);
 let h1=max(0.4,2.0+w1.y)*cap(s,column+1.0);
 var rail=0.0;
 if column>=0.0 && column<s.size.x-24.0 {
  rail=row_above(w0.x+h0-row,(w1.x+h1)-(w0.x+h0))-row_above(w0.x-h0-row,(w1.x-h1)-(w0.x-h0));
 }
 let tx=floor(clamp(p.x,0.0,(s.size.x-24.0)-0.001)/4.0)*4.0;
 let t0=wave(s,tx); let t1=wave(s,tx+4.0);
 let tint=mix(min(0.55,abs(t0.y)*0.35+abs(t0.x)*0.25),min(0.55,abs(t1.y)*0.35+abs(t1.x)*0.25),(clamp(p.x,0.0,s.size.x-24.0)-tx)/4.0);
 var c=vec4<f32>(vec3<f32>(0.14*rail),0.14*rail);
 c=over(vec4<f32>(color*tint*rail,tint*rail),c);
 let e=span(s);
 let left=e.x; let step=(e.y-left)/28.0;
 let sigma=(9.0+min(5.0, abs(s.a.y)*0.03))*0.5;
 let wide=sqrt(sigma*sigma+1.0/12.0);
 let tight=sqrt(1.75*1.75+1.0/12.0);
 var edge0=vec2<f32>(phi((p.x-left)/wide),phi((p.x-left)/tight));
 var cover=0.0; var glow=vec2<f32>(0.0);
 var q0=bead(s,e,left);
 for (var k=0; k<28; k+=1) {
  let x0=left+step*f32(k); let x1=x0+step;
  let q1=bead(s,e,x1);
  let edge1=vec2<f32>(phi((p.x-x1)/wide),phi((p.x-x1)/tight));
  let lo=max(x0,column); let hi=min(x1,column+1.0);
  if hi>lo {
   let qa=mix(q0,q1,(lo-x0)/step); let qb=mix(q0,q1,(hi-x0)/step);
   let bottom=row_above(qa.x+qa.y-row,(qb.x+qb.y)-(qa.x+qa.y));
   let top=row_above(qa.x-qa.y-row,(qb.x-qb.y)-(qa.x-qa.y));
   cover+=(hi-lo)*(bottom-top);
  }
  let q=(q0+q1)*0.5;
  let top=p.y-(q.x-q.y); let bottom=p.y-(q.x+q.y);
  let across=vec2<f32>(phi(top/wide)-phi(bottom/wide),phi(top/tight)-phi(bottom/tight));
  glow+=(edge0-edge1)*across;
  edge0=edge1; q0=q1;
 }
 let ink=vec4<f32>(color,1.0)*s.color2.r;
 c=over(ink*0.75*glow.x,c);
 c=over(ink*cover,c);
 c=over(ink*0.9*glow.y,c);
 c=over(ink*cover,c);
 return vec4<f32>(c.rgb/max(c.a,0.00001),c.a);
}
