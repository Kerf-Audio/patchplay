//! DSP primitives. Everything here is plain state with no allocation, so it
//! is safe on the audio thread.
//!
//! Voltage conventions shared by every module (Eurorack-like):
//! - audio: ±5 V
//! - pitch: 1 V per octave, 0 V = C4
//! - gate and clock: 0 V low, 10 V high
//! - cv: 0..10 V

use std::f32::consts::PI;

pub const AUDIO_VOLTS: f32 = 5.0;
pub const GATE_HIGH: f32 = 10.0;
pub const CV_MAX: f32 = 10.0;
/// Length of trigger pulses (clock ticks, envelope ends, sequencer hits).
pub const PULSE_SECONDS: f32 = 0.005;

/// 0 V = C4, 1 V per octave.
pub fn volts_to_hz(v: f32) -> f32 {
    261.625_58 * v.exp2()
}

pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Coefficient for a one-pole smoother that settles in about `seconds`.
pub fn smoothing(seconds: f32, sample_rate: f32) -> f32 {
    if seconds <= 0.0 {
        1.0
    } else {
        1.0 - (-1.0 / (seconds * sample_rate)).exp()
    }
}

/// Coefficient for a one-pole low-pass at `hz`.
pub fn lowpass_coeff(hz: f32, sample_rate: f32) -> f32 {
    1.0 - (-2.0 * PI * hz / sample_rate).exp()
}

/// Zeroes values too small to matter. Recursive states that decay towards
/// silence otherwise settle in the denormal range, which is slow on x86 and
/// WebAssembly, and a product of two such states stays there.
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}

/// One-pole low-pass, used for slew, glide and gentle filtering.
#[derive(Debug, Clone, Copy, Default)]
pub struct OnePole {
    y: f32,
}

impl OnePole {
    pub fn process(&mut self, x: f32, coeff: f32) -> f32 {
        self.y = flush(self.y + coeff * (x - self.y));
        self.y
    }

    pub fn value(&self) -> f32 {
        self.y
    }
}

/// Outputs of one [`Svf`] step.
#[derive(Debug, Clone, Copy)]
pub struct SvfOut {
    pub low: f32,
    pub band: f32,
    pub high: f32,
}

/// Filter coefficients for one cutoff, `q` and sample rate.
#[derive(Debug, Clone, Copy)]
struct SvfCoeffs {
    cutoff: f32,
    q: f32,
    sample_rate: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl SvfCoeffs {
    fn new(cutoff: f32, q: f32, sample_rate: f32) -> Self {
        let g = (PI * (cutoff / sample_rate).min(0.49)).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self {
            cutoff,
            q,
            sample_rate,
            k,
            a1,
            a2,
            a3: g * a2,
        }
    }

    fn is_for(&self, cutoff: f32, q: f32, sample_rate: f32) -> bool {
        self.cutoff == cutoff && self.q == q && self.sample_rate == sample_rate
    }
}

/// Topology-preserving state-variable filter (Zavalishin). Stable under fast
/// cutoff modulation. The coefficients, which cost a `tan`, are kept until
/// the cutoff, `q` or sample rate changes.
#[derive(Debug, Clone, Copy, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
    coeffs: Option<SvfCoeffs>,
}

impl Svf {
    /// `cutoff` in Hz, `q` > 0.
    pub fn process(&mut self, x: f32, cutoff: f32, q: f32, sample_rate: f32) -> SvfOut {
        let coeffs = match self.coeffs {
            Some(c) if c.is_for(cutoff, q, sample_rate) => c,
            _ => {
                let c = SvfCoeffs::new(cutoff, q, sample_rate);
                self.coeffs = Some(c);
                c
            }
        };
        let SvfCoeffs { k, a1, a2, a3, .. } = coeffs;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = flush(2.0 * v1 - self.ic1);
        self.ic2 = flush(2.0 * v2 - self.ic2);
        SvfOut {
            low: v2,
            band: v1,
            high: x - k * v1 - v2,
        }
    }
}

/// Oscillator phase in `0..1`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Phase(f32);

impl Phase {
    /// Advances by `dt` (cycles per sample) and returns the new phase.
    pub fn advance(&mut self, dt: f32) -> f32 {
        self.0 += dt;
        if self.0 >= 1.0 {
            self.0 -= self.0.floor();
        }
        self.0
    }
}

/// PolyBLEP correction for a unit step at phase 0; `dt` is the phase step.
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

/// What a gate input did this sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    None,
    Rose,
    Fell,
}

/// Schmitt-trigger gate reader: high above 1 V, low below 0.5 V.
#[derive(Debug, Clone, Copy, Default)]
pub struct Gate {
    high: bool,
}

impl Gate {
    pub fn update(&mut self, volts: f32) -> Edge {
        if !self.high && volts > 1.0 {
            self.high = true;
            Edge::Rose
        } else if self.high && volts < 0.5 {
            self.high = false;
            Edge::Fell
        } else {
            Edge::None
        }
    }

    pub fn is_high(&self) -> bool {
        self.high
    }
}

/// Seeded PRNG (SplitMix64): same seed, same sequence on every platform.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..1`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// FNV-1a: turns ids into seeds.
pub fn hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svf_low_pass_passes_dc_and_blocks_nyquist() {
        let mut f = Svf::default();
        let mut dc = 0.0;
        for _ in 0..4800 {
            dc = f.process(1.0, 500.0, 0.707, 48_000.0).low;
        }
        assert!((dc - 1.0).abs() < 1e-3, "dc {dc}");
        let mut f = Svf::default();
        let mut peak = 0f32;
        for n in 0..4800 {
            let x = if n % 2 == 0 { 1.0 } else { -1.0 };
            let y = f.process(x, 500.0, 0.707, 48_000.0).low;
            if n > 2400 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.01, "nyquist leaks {peak}");
    }

    #[test]
    fn one_pole_decays_to_exact_silence() {
        let mut f = OnePole::default();
        f.process(1.0, 1.0);
        for _ in 0..20_000 {
            f.process(0.0, 0.01);
        }
        assert_eq!(f.value(), 0.0, "stuck at {:e}", f.value());
    }

    #[test]
    fn svf_rings_down_to_exact_silence() {
        let mut f = Svf::default();
        f.process(1.0, 100.0, 0.707, 48_000.0);
        for _ in 0..200_000 {
            f.process(0.0, 100.0, 0.707, 48_000.0);
        }
        assert_eq!((f.ic1, f.ic2), (0.0, 0.0));
    }

    #[test]
    fn svf_cached_coefficients_match_fresh_ones() {
        let mut cached = Svf::default();
        for n in 0..2_000 {
            let x = ((n * 37 % 101) as f32 - 50.0) / 50.0;
            // The cutoff holds for 100 samples at a time: hits and misses.
            let cutoff = 200.0 * (1 + n / 100) as f32;
            let mut fresh = Svf {
                coeffs: None,
                ..cached
            };
            let a = cached.process(x, cutoff, 2.0, 48_000.0);
            let b = fresh.process(x, cutoff, 2.0, 48_000.0);
            assert_eq!((a.low, a.band, a.high), (b.low, b.band, b.high));
            assert_eq!((cached.ic1, cached.ic2), (fresh.ic1, fresh.ic2));
        }
    }
}
