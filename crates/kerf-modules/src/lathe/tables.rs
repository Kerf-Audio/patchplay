//! The Lathe wavetable bank: 32 tables of 8 frames, each frame stored as a
//! stack of band-limited mips so reads never alias.

use std::f32::consts::{PI, TAU};
use std::sync::LazyLock;

use kerf_dsp::{Rng, lerp};

pub const TABLES: usize = 32;
pub const FRAMES: usize = 8;
pub const MIPS: usize = 9;

/// Harmonics kept in mip `m`: 256, 128, … 1.
pub const fn harmonics(mip: usize) -> usize {
    256 >> mip
}

/// Samples per cycle in mip `m`: eight per harmonic, so cubic reads stay
/// clean right up to the top partial.
const fn size(mip: usize) -> usize {
    8 * harmonics(mip)
}

/// One spectral shape. Harmonic `k` has cosine and sine amplitudes.
#[derive(Debug, Clone, Copy)]
enum Shape {
    Sine,
    Tri,
    Saw,
    Square,
    /// Pulse of the given width.
    Pulse(f32),
    /// Every harmonic at `1 / k^p`.
    Tilt(f32),
    /// Odd harmonics at `1 / k^p`.
    Odd(f32),
    /// The fundamental and even harmonics at `1 / k^p`.
    Even(f32),
    /// A vowel-like hump of harmonics: centre and width, in harmonics.
    Formant(f32, f32),
    /// The fundamental and every n-th harmonic.
    Comb(usize),
    /// Organ drawbars (1, 2, 3, 4, 6, 8, 10, 12) as bits.
    Organ(u8),
    /// Harmonics at scrambled phases, seeded: a buzzy, digital edge.
    Digital(u64),
}

use Shape::*;

const ORGAN: [usize; 8] = [1, 2, 3, 4, 6, 8, 10, 12];

impl Shape {
    /// `(cosine, sine)` amplitudes of harmonic `k` (1-based).
    fn coeff(self, k: usize) -> (f32, f32) {
        let kf = k as f32;
        let odd = k % 2 == 1;
        match self {
            Sine => (0.0, if k == 1 { 1.0 } else { 0.0 }),
            Saw => (0.0, 1.0 / kf),
            Square if odd => (0.0, 1.0 / kf),
            Tri if odd => (0.0, if k % 4 == 1 { 1.0 } else { -1.0 } / (kf * kf)),
            Pulse(width) => {
                let a = TAU * kf * width;
                (a.sin() / kf, (1.0 - a.cos()) / kf)
            }
            Tilt(p) => (0.0, kf.powf(-p)),
            Odd(p) if odd => (0.0, kf.powf(-p)),
            Even(p) if !odd || k == 1 => (0.0, kf.powf(-p)),
            Formant(centre, width) => {
                let d = (kf - centre) / width;
                let fundamental = if k == 1 { 0.3 } else { 0.0 };
                (0.0, (-d * d).exp() / kf.sqrt() + fundamental)
            }
            Comb(n) if k == 1 || k % n == 0 => (0.0, 1.0 / kf),
            Organ(bits) => {
                let on = ORGAN
                    .iter()
                    .enumerate()
                    .any(|(i, &h)| h == k && bits >> i & 1 == 1);
                (0.0, if on { 1.0 } else { 0.0 })
            }
            Digital(seed) => {
                let phase = TAU * Rng::new(seed * 1000 + k as u64).unit();
                let a = kf.powf(-0.8);
                (a * phase.cos(), a * phase.sin())
            }
            _ => (0.0, 0.0),
        }
    }
}

/// Four shapes per table; Turn sweeps from the first to the last.
const RECIPES: [[Shape; 4]; TABLES] = [
    [Sine, Tri, Square, Saw],
    [Saw, Square, Pulse(0.25), Pulse(0.08)],
    [Tri, Saw, Tilt(0.5), Tilt(0.25)],
    [Sine, Tilt(2.0), Tilt(1.0), Tilt(0.5)],
    [Sine, Odd(1.5), Odd(1.0), Odd(0.5)],
    [Square, Even(1.0), Saw, Even(0.5)],
    [Odd(1.0), Even(1.0), Odd(0.7), Even(0.7)],
    [Sine, Odd(2.0), Even(1.5), Tilt(0.8)],
    [
        Saw,
        Formant(3.0, 1.5),
        Formant(6.0, 2.0),
        Formant(10.0, 3.0),
    ],
    [Sine, Saw, Square, Formant(6.0, 2.0)],
    [
        Square,
        Formant(4.0, 1.2),
        Formant(9.0, 2.0),
        Formant(14.0, 3.0),
    ],
    [
        Tri,
        Formant(2.0, 1.0),
        Formant(5.0, 1.5),
        Formant(12.0, 4.0),
    ],
    [
        Formant(3.0, 1.0),
        Formant(8.0, 1.5),
        Formant(5.0, 1.0),
        Formant(15.0, 2.0),
    ],
    [
        Formant(2.0, 0.8),
        Formant(4.0, 1.0),
        Formant(7.0, 1.2),
        Formant(11.0, 1.5),
    ],
    [
        Saw,
        Formant(20.0, 3.0),
        Formant(8.0, 2.0),
        Formant(3.0, 1.0),
    ],
    [Sine, Formant(12.0, 2.0), Formant(6.0, 1.0), Saw],
    [
        Sine,
        Organ(0b0000_0011),
        Organ(0b0001_1111),
        Organ(0b1111_1111),
    ],
    [
        Organ(0b0000_0101),
        Organ(0b0010_1101),
        Organ(0b0111_0111),
        Organ(0b1110_1011),
    ],
    [
        Organ(0b0000_1001),
        Organ(0b0100_0001),
        Organ(0b1000_0011),
        Organ(0b0011_1100),
    ],
    [
        Sine,
        Organ(0b0000_0010),
        Organ(0b0000_1110),
        Organ(0b0111_1110),
    ],
    [Pulse(0.5), Pulse(0.4), Pulse(0.2), Pulse(0.05)],
    [Comb(2), Comb(3), Comb(4), Comb(5)],
    [Saw, Comb(2), Comb(3), Comb(5)],
    [Sine, Comb(3), Comb(7), Comb(11)],
    [Sine, Digital(1), Digital(2), Digital(3)],
    [Saw, Digital(4), Digital(5), Square],
    [Tri, Digital(6), Digital(7), Digital(8)],
    [Digital(9), Digital(10), Digital(11), Digital(12)],
    [Sine, Tilt(0.3), Tilt(0.6), Tilt(0.9)],
    [Saw, Pulse(0.3), Square, Pulse(0.15)],
    [Tri, Odd(0.5), Square, Odd(0.3)],
    [
        Formant(2.0, 1.0),
        Formant(6.0, 1.0),
        Formant(12.0, 2.0),
        Formant(24.0, 4.0),
    ],
];

type Spectrum = [(f32, f32); harmonics(0) + 1];

/// The spectrum of one frame: the recipe's four shapes blended along Turn.
fn spectrum(recipe: &[Shape; 4], frame: usize) -> Spectrum {
    let x = frame as f32 / (FRAMES - 1) as f32 * 3.0;
    let seg = (x as usize).min(2);
    let t = x - seg as f32;
    let mut s = [(0.0, 0.0); harmonics(0) + 1];
    for (k, out) in s.iter_mut().enumerate().skip(1) {
        let (c0, s0) = recipe[seg].coeff(k);
        let (c1, s1) = recipe[seg + 1].coeff(k);
        *out = (lerp(c0, c1, t), lerp(s0, s1, t));
    }
    s
}

/// In-place radix-2 FFT with a positive exponent (an inverse transform
/// without the 1/n scale). Construction time only.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let step = TAU / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (step * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * c - im[b] * s;
                let ti = re[b] * s + im[b] * c;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// One cycle of `spectrum` limited to `mip`'s harmonics, with Lanczos
/// smoothing so the cut-off does not ring.
fn cycle(spectrum: &Spectrum, mip: usize) -> Vec<f32> {
    let n = size(mip);
    let h = harmonics(mip);
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    for k in 1..=h {
        let x = PI * k as f32 / (h + 1) as f32;
        let sigma = x.sin() / x;
        let (c, s) = spectrum[k];
        re[k] = c * sigma;
        im[k] = -s * sigma;
    }
    fft(&mut re, &mut im);
    re
}

pub struct Bank {
    /// Per mip: every table's frames back to back.
    mips: Vec<Vec<f32>>,
}

impl Bank {
    fn new() -> Self {
        let mut mips: Vec<Vec<f32>> = (0..MIPS)
            .map(|m| Vec::with_capacity(TABLES * FRAMES * size(m)))
            .collect();
        for recipe in &RECIPES {
            for frame in 0..FRAMES {
                let spectrum = spectrum(recipe, frame);
                let mut gain = 1.0;
                for (m, mip) in mips.iter_mut().enumerate() {
                    let wave = cycle(&spectrum, m);
                    if m == 0 {
                        let peak = wave.iter().fold(0.0f32, |p, v| p.max(v.abs()));
                        // Headroom for the cubic read and the ripple in coarser mips.
                        gain = 0.97 / peak.max(1e-6);
                    }
                    mip.extend(wave.iter().map(|v| v * gain));
                }
            }
        }
        Self { mips }
    }

    /// The finest mip whose top harmonic stays below 0.45 of the sample
    /// rate at `dt` cycles per sample.
    pub fn mip_for(dt: f32) -> usize {
        (0..MIPS - 1)
            .find(|&m| harmonics(m) as f32 * dt <= 0.45)
            .unwrap_or(MIPS - 1)
    }

    /// One cycle of `table`'s `frame` in `mip`; the length is a power of two.
    pub fn cycle(&self, mip: usize, table: usize, frame: usize) -> &[f32] {
        let n = size(mip);
        let at = (table * FRAMES + frame) * n;
        &self.mips[mip][at..at + n]
    }
}

/// Built on first use, off the audio thread: a patch builds its Lathes
/// before the engine runs.
pub static BANK: LazyLock<Bank> = LazyLock::new(Bank::new);
