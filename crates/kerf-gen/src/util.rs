//! Small seeded helpers shared by random, evolve and breed.

use kerf_dsp::Rng;

/// Seed for attempt `n` of a generation. Attempt 0 is the seed itself, so a
/// patch that passes first time records the seed the caller gave.
pub(crate) fn derive_seed(seed: u64, attempt: u32) -> u64 {
    if attempt == 0 {
        seed
    } else {
        Rng::new(seed.wrapping_add(u64::from(attempt).wrapping_mul(0x9E37_79B9_7F4A_7C15)))
            .next_u64()
    }
}

/// A uniform index into a slice of `len` (> 0).
pub(crate) fn index(rng: &mut Rng, len: usize) -> usize {
    ((rng.unit() * len as f32) as usize).min(len - 1)
}

/// A uniform integer in `lo..=hi`.
pub(crate) fn int_between(rng: &mut Rng, lo: i32, hi: i32) -> i32 {
    (lo + (rng.unit() * (hi - lo + 1) as f32) as i32).min(hi)
}

/// `true` with probability `p`; draws nothing when `p` is certain.
pub(crate) fn chance(rng: &mut Rng, p: f32) -> bool {
    p >= 1.0 || rng.unit() < p
}

/// Index chosen by weight, skipping `exclude`.
pub(crate) fn weighted(rng: &mut Rng, weights: &[f32], exclude: Option<usize>) -> usize {
    let total: f32 = weights
        .iter()
        .enumerate()
        .filter(|&(i, _)| Some(i) != exclude)
        .map(|(_, w)| w)
        .sum();
    let mut at = rng.unit() * total;
    let mut last = 0;
    for (i, w) in weights.iter().enumerate() {
        if Some(i) == exclude {
            continue;
        }
        last = i;
        if at < *w {
            return i;
        }
        at -= w;
    }
    last
}

/// Pitch class of a key such as `"A minor"`: 0 = C … 11 = B.
pub(crate) fn key_root(key: &str) -> Option<i32> {
    Some(match key.split_whitespace().next()? {
        "C" => 0,
        "C#" | "Db" => 1,
        "D" => 2,
        "D#" | "Eb" => 3,
        "E" => 4,
        "F" => 5,
        "F#" | "Gb" => 6,
        "G" => 7,
        "G#" | "Ab" => 8,
        "A" => 9,
        "A#" | "Bb" => 10,
        "B" => 11,
        _ => return None,
    })
}
