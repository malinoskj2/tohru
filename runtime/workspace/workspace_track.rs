#![deny(unsafe_code)]
use std::time::Instant;

use super::workspace_motion::{MAX_FRAME, Rng, SETTLE_ENERGY, STEP};

/// Width of one workspace's slot along the ribbon.
pub(crate) const SLOT: f32 = 16.0;
/// Height of the row the ribbon is centred in, which also bounds each slot's click target.
pub(crate) const ROW: f32 = 22.0;
/// The drawing box reaches this far past each end of the ribbon, so the glow has room.
pub(crate) const PAD: f32 = 12.0;
/// The drawing box's height; it starts `(HEIGHT - ROW) / 2` above the row.
pub(crate) const HEIGHT: f32 = 42.0;
/// The ribbon's centre line, from the top of the drawing box.
pub(crate) const CENTER: f32 = 21.0;
/// The ribbon never shrinks below this many slots, and grows to the highest workspace id up to
/// `MAX_SLOTS`.
pub(crate) const MIN_SLOTS: usize = 5;
pub(crate) const MAX_SLOTS: usize = 10;
/// The calendar's month rail: the ribbon stretched to a slot per month.
pub(crate) const RAIL_SLOTS: usize = 12;
/// One ribbon vertex per pixel column boundary.
#[cfg(test)]
pub(crate) const MAX_VERTICES: usize = RAIL_SLOTS * SLOT as usize + 1;
/// Segments of the moving bead's outline.
#[cfg(test)]
const BEAD_SEGMENTS: usize = 28;
#[cfg(test)]
pub(crate) const BEAD_POINTS: usize = BEAD_SEGMENTS + 1;

/// The bead's colour at each slot, blended in between; slots past the fifth cycle through it.
const PALETTE: [[f64; 3]; 5] = [
    [250.0, 179.0, 135.0],
    [249.0, 226.0, 175.0],
    [166.0, 227.0, 161.0],
    [180.0, 190.0, 254.0],
    [203.0, 166.0, 247.0],
];

/// Below `SETTLE_ENERGY`, with the bead this close to its slot, the animation settles.
const SETTLE_DISTANCE: f64 = 0.05;
const REST_HALF_HEIGHT: f64 = 2.0;

/// The wave a switch throws along the ribbon.
struct Wave {
    t: f64,
    dir: f64,
    /// The crest's position and velocity.
    c: f64,
    v: f64,
    amp: f64,
    av: f64,
    /// The swell's height as it breaks, its centre and its spread.
    wash: f64,
    wash_c: f64,
    wash_w: f64,
    gain: f64,
    reach: f64,
    life: f64,
    phase: f64,
}

/// Everything `Gpu::track` needs for one frame, in drawing-box pixels with x from the ribbon's
/// start and y from its centre line.
#[cfg(test)]
pub(crate) struct Frame {
    /// The ribbon's straight-alpha ink, white on the bar and adapted inside panels.
    pub(crate) rail: [f32; 4],
    /// The ribbon's length.
    pub(crate) width: f32,
    /// Per vertex: the ribbon's top and bottom edge, and the tint's alpha at that vertex (used at
    /// every fourth, where the gradient has its stops).
    pub(crate) ribbon: [[f32; 4]; MAX_VERTICES],
    /// The bead's colour, 0-255; `None` without a focused slot.
    pub(crate) bead: Option<Bead>,
}

#[cfg(test)]
pub(crate) struct Bead {
    pub(crate) color: [f32; 3],
    /// The outline spans `left..right`, sampled at `BEAD_POINTS` evenly spaced points, each with
    /// its centre's offset from the ribbon's centre line and its half height.
    pub(crate) left: f32,
    pub(crate) right: f32,
    pub(crate) points: [[f32; 2]; BEAD_POINTS],
    /// The wide glow's Gaussian standard deviation, growing with speed.
    pub(crate) glow_sigma: f32,
}

pub(crate) struct Track {
    slots: usize,
    /// The most slots it grows to.
    most: usize,
    /// The bead takes the palette across the whole ribbon (`RampRail`), not slot by slot.
    ramp: bool,
    /// The focused slot, 1-based.
    active: Option<usize>,
    /// Per vertex: the swell thickening the ribbon, and its vertical offset. The design keeps them
    /// in `Float32Array`s.
    d: Vec<f32>,
    o: Vec<f32>,
    x: f64,
    vx: f64,
    /// The bead's half height and its rounding toward an ellipse, with their velocities.
    hh: f64,
    hv: f64,
    round: f64,
    rv: f64,
    stiffness: f64,
    damping: f64,
    wave: Wave,
    rng: Rng,
    /// While animating: when the simulation was last advanced, and the time it still owes.
    clock: Option<(Instant, f64)>,
}

impl Default for Track {
    fn default() -> Self {
        let slots = MIN_SLOTS;
        let x = 8.0;
        Self {
            slots,
            most: MAX_SLOTS,
            ramp: false,
            active: None,
            d: vec![0.0; vertices(slots)],
            o: vec![0.0; vertices(slots)],
            x,
            vx: 0.0,
            hh: REST_HALF_HEIGHT,
            hv: 0.0,
            round: 0.0,
            rv: 0.0,
            stiffness: 170.0,
            damping: 2.0 * 170f64.sqrt() * 0.5,
            wave: Wave {
                t: 0.0,
                dir: 1.0,
                c: x,
                v: 0.0,
                amp: 0.0,
                av: 0.0,
                wash: 0.0,
                wash_c: x,
                wash_w: 4.0,
                gain: 1.0,
                reach: 12.0,
                life: 0.5,
                phase: 0.0,
            },
            rng: Rng(0x0123_4567_89ab_cdef),
            clock: None,
        }
    }
}

fn vertices(slots: usize) -> usize {
    slots * SLOT as usize + 1
}

fn sq(v: f64) -> f64 {
    v * v
}

/// `Math.round`: halves round up, also below zero.
fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// The slot count for workspaces up to id `highest`.
pub(crate) fn slots_for(highest: Option<i64>) -> usize {
    let highest = highest.map_or(0, |id| usize::try_from(id).unwrap_or(0));
    highest.clamp(MIN_SLOTS, MAX_SLOTS)
}

impl Track {
    /// A rail of `slots`, its bead coloured through the palette by where it is along it.
    pub(crate) fn rail(slots: usize) -> Self {
        let slots = slots.clamp(1, RAIL_SLOTS);
        Self {
            slots,
            most: slots,
            ramp: true,
            d: vec![0.0; vertices(slots)],
            o: vec![0.0; vertices(slots)],
            ..Self::default()
        }
    }

    pub(crate) fn width(&self) -> f64 {
        (self.slots * SLOT as usize) as f64
    }

    fn target(&self) -> f64 {
        self.active
            .map_or(self.x, |a| (a - 1) as f64 * f64::from(SLOT) + 8.0)
    }

    /// Whether the bead is still moving; the caller keeps drawing frames until it is not.
    pub(crate) fn animating(&self) -> bool {
        self.clock.is_some()
    }

    /// The bead's velocity along the ribbon in px/s, which sloshes the mark's liquid.
    pub(crate) fn has_active(&self) -> bool {
        self.active.is_some()
    }

    /// Follows the model: `slots` along the ribbon, the bead on `active` (1-based). A bead that
    /// appears lands in place; one that moves springs there from wherever it is.
    pub(crate) fn sync(&mut self, slots: usize, active: Option<usize>, now: Instant) {
        let slots = if self.ramp {
            self.slots
        } else {
            slots.clamp(MIN_SLOTS, self.most)
        };
        if slots != self.slots {
            self.slots = slots;
            self.d = vec![0.0; vertices(slots)];
            self.o = vec![0.0; vertices(slots)];
        }
        let active = active.filter(|a| (1..=slots).contains(a));
        if active == self.active {
            return;
        }
        let from = self.active;
        self.active = active;
        match (from, active) {
            (Some(_), Some(_)) => self.go(now),
            (None, Some(_)) => {
                self.settle();
                self.clock = None;
            }
            (_, None) => self.clock = None,
        }
    }

    fn go(&mut self, now: Instant) {
        self.stiffness = 140.0 + self.rng.unit() * 70.0;
        self.damping = 2.0 * self.stiffness.sqrt() * (0.4 + self.rng.unit() * 0.2);
        let tx = self.target();
        let dir = if tx > self.x { 1.0 } else { -1.0 };
        let gain = self.rng.range(0.8, 1.25) * (0.7 + (tx - self.x).abs() / 40.0).min(1.3);
        let reach = self.rng.range(9.0, 16.0);
        let life = self.rng.range(0.38, 0.6);
        #[expect(
            clippy::approx_constant,
            reason = "the design draws the phase below 6.28, not a full turn"
        )]
        let phase = self.rng.range(0.0, 6.28);
        let w = &mut self.wave;
        w.t = 0.0;
        w.dir = dir;
        w.c = self.x - dir * 5.0;
        w.v = 0.0;
        w.wash_w = 4.0;
        w.wash_c = self.x;
        w.gain = gain;
        w.reach = reach;
        w.life = life;
        w.phase = phase;
        if self.clock.is_none() {
            self.clock = Some((now, 0.0));
        }
    }

    /// Runs the simulation up to `now` in fixed steps, settling it once it has come to rest.
    pub(crate) fn advance(&mut self, now: Instant) {
        let Some((last, owed)) = self.clock else {
            return;
        };
        let mut owed = owed
            + now
                .saturating_duration_since(last)
                .as_secs_f64()
                .min(MAX_FRAME);
        let mut energy = 1.0;
        while owed >= STEP {
            energy = self.step(STEP);
            owed -= STEP;
        }
        if energy < SETTLE_ENERGY && (self.x - self.target()).abs() < SETTLE_DISTANCE {
            self.settle();
            self.clock = None;
        } else {
            self.clock = Some((now, owed));
        }
    }

    fn settle(&mut self) {
        self.x = self.target();
        self.vx = 0.0;
        self.hh = REST_HALF_HEIGHT;
        self.round = 0.0;
        self.d.fill(0.0);
        self.o.fill(0.0);
        self.wave.amp = 0.0;
        self.wave.av = 0.0;
        self.wave.wash = 0.0;
    }

    /// One step of `dt`; returns the energy left in the springs and the wave.
    fn step(&mut self, dt: f64) -> f64 {
        let tx = self.target();
        let accel = self.stiffness * (tx - self.x) - self.damping * self.vx;
        self.vx += accel * dt;
        self.x += self.vx * dt;
        let sp = self.vx.abs();
        let th = 2.0 + (sp * 0.02).min(3.0);
        self.hv += (420.0 * (th - self.hh) - 2.0 * 420f64.sqrt() * 0.4 * self.hv) * dt;
        self.hh += self.hv * dt;
        let tr = (sp / 50.0).min(1.0);
        self.rv += (260.0 * (tr - self.round) - 2.0 * 260f64.sqrt() * 0.55 * self.rv) * dt;
        self.round += self.rv * dt;

        let width = self.width();
        let w = &mut self.wave;
        w.t += dt;
        let dir = w.dir;
        let skew = (w.t / w.life).min(1.0);
        // The crest rides just ahead of the bead, then outruns it and breaks forward.
        let crest = self.x + dir * (3.0 + w.reach * skew);
        w.v += (70.0 * (crest - w.c) - 11.0 * w.v) * dt;
        w.c += w.v * dt;
        // The swell builds with speed and collapses as the bead lands.
        let early = if w.t < w.life { 1.0 } else { 0.0 };
        let amp = w.gain * ((sp * 0.032).min(3.0) + 0.9 * (1.0 - skew) * early);
        let prev = w.amp;
        w.av += (220.0 * (amp - w.amp) - 2.0 * 220f64.sqrt() * 0.8 * w.av) * dt;
        w.amp += w.av * dt;
        let lost = (prev - w.amp).max(0.0);
        w.wash += lost * 1.3 * (0.4 + skew) - w.wash * 4.2 * dt;
        w.wash_c += (w.c + dir * 4.0 - w.wash_c) * 6.0 * dt;
        w.wash_w += (18.0 + 10.0 * w.gain) * dt;
        let (front, back) = (9.0 - 6.0 * skew, 12.0 + 12.0 * skew);
        for (i, (d, o)) in self.d.iter_mut().zip(&mut self.o).enumerate() {
            let i = i as f64;
            let env = (std::f64::consts::PI * i / width).sin().sqrt();
            let u = (i - w.c) * dir;
            let shape = if u > 0.0 {
                (-sq(u / front)).exp()
            } else {
                (-sq(u / back)).exp() - 0.22 * (-sq((u + back * 1.3) / back)).exp()
            };
            let chop = 0.12 * (i * 0.23 + w.t * 7.0 + w.phase).sin() * w.amp;
            *o = (env * (-w.amp * shape + chop)) as f32;
            *d = (env
                * (w.amp * 0.3 * shape.max(0.0)
                    + w.wash * (-sq((i - w.wash_c) / w.wash_w.max(3.0))).exp()))
                as f32;
        }
        sq(self.vx) * 1e-3
            + sq(self.hv) * 1e-2
            + (self.hh - REST_HALF_HEIGHT).abs()
            + self.round.abs() * 0.1
            + sq(w.amp)
            + sq(w.av) * 1e-3
            + sq(w.wash)
    }

    /// The bead's colour at its current position, blended between the slots it is between.
    fn color(&self) -> [f64; 3] {
        if self.ramp {
            let last = (PALETTE.len() - 1) as f64;
            let p = (self.x / self.width() * last).clamp(0.0, last);
            let i = p.floor().min(last - 1.0);
            let f = p - i;
            let (a, b) = (PALETTE[i as usize], PALETTE[i as usize + 1]);
            return std::array::from_fn(|k| js_round(a[k] + (b[k] - a[k]) * f));
        }
        let last = (self.slots - 1) as f64;
        let p = ((self.x - 8.0) / f64::from(SLOT)).clamp(0.0, last);
        let i = p.floor().min(last - 1.0);
        let f = p - i;
        let i = i as usize;
        let (a, b) = (PALETTE[i % PALETTE.len()], PALETTE[(i + 1) % PALETTE.len()]);
        std::array::from_fn(|k| js_round(a[k] + (b[k] - a[k]) * f))
    }

    /// The bead's extent `(left, right, behind, ahead)`: it stretches behind as it speeds up.
    fn span(&self) -> (f64, f64, f64, f64) {
        let sp = self.vx.abs();
        let front = 8.0 + (sp * 0.01).min(2.0);
        let back = 8.0 + (sp * 0.045).min(9.0);
        if self.vx >= 0.0 {
            (self.x - back, self.x + front, back, front)
        } else {
            (self.x - front, self.x + back, front, back)
        }
    }

    /// How much of the ribbon's thickness is left at `x`: the ends taper to a point over their
    /// last two pixels, and nothing is left past them.
    #[cfg(test)]
    fn cap(&self, x: f64) -> f64 {
        let m = x.min(self.width() - x);
        if m >= 2.0 {
            1.0
        } else {
            (1.0 - sq(1.0 - m / 2.0)).max(0.0).sqrt()
        }
    }

    #[cfg(test)]
    fn offset_at(&self, x: f64) -> f64 {
        let i = js_round(x).clamp(0.0, self.width()) as usize;
        f64::from(self.o[i])
    }

    #[cfg(test)]
    pub(crate) fn frame(&self) -> Frame {
        let width = self.width();
        let last = self.slots * SLOT as usize;
        let mut ribbon = [[0.0; 4]; MAX_VERTICES];
        for (i, vertex) in ribbon.iter_mut().enumerate().take(last + 1) {
            let (d, o) = (f64::from(self.d[i]), f64::from(self.o[i]));
            let half = (2.0 + d).max(0.4) * self.cap(i as f64);
            let tint = (d.abs() * 0.35 + o.abs() * 0.25).min(0.55);
            *vertex = [(o - half) as f32, (o + half) as f32, tint as f32, 0.0];
        }
        Frame {
            rail: [1.0, 1.0, 1.0, 0.14],
            width: width as f32,
            ribbon,
            bead: self.active.map(|_| self.bead()),
        }
    }

    #[cfg(test)]
    fn bead(&self) -> Bead {
        let sp = self.vx.abs();
        let (left, right, behind, ahead) = self.span();
        let resting = self.round < 0.02 && sp < 1.0;
        // A superellipse whose exponent falls from nearly square toward an ellipse with speed.
        let ex = 40.0 + (2.2 - 40.0) * self.round.clamp(0.0, 1.0);
        let mut points = [[0.0; 2]; BEAD_POINTS];
        for (k, point) in points.iter_mut().enumerate() {
            let xx = left + (right - left) * k as f64 / BEAD_SEGMENTS as f64;
            let (y, h) = if resting {
                (self.offset_at(self.x), self.hh)
            } else {
                let u = if xx < self.x {
                    (xx - self.x) / behind
                } else {
                    (xx - self.x) / ahead
                };
                let h = self.hh * (1.0 - u.abs().min(1.0).powf(ex)).max(0.0).powf(1.0 / ex);
                (self.offset_at(xx), h)
            };
            // Unlike the design's, the bead takes the ribbon's rounded ends rather than squaring
            // them off on the first and last slots.
            *point = [
                y as f32,
                (h * if resting { 1.0 } else { self.cap(xx) }) as f32,
            ];
        }
        Bead {
            color: self.color().map(|c| c as f32),
            left: left as f32,
            right: right as f32,
            points,
            glow_sigma: self.glow_sigma(),
        }
    }

    /// The wide glow's Gaussian standard deviation, growing with the bead's speed.
    fn glow_sigma(&self) -> f32 {
        ((9.0 + (self.vx.abs() * 0.03).min(5.0)) * 0.5) as f32
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const FRAME: Duration = Duration::from_micros(16_667);

    /// Draws frames every 16.7ms until the bead settles, returning how many it took.
    fn run(track: &mut Track, mut now: Instant) -> usize {
        for frame in 1..=600 {
            now += FRAME;
            track.advance(now);
            if !track.animating() {
                return frame;
            }
        }
        panic!("the bead never settled");
    }

    #[test]
    fn a_bead_that_appears_lands_in_place() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.sync(5, Some(4), t0);
        assert!(!track.animating());
        assert_eq!(track.x, 56.0);
        let bead = track.frame().bead.expect("a focused slot");
        assert_eq!((bead.left, bead.right), (48.0, 64.0));
        assert_eq!(bead.color, [180.0, 190.0, 254.0]);
        assert!(bead.points.iter().all(|&p| p == [0.0, 2.0]));
    }

    #[test]
    fn a_bead_on_an_end_slot_is_square_at_rest() {
        for (slot, end) in [(1, 0), (5, BEAD_SEGMENTS)] {
            let mut track = Track::default();
            track.sync(5, Some(slot), Instant::now());
            let bead = track.frame().bead.expect("a focused slot");
            assert_eq!(bead.points[end], [0.0, 2.0], "slot {slot}");
            assert_eq!(bead.points[end.abs_diff(1)][1], 2.0, "slot {slot}");
            assert_eq!(bead.points[BEAD_SEGMENTS / 2], [0.0, 2.0], "slot {slot}");
        }
    }

    #[test]
    fn a_switch_springs_to_the_new_slot_and_settles_exactly_there() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.sync(5, Some(1), t0);
        track.sync(5, Some(5), t0);
        assert!(track.animating());
        track.advance(t0 + FRAME * 6);
        assert!(track.x > 8.0 && track.x < 72.0, "mid-flight at {}", track.x);
        assert!(
            track.o.iter().any(|o| *o != 0.0),
            "the switch throws a wave along the ribbon"
        );
        let frames = run(&mut track, t0 + FRAME * 6);
        assert!(frames < 180, "settled after {frames} more frames");
        assert_eq!(track.x, 72.0);
        assert!(track.o.iter().chain(&track.d).all(|v| *v == 0.0));
        let bead = track.frame().bead.expect("a focused slot");
        assert_eq!(bead.color, [203.0, 166.0, 247.0]);
    }

    #[test]
    fn the_ribbon_tapers_to_a_point_at_both_ends() {
        let track = Track::default();
        let frame = track.frame();
        assert_eq!(frame.width, 80.0);
        assert_eq!(frame.ribbon[0][..2], [0.0, 0.0]);
        assert_eq!(frame.ribbon[80][..2], [0.0, 0.0]);
        let shoulder = (0.75f64.sqrt() * 2.0) as f32;
        assert_eq!(frame.ribbon[1][..2], [-shoulder, shoulder]);
        assert_eq!(frame.ribbon[2][..2], [-2.0, 2.0]);
        assert_eq!(frame.ribbon[81], [0.0; 4], "past the last slot");
    }

    #[test]
    fn the_ribbon_grows_to_the_highest_workspace_up_to_ten_slots() {
        assert_eq!(slots_for(None), 5);
        assert_eq!(slots_for(Some(3)), 5);
        assert_eq!(slots_for(Some(7)), 7);
        assert_eq!(slots_for(Some(42)), 10);
        let mut track = Track::default();
        track.sync(7, Some(7), Instant::now());
        assert_eq!(track.frame().width, 112.0);
        let bead = track.frame().bead.expect("slot 7 exists");
        assert_eq!(bead.color, PALETTE[1].map(|c| c as f32), "colours cycle");
        track.sync(7, Some(11), Instant::now());
        assert!(track.frame().bead.is_none(), "no slot, no bead");
    }

    #[test]
    fn a_switch_mid_flight_redirects_the_running_animation() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.sync(5, Some(1), t0);
        track.sync(5, Some(5), t0);
        track.advance(t0 + FRAME * 5);
        let clock = track.clock;
        track.sync(5, Some(2), t0 + FRAME * 5);
        assert_eq!(track.clock, clock, "the running clock carries on");
        run(&mut track, t0 + FRAME * 5);
        assert_eq!(track.x, 24.0);
    }

    #[test]
    fn a_late_frame_advances_at_most_50ms() {
        let t0 = Instant::now();
        let mut track = Track::default();
        track.sync(5, Some(1), t0);
        track.sync(5, Some(5), t0);
        let mut late = Track::default();
        late.sync(5, Some(1), t0);
        late.sync(5, Some(5), t0);
        track.advance(t0 + Duration::from_millis(50));
        late.advance(t0 + Duration::from_secs(3));
        assert_eq!(track.x, late.x);
    }
}

impl Track {
    pub(crate) fn shader_values(&self) -> [f32; 20] {
        let w = &self.wave;
        let (l, r, _, _) = self.span();
        let color = self.color();
        [
            self.x as f32,
            self.vx as f32,
            self.hh as f32,
            self.round as f32,
            w.c as f32,
            w.amp as f32,
            w.wash as f32,
            w.wash_c as f32,
            w.wash_w as f32,
            w.t as f32,
            w.dir as f32,
            w.life as f32,
            w.phase as f32,
            l as f32,
            r as f32,
            self.glow_sigma(),
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
            0.0,
        ]
    }
}

/// Canonical replay: event timestamps are compared against the 240 Hz clock,
/// independent of how often Luau requests a frame. Rest does no simulation work.
#[cfg(test)]
pub(crate) struct Replay {
    pub(crate) track: Track,
    ticks: u64,
    event: usize,
    active: bool,
}

#[cfg(test)]
const EVENTS: [(u64, usize); 10] = [
    (0, 1),
    (1000, 2),
    (2200, 3),
    (3400, 4),
    (4600, 5),
    (5800, 1),
    (7000, 5),
    (7120, 2),
    (7240, 4),
    (9000, 1),
];

#[cfg(test)]
impl Replay {
    pub(crate) fn new(active: bool) -> Self {
        let mut track = Track::default();
        track.sync(5, Some(1), Instant::now());
        Self {
            track,
            ticks: 0,
            event: 1,
            active,
        }
    }

    pub(crate) fn advance_ms(&mut self, ms: f64) {
        let until = ((if self.active { ms } else { ms.min(12000.0) }) * 0.24).floor() as u64;
        while self.ticks < until {
            self.ticks += 1;
            let elapsed = self.ticks as f64 * 1000.0 / 240.0;
            let next = if self.active {
                Some((self.event as u64 * 300, [1, 5, 2, 4, 3][self.event % 5]))
            } else {
                EVENTS.get(self.event).copied()
            };
            if let Some((at, target)) = next {
                if elapsed >= at as f64 {
                    self.track.sync(5, Some(target), Instant::now());
                    self.event += 1;
                }
            }
            if self.track.animating() {
                let energy = self.track.step(STEP);
                if energy < SETTLE_ENERGY
                    && (self.track.x - self.track.target()).abs() < SETTLE_DISTANCE
                {
                    self.track.settle();
                    self.track.clock = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod replay_tests {
    use super::*;

    #[test]
    fn replay_is_independent_of_render_cadence_and_finishes_flat() {
        let mut a = Replay::new(false);
        let mut b = Replay::new(false);
        for ms in (0..=12000).step_by(10) {
            a.advance_ms(f64::from(ms));
        }
        for ms in (0..=12000).step_by(25) {
            b.advance_ms(f64::from(ms));
        }
        assert_eq!(a.track.shader_values(), b.track.shader_values());
        assert_eq!(a.track.active, Some(1));
        assert!(!a.track.animating());
        assert!(
            a.track
                .frame()
                .bead
                .unwrap()
                .points
                .iter()
                .all(|p| *p == [0.0, 2.0])
        );
    }

    #[test]
    fn interruption_preserves_momentum_and_produces_finite_wave() {
        let mut replay = Replay::new(false);
        replay.advance_ms(7116.0);
        let old = replay.track.vx;
        replay.advance_ms(7125.0);
        assert_eq!(replay.track.active, Some(2));
        assert!(old.abs() > 1.0);
        replay.advance_ms(7300.0);
        assert_eq!(replay.track.active, Some(4));
        assert!(replay.track.shader_values().iter().all(|v| v.is_finite()));
        assert!(replay.track.o.iter().any(|o| o.abs() > 0.1));
    }

    #[test]
    fn seed_and_per_switch_variation_are_reproducible() {
        let mut a = Replay::new(false);
        let mut b = Replay::new(false);
        a.advance_ms(1100.0);
        b.advance_ms(1100.0);
        assert_eq!(a.track.shader_values(), b.track.shader_values());
        let tune = a.track.stiffness;
        a.advance_ms(2300.0);
        assert_ne!(tune, a.track.stiffness);
    }
}
