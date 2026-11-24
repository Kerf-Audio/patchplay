//! Safe-check: render a patch offline and judge the result.

use kerf_core::Patch;
use kerf_engine::Engine;
use serde::Serialize;

/// Offline render length and rate. Short and low: a check runs for every
/// attempt of every generation.
const SECONDS: f32 = 6.0;
const SAMPLE_RATE: f32 = 32_000.0;

const MIN_RMS_DB: f32 = -45.0;
const HOT_RMS_DB: f32 = -6.0;
const SILENT_WINDOW_DB: f32 = -60.0;
const MAX_SILENT_FRACTION: f32 = 0.6;
const MAX_DC: f32 = 0.02;
/// Peaks may sit this far above the limiter ceiling (or -1 dBFS) before
/// the patch counts as clipping.
const PEAK_MARGIN_DB: f32 = 0.05;
const PEAK_CEILING_DB: f32 = -1.0;
const RUNAWAY_GROWTH: f32 = 3.0;
const RUNAWAY_FLOOR_DB: f32 = -20.0;
const FLOOR_DB: f32 = -120.0;
const WINDOW_SECONDS: f32 = 0.1;

/// What `safe_check` found. dB values are floored at -120 so the report is
/// plain JSON.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckReport {
    pub pass: bool,
    pub seconds: f32,
    pub sample_rate: f32,
    pub peak_db: f32,
    pub rms_db: f32,
    /// The Finish limiter ceiling the peak is judged against.
    pub ceiling_db: f32,
    /// Share of 100 ms windows below -60 dB.
    pub silent_fraction: f32,
    pub dc_offset: f32,
    pub finite: bool,
    pub clipped: bool,
    pub runaway: bool,
    /// Module kinds in the patch that make no sound yet (informational).
    pub silent_kinds: Vec<String>,
    /// One line per failed rule.
    pub problems: Vec<String>,
}

fn to_db(gain: f32) -> f32 {
    if gain > 0.0 {
        (20.0 * gain.log10()).max(FLOOR_DB)
    } else {
        FLOOR_DB
    }
}

/// The lowest Finish ceiling in the patch, in dB; 0 when there is no Finish.
fn ceiling_db(patch: &Patch) -> f32 {
    patch
        .modules
        .iter()
        .filter(|m| m.kind == "Finish")
        .filter_map(|m| m.params.iter().find(|p| p.key.as_str() == "ceiling"))
        .map(|p| -12.0 + p.value * 12.0)
        .fold(0.0, f32::min)
}

/// Renders the patch offline and checks peak, RMS, silence, NaN or infinity,
/// DC offset, clipping above the Finish ceiling and runaway feedback.
pub fn safe_check(patch: &Patch) -> CheckReport {
    let ceiling = ceiling_db(patch);
    match Engine::new(patch, SAMPLE_RATE) {
        Ok(mut engine) => {
            let silent_kinds = engine.silent_kinds().to_vec();
            let render = engine.render((SECONDS * SAMPLE_RATE) as usize);
            analyse(
                &render.left,
                &render.right,
                SAMPLE_RATE,
                ceiling,
                silent_kinds,
            )
        }
        Err(e) => CheckReport {
            pass: false,
            seconds: 0.0,
            sample_rate: SAMPLE_RATE,
            peak_db: FLOOR_DB,
            rms_db: FLOOR_DB,
            ceiling_db: ceiling,
            silent_fraction: 1.0,
            dc_offset: 0.0,
            finite: true,
            clipped: false,
            runaway: false,
            silent_kinds: Vec::new(),
            problems: vec![format!("the engine refused the patch: {e}")],
        },
    }
}

pub(crate) fn analyse(
    left: &[f32],
    right: &[f32],
    rate: f32,
    ceiling_db: f32,
    silent_kinds: Vec<String>,
) -> CheckReport {
    let frames = left.len().min(right.len());
    let window = ((rate * WINDOW_SECONDS) as usize).max(1);
    let (mut finite, mut peak) = (true, 0.0f32);
    let (mut sum_sq, mut sum_l, mut sum_r) = (0.0f64, 0.0f64, 0.0f64);
    let (mut win_sq, mut in_win) = (0.0f64, 0);
    let mut windows: Vec<f32> = Vec::new();
    for (&l, &r) in left.iter().zip(right) {
        if !(l.is_finite() && r.is_finite()) {
            finite = false;
            continue;
        }
        peak = peak.max(l.abs()).max(r.abs());
        let sq = f64::from(l) * f64::from(l) + f64::from(r) * f64::from(r);
        sum_sq += sq;
        win_sq += sq;
        sum_l += f64::from(l);
        sum_r += f64::from(r);
        in_win += 1;
        if in_win == window {
            windows.push((win_sq / (2.0 * window as f64)).sqrt() as f32);
            (win_sq, in_win) = (0.0, 0);
        }
    }
    let n = frames.max(1) as f64;
    let rms = (sum_sq / (2.0 * n)).sqrt() as f32;
    let dc_offset = (sum_l.abs().max(sum_r.abs()) / n) as f32;
    let silent_fraction = if windows.is_empty() {
        1.0
    } else {
        let floor = 10f32.powf(SILENT_WINDOW_DB / 20.0);
        windows.iter().filter(|&&w| w < floor).count() as f32 / windows.len() as f32
    };
    let half = windows.len() / 2;
    let mean = |w: &[f32]| w.iter().sum::<f32>() / w.len().max(1) as f32;
    let (head, tail) = (mean(&windows[..half]), mean(&windows[half..]));
    let (peak_db, rms_db) = (to_db(peak), to_db(rms));

    let limit = ceiling_db.min(PEAK_CEILING_DB);
    let clipped = peak_db > limit + PEAK_MARGIN_DB;
    let runaway =
        (tail > head * RUNAWAY_GROWTH && to_db(tail) > RUNAWAY_FLOOR_DB) || rms_db > HOT_RMS_DB;

    let mut problems = Vec::new();
    if !finite {
        problems.push("the render contains NaN or infinite samples".to_owned());
    }
    if rms_db < MIN_RMS_DB {
        problems.push(format!(
            "too quiet: RMS {rms_db:.1} dBFS is below {MIN_RMS_DB}"
        ));
    }
    if silent_fraction > MAX_SILENT_FRACTION {
        problems.push(format!(
            "{:.0}% of the render is silent",
            silent_fraction * 100.0
        ));
    }
    if dc_offset > MAX_DC {
        problems.push(format!("DC offset {dc_offset:.3} is above {MAX_DC}"));
    }
    if clipped {
        problems.push(format!(
            "peak {peak_db:.1} dBFS is above the {limit:.1} dBFS ceiling"
        ));
    }
    if runaway {
        problems.push("the level runs away (feedback or a pinned limiter)".to_owned());
    }
    CheckReport {
        pass: problems.is_empty(),
        seconds: frames as f32 / rate,
        sample_rate: rate,
        peak_db,
        rms_db,
        ceiling_db,
        silent_fraction,
        dc_offset,
        finite,
        clipped,
        runaway,
        silent_kinds,
        problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 8000.0;

    fn tone(amp: f32, frames: usize, f: impl Fn(usize) -> f32) -> Vec<f32> {
        (0..frames)
            .map(|i| amp * (std::f32::consts::TAU * 220.0 * i as f32 / RATE).sin() * f(i))
            .collect()
    }

    fn check(left: &[f32]) -> CheckReport {
        analyse(left, left, RATE, -2.4, Vec::new())
    }

    #[test]
    fn a_steady_quiet_tone_passes() {
        let s = tone(0.2, 48_000, |_| 1.0);
        let r = check(&s);
        assert!(r.pass, "{:?}", r.problems);
        assert!(r.rms_db < -10.0 && r.rms_db > -20.0);
    }

    #[test]
    fn silence_fails() {
        let r = check(&vec![0.0; 48_000]);
        assert!(!r.pass);
        assert!(r.silent_fraction > 0.99);
    }

    #[test]
    fn nan_fails() {
        let mut s = tone(0.2, 48_000, |_| 1.0);
        s[100] = f32::NAN;
        let r = check(&s);
        assert!(!r.finite && !r.pass);
    }

    #[test]
    fn dc_offset_fails() {
        let s: Vec<f32> = tone(0.1, 48_000, |_| 1.0).iter().map(|x| x + 0.1).collect();
        let r = check(&s);
        assert!(r.dc_offset > MAX_DC && !r.pass);
    }

    #[test]
    fn peaks_above_the_ceiling_fail() {
        let r = check(&tone(0.9, 48_000, |_| 1.0));
        assert!(r.clipped && !r.pass);
    }

    #[test]
    fn a_swell_that_runs_away_fails() {
        let s = tone(0.3, 48_000, |i| if i < 24_000 { 0.05 } else { 1.0 });
        assert!(check(&s).runaway);
    }

    #[test]
    fn reports_serialise_without_nulls() {
        let json = serde_json::to_string(&check(&vec![0.0; 8000])).unwrap();
        assert!(!json.contains("null"));
    }
}
