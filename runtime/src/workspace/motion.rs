#![deny(unsafe_code)]
/// The design's fixed simulation step.
pub(crate) const STEP: f64 = 1.0 / 240.0;
/// A late frame advances a simulation by at most this much.
pub(crate) const MAX_FRAME: f64 = 0.05;
/// Below this residual energy a spring system with pixel-sized coordinates comes to rest.
pub(crate) const SETTLE_ENERGY: f64 = 6e-3;
/// xorshift64*: the design draws each spring's stiffness and damping from `Math.random`. A fixed
/// seed keeps rendering reproducible; consecutive draws still differ.
#[derive(Clone, Debug)]
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    #[allow(
        clippy::cast_precision_loss,
        reason = "The RNG shifts to 53 bits before conversion, exactly representable in f64"
    )]
    pub(crate) fn unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
    }

    pub(crate) fn range(&mut self, a: f64, b: f64) -> f64 {
        a + self.unit() * (b - a)
    }
}
