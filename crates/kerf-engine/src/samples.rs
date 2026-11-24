//! Sample files for sample modules (Veneer, Splinter): decode, downmix to mono, resample
//! to the engine rate, normalise gently, cap the length. `kerf-modules` gets
//! buffers; this is where they come from.
//!
//! Two sources: files on disk (the `files` feature, symphonia; the only place
//! that touches the filesystem for audio) and audio a host decoded itself and
//! handed over with [`provide`] under a name of its choosing (the browser,
//! which decodes with the Web Audio API). A sample module's path that matches a
//! provided name uses that audio; anything else is a file path.
//!
//! Decoded files are cached by file (path, size, modification time) and
//! rate, and provided audio by name and rate, so a rebuild after a cable edit
//! does not decode or resample again.

use std::f64::consts::PI;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use kerf_core::Patch;
use kerf_modules::VENEER_SLICES;

/// Longer files are cut to this many seconds.
pub const MAX_SECONDS: f32 = 60.0;
/// Peaks are scaled to this, with at most [`MAX_GAIN`] of boost.
const TARGET_PEAK: f32 = 0.9;
const MAX_GAIN: f32 = 8.0;
/// Zero crossings each side of the resampling kernel, and its table density.
const KERNEL_WIDTH: usize = 8;
const KERNEL_STEPS: usize = 1024;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SampleError {
    #[error("file not found")]
    NotFound,
    #[error("cannot read the file: {0}")]
    Io(String),
    #[error("not a supported audio file ({0}); WAV, AIFF, FLAC, MP3 and Ogg Vorbis work")]
    Unsupported(String),
    #[error("the file has no audio in it")]
    Empty,
    #[error("the file is too short to play (at least {VENEER_SLICES} samples)")]
    TooShort,
}

/// Audio a host handed over, with its prepared copies per engine rate.
struct Provided {
    name: String,
    mono: Arc<[f32]>,
    rate: u32,
    ready: Vec<(u32, Arc<[f32]>)>,
}

static PROVIDED: Mutex<Vec<Provided>> = Mutex::new(Vec::new());

/// Registers decoded mono audio (`rate` Hz) under `name`, replacing any earlier audio of that
/// name. Sample modules whose sample path is `name` play it, whatever the platform.
pub fn provide(name: &str, mono: &[f32], rate: u32) -> Result<(), SampleError> {
    if mono.is_empty() || rate == 0 {
        return Err(SampleError::Empty);
    }
    let keep = ((MAX_SECONDS * rate as f32) as usize).min(mono.len());
    let entry = Provided {
        name: name.to_owned(),
        mono: mono[..keep].into(),
        rate,
        ready: Vec::new(),
    };
    let mut all = PROVIDED.lock().unwrap_or_else(PoisonError::into_inner);
    all.retain(|p| p.name != name);
    all.push(entry);
    Ok(())
}

/// Forgets audio registered with [`provide`].
pub fn forget(name: &str) {
    PROVIDED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .retain(|p| p.name != name);
}

fn provided(name: &str, rate: u32) -> Option<Result<Arc<[f32]>, SampleError>> {
    let mut all = PROVIDED.lock().unwrap_or_else(PoisonError::into_inner);
    let p = all.iter_mut().find(|p| p.name == name)?;
    if let Some((_, hit)) = p.ready.iter().find(|(r, _)| *r == rate) {
        return Some(Ok(hit.clone()));
    }
    let made = prepare(&p.mono, p.rate, rate);
    if let Ok(buf) = &made {
        p.ready.push((rate, buf.clone()));
    }
    Some(made)
}

/// Resamples decoded mono audio from `from` to `to` Hz, caps its length, checks it can be cut
/// into slices and normalises it: the buffer a Veneer slices or a Splinter grains.
pub fn prepare(mono: &[f32], from: u32, to: u32) -> Result<Arc<[f32]>, SampleError> {
    if mono.is_empty() || from == 0 || to == 0 {
        return Err(SampleError::Empty);
    }
    let mut samples = resample(mono, from, to);
    samples.truncate((MAX_SECONDS * to as f32) as usize);
    if samples.len() < VENEER_SLICES {
        return Err(SampleError::TooShort);
    }
    normalise(&mut samples);
    Ok(samples.into())
}

/// Loads `path` as mono samples at `rate` Hz, ready for a sample module: provided audio of that name,
/// else the file. Relative paths are taken from the working directory; see [`resolve_paths`].
pub fn load(path: &Path, rate: f32) -> Result<Arc<[f32]>, SampleError> {
    let rate = rate.round() as u32;
    if let Some(found) = path.to_str().and_then(|name| provided(name, rate)) {
        return found;
    }
    load_file(path, rate)
}

#[cfg(not(feature = "files"))]
fn load_file(_path: &Path, _rate: u32) -> Result<Arc<[f32]>, SampleError> {
    Err(SampleError::NotFound)
}

#[cfg(feature = "files")]
use files::load_file;

/// Files on disk: decoded once, kept for reuse.
#[cfg(feature = "files")]
mod files {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::time::SystemTime;

    use super::{MAX_SECONDS, SampleError, prepare};
    use crate::decode::decode;

    const CACHE_ENTRIES: usize = 6;

    struct Cached {
        path: PathBuf,
        stamp: Option<(u64, SystemTime)>,
        rate: u32,
        samples: Arc<[f32]>,
    }

    static CACHE: Mutex<Vec<Cached>> = Mutex::new(Vec::new());

    fn stamp(path: &Path) -> Option<(u64, SystemTime)> {
        let meta = std::fs::metadata(path).ok()?;
        Some((meta.len(), meta.modified().ok()?))
    }

    pub(super) fn load_file(path: &Path, rate: u32) -> Result<Arc<[f32]>, SampleError> {
        let stamp = stamp(path);
        {
            let cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(hit) = cache
                .iter()
                .find(|c| c.path == path && c.stamp == stamp && c.rate == rate && stamp.is_some())
            {
                return Ok(hit.samples.clone());
            }
        }
        let decoded = decode(path, MAX_SECONDS)?;
        let samples = prepare(&decoded.mono, decoded.rate, rate)?;
        if stamp.is_some() {
            let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
            cache.retain(|c| !(c.path == path && c.rate == rate));
            if cache.len() == CACHE_ENTRIES {
                cache.remove(0);
            }
            cache.push(Cached {
                path: path.to_owned(),
                stamp,
                rate,
                samples: samples.clone(),
            });
        }
        Ok(samples)
    }
}

/// Makes every relative sample path absolute against `base` (the folder of
/// the `.kerf` file), so an engine built from the patch finds its files.
pub fn resolve_paths(patch: &mut Patch, base: &Path) {
    for path in patch.modules.iter_mut().filter_map(|m| m.sample.as_mut()) {
        if Path::new(path.as_str()).is_relative() {
            *path = base.join(&*path).to_string_lossy().into_owned();
        }
    }
}

/// Min and max of the buffer in `bins` equal stretches, for drawing a
/// waveform. Fewer pairs come back when the buffer is shorter than `bins`.
pub fn peaks(samples: &[f32], bins: usize) -> Vec<[f32; 2]> {
    let bins = bins.min(samples.len());
    (0..bins)
        .map(|b| {
            let from = b * samples.len() / bins;
            let to = ((b + 1) * samples.len() / bins).max(from + 1);
            samples[from..to]
                .iter()
                .fold([f32::MAX, f32::MIN], |[lo, hi], &x| [lo.min(x), hi.max(x)])
        })
        .collect()
}

/// Scales to [`TARGET_PEAK`], boosting by at most [`MAX_GAIN`].
fn normalise(samples: &mut [f32]) {
    let peak = samples.iter().fold(0.0_f32, |p, x| p.max(x.abs()));
    if peak > 1e-6 {
        let gain = (TARGET_PEAK / peak).min(MAX_GAIN);
        for x in samples {
            *x *= gain;
        }
    }
}

/// Windowed-sinc resampling (Hann window, [`KERNEL_WIDTH`] zero crossings,
/// low-passed at the lower of the two Nyquist frequencies).
fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return input.to_vec();
    }
    let step = f64::from(from) / f64::from(to);
    let cutoff = (1.0 / step).min(1.0);
    let radius = KERNEL_WIDTH as f64 / cutoff;
    // Kernel over `u = distance * cutoff` in `0..KERNEL_WIDTH`.
    let table: Vec<f64> = (0..=KERNEL_WIDTH * KERNEL_STEPS)
        .map(|i| {
            let u = i as f64 / KERNEL_STEPS as f64;
            let sinc = if u == 0.0 {
                1.0
            } else {
                (PI * u).sin() / (PI * u)
            };
            sinc * (0.5 + 0.5 * (PI * u / KERNEL_WIDTH as f64).cos())
        })
        .collect();
    let kernel = |distance: f64| {
        let at = distance.abs() * cutoff * KERNEL_STEPS as f64;
        let i = at as usize;
        match (table.get(i), table.get(i + 1)) {
            (Some(a), Some(b)) => a + (b - a) * (at - i as f64),
            _ => 0.0,
        }
    };

    let out_len = (input.len() as f64 / step).round() as usize;
    (0..out_len)
        .map(|n| {
            let t = n as f64 * step;
            let first = (t - radius).ceil().max(0.0) as usize;
            let last = ((t + radius).floor() as usize).min(input.len() - 1);
            let mut sum = 0.0;
            let mut weight = 0.0;
            let window = input.get(first..=last).unwrap_or_default();
            for (k, &x) in (first..).zip(window) {
                let w = kernel(k as f64 - t);
                sum += f64::from(x) * w;
                weight += w;
            }
            if weight.abs() > 1e-9 {
                (sum / weight) as f32
            } else {
                0.0
            }
        })
        .collect()
}
