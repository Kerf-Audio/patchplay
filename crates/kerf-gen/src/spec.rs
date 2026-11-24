//! Param specs: how a rulebook says what a param may be.

use kerf_dsp::Rng;
use serde::Deserialize;

use crate::util::{index, int_between};

/// Lane lengths run 1..=16 and are stored as `(n - 1) / 15`.
const MAX_STEPS: i32 = 16;
/// Pitch params such as Chisel's `tune` span ±2 octaves: 48 semitones.
const TUNE_SEMITONES: f32 = 48.0;
/// Fret's `root` spans C to B: 12 semitones, stored as `n / 12`.
const ROOT_SEMITONES: f32 = 12.0;

/// What one param may be. In TOML:
///
/// | form | meaning |
/// |---|---|
/// | `0.4` | always this value |
/// | `[0.2, 0.5]` | uniform in the range |
/// | `{ steps = [5, 9] }` | a lane length of 5 to 9 steps |
/// | `{ pick = [0.0, 0.5] }` | one of these values |
/// | `{ octave = [-2, -1] }` | a pitch param set to the patch key's root, that many octaves from C4 |
/// | `{ root = true }` | a quantiser root (Fret's `root`, stored as semitones / 12): the patch key's note |
///
/// All values are normalised `0..=1`, like the params themselves.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Spec {
    Fixed(f32),
    Range([f32; 2]),
    Steps { steps: [u8; 2] },
    Pick { pick: Vec<f32> },
    Octave { octave: [i8; 2] },
    Root { root: bool },
}

/// Every value in `0..=1`: what a patch with no known style may drift over.
pub(crate) static FULL_RANGE: Spec = Spec::Range([0.0, 1.0]);

fn tidy(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * 1e5).round() / 1e5
}

fn steps_value(n: i32) -> f32 {
    tidy((n - 1) as f32 / (MAX_STEPS - 1) as f32)
}

fn steps_of(value: f32) -> i32 {
    1 + (value * (MAX_STEPS - 1) as f32).round() as i32
}

/// A pitch param that puts `root` (0 = C … 11 = B) `octave` octaves above C4.
fn octave_value(root: i32, octave: i32) -> f32 {
    tidy(0.5 + (root + 12 * octave) as f32 / TUNE_SEMITONES)
}

impl Spec {
    /// A fresh value. `root` is the key's pitch class.
    pub(crate) fn sample(&self, rng: &mut Rng, root: i32) -> f32 {
        match self {
            Spec::Fixed(v) => *v,
            Spec::Range([lo, hi]) => tidy(lo + (hi - lo) * rng.unit()),
            Spec::Steps { steps: [lo, hi] } => {
                steps_value(int_between(rng, i32::from(*lo), i32::from(*hi)))
            }
            Spec::Pick { pick } => pick[index(rng, pick.len())],
            Spec::Octave { octave: [lo, hi] } => {
                octave_value(root, int_between(rng, i32::from(*lo), i32::from(*hi)))
            }
            Spec::Root { .. } => tidy(root as f32 / ROOT_SEMITONES),
        }
    }

    /// A drift from `current`, by up to `strength` of the spec's span. Values
    /// already outside the range stay reachable.
    pub(crate) fn nudge(&self, current: f32, strength: f32, rng: &mut Rng, root: i32) -> f32 {
        let wobble = rng.unit() * 2.0 - 1.0;
        match self {
            Spec::Fixed(_) => current,
            Spec::Range([lo, hi]) => tidy(
                (current + wobble * strength * (hi - lo)).clamp(lo.min(current), hi.max(current)),
            ),
            Spec::Steps { steps: [lo, hi] } => {
                let (lo, hi) = (i32::from(*lo), i32::from(*hi));
                let now = steps_of(current);
                let reach = ((strength * (hi - lo) as f32).round() as i32).max(1);
                let moved = now + (wobble * (reach as f32 + 0.5)).round() as i32;
                steps_value(moved.clamp(lo.min(now), hi.max(now)))
            }
            Spec::Pick { .. } | Spec::Octave { .. } | Spec::Root { .. } => self.sample(rng, root),
        }
    }

    /// True for pitch params that carry the patch key.
    pub(crate) fn is_key_bound(&self) -> bool {
        matches!(self, Spec::Octave { .. } | Spec::Root { .. })
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let unit = |v: f32| (0.0..=1.0).contains(&v);
        let ok = match self {
            Spec::Fixed(v) => unit(*v),
            Spec::Range([lo, hi]) => unit(*lo) && unit(*hi) && lo <= hi,
            Spec::Steps { steps: [lo, hi] } => 1 <= *lo && lo <= hi && i32::from(*hi) <= MAX_STEPS,
            Spec::Pick { pick } => !pick.is_empty() && pick.iter().all(|&v| unit(v)),
            Spec::Octave { octave: [lo, hi] } => -2 <= *lo && lo <= hi && *hi <= 1,
            Spec::Root { root } => *root,
        };
        if ok {
            Ok(())
        } else {
            Err(format!(
                "{self:?} is outside 0..=1, 1..=16 steps or -2..=1 octaves"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_round_trip_through_the_param_value() {
        for n in 1..=16 {
            assert_eq!(steps_of(steps_value(n)), n);
        }
    }

    #[test]
    fn octave_puts_the_root_on_the_pitch_grid() {
        // C at -1 octave: tune 0.5 - 12/48. A at -2 octaves: 0.5 - 15/48.
        assert!((octave_value(0, -1) - 0.25).abs() < 1e-5);
        assert!((octave_value(9, -2) - (0.5 - 15.0 / 48.0)).abs() < 1e-5);
    }

    #[test]
    fn root_is_the_key_note_in_twelfths() {
        let mut rng = Rng::new(1);
        let root = Spec::Root { root: true };
        // A is nine semitones above C: Fret's default root of 0.75.
        assert_eq!(root.sample(&mut rng, 9), 0.75);
        assert!(root.is_key_bound());
        assert!(Spec::Root { root: false }.validate().is_err());
    }

    #[test]
    fn samples_stay_inside_their_spec() {
        let mut rng = Rng::new(3);
        let range = Spec::Range([0.2, 0.4]);
        let steps = Spec::Steps { steps: [3, 6] };
        for _ in 0..200 {
            let v = range.sample(&mut rng, 0);
            assert!((0.2..=0.4).contains(&v));
            assert!((3..=6).contains(&steps_of(steps.sample(&mut rng, 0))));
            let n = range.nudge(v, 1.0, &mut rng, 0);
            assert!((0.2..=0.4).contains(&n));
        }
    }

    #[test]
    fn toml_forms_parse() {
        #[derive(Deserialize)]
        struct T {
            fixed: Spec,
            int: Spec,
            range: Spec,
            steps: Spec,
            pick: Spec,
            octave: Spec,
            root: Spec,
        }
        let t: T = toml::from_str(
            "fixed = 0.4\nint = 1\nrange = [0.1, 1]\nsteps = { steps = [5, 7] }\npick = { pick = [0, 0.5] }\noctave = { octave = [-2, -1] }\nroot = { root = true }",
        )
        .unwrap();
        assert_eq!(t.fixed, Spec::Fixed(0.4));
        assert_eq!(t.int, Spec::Fixed(1.0));
        assert_eq!(t.range, Spec::Range([0.1, 1.0]));
        assert_eq!(t.steps, Spec::Steps { steps: [5, 7] });
        assert_eq!(
            t.pick,
            Spec::Pick {
                pick: vec![0.0, 0.5]
            }
        );
        assert_eq!(t.octave, Spec::Octave { octave: [-2, -1] });
        assert_eq!(t.root, Spec::Root { root: true });
    }
}
