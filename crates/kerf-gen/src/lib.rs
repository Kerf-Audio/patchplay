//! Style rulebooks and the generator: `random`, `evolve`, `breed` and
//! `safe_check`.
//!
//! Everything is seeded with `kerf_dsp::Rng` and `hash`, so a seed gives the
//! same patch text on every platform. Each generator retries with derived
//! seeds until `safe_check` passes, up to `ATTEMPTS` times. For `random` the
//! seed that passed is the one stored in the patch's `style` line, because
//! the engine seeds its modules from it. `evolve` and `breed` keep the
//! parent's style seed, so sequencer note patterns survive them.

mod breed;
mod build;
mod check;
mod mutate;
mod place;
mod rulebook;
mod spec;
mod styles;
mod util;
mod validate;

use kerf_core::Patch;

pub use check::{CheckReport, safe_check};
pub use rulebook::{Rulebook, RulebookError};
pub use styles::{PartInfo, StyleInfo, styles};

/// How many derived seeds a generation tries before giving up.
pub const ATTEMPTS: u32 = 16;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GenError {
    #[error("no style `{0}`; `kerf styles` lists them")]
    UnknownStyle(String),
    #[error("strength {0} is outside 0..=1")]
    BadStrength(f32),
    #[error("no safe patch in {attempts} attempts; last check: {}", last.problems.join("; "))]
    NoSafePatch {
        attempts: u32,
        last: Box<CheckReport>,
    },
}

/// The first of `ATTEMPTS` derived seeds whose patch passes `safe_check`.
fn first_safe(seed: u64, mut make: impl FnMut(u64) -> Patch) -> Result<Patch, GenError> {
    let mut last = None;
    for attempt in 0..ATTEMPTS {
        let patch = make(util::derive_seed(seed, attempt));
        let report = safe_check(&patch);
        if report.pass {
            return Ok(patch);
        }
        last = Some(report);
    }
    Err(GenError::NoSafePatch {
        attempts: ATTEMPTS,
        last: Box::new(last.expect("ATTEMPTS is above zero")),
    })
}

/// A new patch in the named style.
pub fn random(style: &str, seed: u64) -> Result<Patch, GenError> {
    let rb = styles::rulebook(style).ok_or_else(|| GenError::UnknownStyle(style.to_owned()))?;
    first_safe(seed, |s| build::build(rb, s))
}

/// `patch` drifted by `strength` (0..=1). Locked modules, params and parts
/// are untouched. A patch with no known style drifts every param of every
/// module except Finish, Router and Tally.
pub fn evolve(patch: &Patch, strength: f32, seed: u64) -> Result<Patch, GenError> {
    if !(0.0..=1.0).contains(&strength) {
        return Err(GenError::BadStrength(strength));
    }
    let rb = rulebook_of(patch);
    first_safe(seed, |s| mutate::mutate(patch, rb, strength, s))
}

/// A child of `a` and `b`: `a` with some params, the tempo and the key taken
/// from `b`. Locks in `a` hold.
pub fn breed(a: &Patch, b: &Patch, seed: u64) -> Result<Patch, GenError> {
    let rb = rulebook_of(a);
    first_safe(seed, |s| breed::cross(a, b, rb, s))
}

fn rulebook_of(patch: &Patch) -> Option<&'static Rulebook> {
    patch.style.as_ref().and_then(|s| styles::rulebook(&s.name))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod key_tests;
