//! Breed: a child of two patches. The child is `a` with some params taken
//! from `b`, so every value in it is one of its parents'.

use kerf_core::{Module, ParamValue, Patch};
use kerf_dsp::Rng;

use crate::rulebook::Rulebook;
use crate::spec::Spec;
use crate::util::chance;

/// How a part's params are drawn from the two parents.
#[derive(Clone, Copy)]
enum Mode {
    A,
    B,
    Mix,
}

fn key_bound(rb: Option<&Rulebook>, m: &Module, p: &ParamValue) -> bool {
    rb.and_then(|rb| rb.slot(&m.id, &m.kind))
        .and_then(|s| s.params.get(&p.key))
        .is_some_and(Spec::is_key_bound)
}

fn counterpart<'b>(b: &'b Patch, m: &Module, p: &ParamValue) -> Option<&'b ParamValue> {
    b.module(&m.id)
        .filter(|bm| bm.kind == m.kind)
        .and_then(|bm| bm.param(&p.key))
}

pub(crate) fn cross(a: &Patch, b: &Patch, rb: Option<&Rulebook>, seed: u64) -> Patch {
    let mut rng = Rng::new(seed);
    let mut child = a.clone();

    // The key moves only if every key-bound pitch param can move with it.
    let key_from_b = b.key.is_some()
        && a.modules.iter().all(|m| {
            m.params.iter().all(|p| {
                !key_bound(rb, m, p) || (!a.is_locked(m, p) && counterpart(b, m, p).is_some())
            })
        })
        && chance(&mut rng, 0.5);
    if key_from_b {
        child.key.clone_from(&b.key);
    }
    if chance(&mut rng, 0.5) {
        child.tempo = b.tempo;
    }

    let modes: Vec<Mode> = a
        .parts
        .iter()
        .map(|_| match (rng.unit() * 3.0) as u8 {
            0 => Mode::A,
            1 => Mode::B,
            _ => Mode::Mix,
        })
        .collect();
    for (mi, m) in a.modules.iter().enumerate() {
        let mode = m
            .part
            .as_ref()
            .and_then(|id| a.parts.iter().position(|p| &p.id == id))
            .map_or(Mode::Mix, |i| modes[i]);
        for (pi, p) in m.params.iter().enumerate() {
            if a.is_locked(m, p) {
                continue;
            }
            let Some(theirs) = counterpart(b, m, p) else {
                continue;
            };
            let take = if key_bound(rb, m, p) {
                key_from_b
            } else {
                match mode {
                    Mode::A => false,
                    Mode::B => true,
                    Mode::Mix => chance(&mut rng, 0.5),
                }
            };
            if take {
                child.modules[mi].params[pi].value = theirs.value;
            }
        }
    }
    child
}
