//! Exact retained ribbon physics. Native services observe workspaces; this module
//! owns only motion and bounded shader publication, with no I/O, threads or timers.
#![deny(unsafe_code)]

mod workspace_motion;
mod workspace_track;

use std::time::Instant;
use workspace_track::{Track, slots_for};

pub const ROW_HEIGHT: f32 = workspace_track::ROW;
pub const SIDE_PADDING: f32 = workspace_track::PAD;
pub const SHADER_HEIGHT: f32 = workspace_track::HEIGHT;
pub const CENTER_Y: f32 = workspace_track::CENTER;

/// Whether native timer publication is needed after publishing this sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Resting,
    Animating,
}

/// Sixteen independent shape scalars and native colour input. No heap allocation
/// occurs when reading a sample. Colour no longer consumes three shader scalars.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub shape: [f32; 16],
    pub color: [f32; 4],
    pub width: f32,
    pub activity: Activity,
}

/// One view's simulation, retained outside the reloadable scene logic.
#[derive(Default)]
pub struct Indicator {
    track: Track,
}

impl Indicator {
    /// The calendar uses the identical retained track stretched to twelve months.
    #[must_use]
    pub fn calendar() -> Self {
        Self {
            track: Track::rail(12),
        }
    }

    /// Observe native workspace state. Slot capacity remains five to ten exactly;
    /// an absent/out-of-range active slot hides the bead as in the reference.
    pub fn observe(&mut self, highest: Option<i64>, active: Option<usize>, now: Instant) {
        self.track.advance(now);
        self.track.sync(slots_for(highest), active, now);
    }

    /// Advance at the design's fixed 240Hz step, capped at 50ms after a late frame.
    /// The native host cancels its publication timer when this rests.
    pub fn advance(&mut self, now: Instant) -> Sample {
        self.track.advance(now);
        self.sample()
    }

    /// Publish without advancing; changing view invocation cannot change physics.
    #[must_use]
    pub fn sample(&self) -> Sample {
        let values = self.track.shader_values();
        let mut shape = [0.0; 16];
        shape.copy_from_slice(&values[..16]);
        Sample {
            shape,
            color: [
                values[16],
                values[17],
                values[18],
                f32::from(self.track.has_active()),
            ],
            width: self.track.width() as f32,
            activity: if self.track.animating() {
                Activity::Animating
            } else {
                Activity::Resting
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn native_observation_appears_at_rest_and_view_reads_do_not_advance() {
        let now = Instant::now();
        let mut indicator = Indicator::default();
        assert_eq!(indicator.sample().color[3], 0.0);
        indicator.observe(Some(5), Some(1), now);
        let sample = indicator.sample();
        assert_eq!(sample.activity, Activity::Resting);
        assert_eq!(sample.width, 80.0);
        assert_eq!(sample.shape[0], 8.0);
        assert_eq!(sample.color[3], 1.0);
        for _ in 0..100 {
            assert_eq!(indicator.sample(), sample);
        }
        assert_eq!(indicator.advance(now + Duration::from_secs(3600)), sample);
    }

    #[test]
    fn interrupted_native_observations_keep_motion_and_settle_square() {
        let now = Instant::now();
        let mut indicator = Indicator::default();
        indicator.observe(Some(5), Some(1), now);
        indicator.observe(Some(5), Some(5), now);
        let flying = indicator.advance(now + Duration::from_millis(40));
        assert_eq!(flying.activity, Activity::Animating);
        assert!(flying.shape[1].abs() > 1.0);
        indicator.observe(Some(5), Some(2), now + Duration::from_millis(40));
        assert_eq!(indicator.sample().shape[1], flying.shape[1]);
        let mut sample = indicator.sample();
        for tick in 2..2000 {
            sample = indicator.advance(now + Duration::from_millis(tick * 20));
            if sample.activity == Activity::Resting {
                break;
            }
        }
        assert_eq!(sample.activity, Activity::Resting);
        assert_eq!(sample.shape[0], 24.0);
        assert_eq!(sample.shape[1], 0.0);
        assert_eq!(sample.shape[2], 2.0);
        assert_eq!(sample.shape[3], 0.0);
        assert!(sample.shape.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn public_rest_and_next_switch_are_identical_across_sub_cap_cadences() {
        let now = Instant::now();
        let mut results = Vec::new();
        for cadence in [1, 5, 10, 25, 40] {
            let mut indicator = Indicator::default();
            indicator.observe(Some(5), Some(1), now);
            indicator.observe(Some(5), Some(5), now);
            for ms in (cadence..=5000).step_by(cadence as usize) {
                indicator.advance(now + Duration::from_millis(ms));
            }
            indicator.advance(now + Duration::from_millis(5000));
            let resting = indicator.sample();
            assert_eq!(resting.activity, Activity::Resting);
            indicator.observe(Some(5), Some(1), now + Duration::from_millis(5000));
            for ms in (cadence..=200).step_by(cadence as usize) {
                indicator.advance(now + Duration::from_millis(5000 + ms));
            }
            indicator.advance(now + Duration::from_millis(5200));
            results.push((resting, indicator.sample()));
        }
        for result in &results[1..] {
            assert_eq!(result, &results[0]);
        }
    }

    #[test]
    fn shader_sample_matches_retained_cpu_geometry() {
        let now = Instant::now();
        let mut indicator = Indicator::default();
        indicator.observe(Some(5), Some(1), now);
        let frame = indicator.track.frame();
        let bead = frame.bead.unwrap();
        let sample = indicator.sample();
        assert_eq!(frame.rail, [1.0, 1.0, 1.0, 0.14]);
        assert_eq!(frame.width, sample.width);
        assert_eq!(bead.glow_sigma, sample.shape[15]);
    }

    #[test]
    fn native_capacity_and_calendar_month_track_are_bounded() {
        let now = Instant::now();
        let mut indicator = Indicator::default();
        indicator.observe(Some(i64::MAX), Some(10), now);
        assert_eq!(indicator.sample().width, 160.0);
        indicator.observe(Some(-1), Some(999), now);
        assert_eq!(indicator.sample().width, 80.0);
        assert_eq!(indicator.sample().color[3], 0.0);
        let mut calendar = Indicator::calendar();
        calendar.observe(Some(12), Some(12), now);
        assert_eq!(calendar.sample().width, 192.0);
        assert_eq!(calendar.sample().shape[0], 184.0);
        assert_eq!(calendar.sample().activity, Activity::Resting);
    }
}
