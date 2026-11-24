//! Evolve: drift a patch by a strength, leaving locked things alone.
//!
//! Three kinds of change, all seeded: params drift inside their rulebook
//! specs, pooled and optional cables move, and a part may swap to another
//! layout. Tempo and the style seed stay put, so the sequencer patterns that
//! come from the seed keep their notes.

use kerf_core::{Key, Module, ModuleId, Part, Patch};
use kerf_dsp::Rng;

use crate::place::{add_cable, add_cables, add_modules, options, remove_module};
use crate::rulebook::{CableRule, PartRule, Rulebook};
use crate::spec::{FULL_RANGE, Spec};
use crate::util::{chance, index, key_root};

/// Without a rulebook, these kinds keep their params: the master chain and
/// the clock, where drifting would only break the patch.
const STEADY_KINDS: [&str; 3] = ["Finish", "Router", "Tally"];

pub(crate) fn mutate(patch: &Patch, rb: Option<&Rulebook>, strength: f32, seed: u64) -> Patch {
    if strength == 0.0 {
        return patch.clone();
    }
    let mut rng = Rng::new(seed);
    let mut child = patch.clone();
    let root = patch.key.as_deref().and_then(key_root).unwrap_or(0);
    drift_params(&mut child, rb, strength, root, &mut rng);
    if let Some(rb) = rb {
        swap_layouts(&mut child, rb, strength, root, &mut rng);
        rewire(&mut child, rb, strength, &mut rng);
    }
    child
}

fn spec_for<'a>(rb: Option<&'a Rulebook>, m: &Module, key: &Key) -> Option<&'a Spec> {
    match rb {
        Some(rb) => rb.slot(&m.id, &m.kind)?.params.get(key),
        None => (!STEADY_KINDS.contains(&m.kind.as_str())).then_some(&FULL_RANGE),
    }
}

fn drift_params(patch: &mut Patch, rb: Option<&Rulebook>, strength: f32, root: i32, rng: &mut Rng) {
    let mut moves = Vec::new();
    for (mi, m) in patch.modules.iter().enumerate() {
        for (pi, p) in m.params.iter().enumerate() {
            if patch.is_locked(m, p) {
                continue;
            }
            let Some(spec) = spec_for(rb, m, &p.key) else {
                continue;
            };
            if chance(rng, 0.1 + 0.9 * strength) {
                moves.push((mi, pi, spec.nudge(p.value, strength, rng, root)));
            }
        }
    }
    for (mi, pi, value) in moves {
        patch.modules[mi].params[pi].value = value;
    }
}

/// True when the module, or the part it sits in, is locked.
fn module_frozen(patch: &Patch, id: &ModuleId) -> bool {
    patch.module(id).is_none_or(|m| {
        m.locked
            || m.part
                .as_ref()
                .and_then(|p| patch.part(p))
                .is_some_and(|p| p.locked)
    })
}

/// True when anything inside the part is locked: swapping its layout would
/// throw that away.
fn part_pinned(patch: &Patch, part: &Part) -> bool {
    part.locked
        || patch
            .modules
            .iter()
            .filter(|m| m.part.as_ref() == Some(&part.id))
            .any(|m| m.locked || m.params.iter().any(|p| p.locked))
}

fn swap_layouts(patch: &mut Patch, rb: &Rulebook, strength: f32, root: i32, rng: &mut Rng) {
    for rule in &rb.parts {
        if rule.layout_count() < 2 {
            continue;
        }
        let Some(part) = patch.part(&rule.id) else {
            continue;
        };
        if part_pinned(patch, part) {
            continue;
        }
        let Some(current) = rule.current_layout(patch) else {
            continue;
        };
        if !chance(rng, 0.4 * strength * strength) {
            continue;
        }
        let next = rule.pick_layout(rng, Some(current));
        replace_layout(patch, rb, rule, next, root, rng);
    }
}

fn replace_layout(
    patch: &mut Patch,
    rb: &Rulebook,
    rule: &PartRule,
    next: usize,
    root: i32,
    rng: &mut Rng,
) {
    let old: Vec<ModuleId> = patch
        .modules
        .iter()
        .filter(|m| m.part.as_ref() == Some(&rule.id))
        .map(|m| m.id.clone())
        .collect();
    for id in &old {
        remove_module(patch, id);
    }
    let layout = rule.layout(next);
    add_modules(patch, layout.modules, Some(&rule.id), root, rng);
    add_cables(patch, layout.cables, rng);
    // Other parts' cables into this part's modules went with the old ones.
    let entering = |c: &kerf_core::JackRef| layout.modules.iter().any(|s| s.id == c.module);
    for other in rb.parts.iter().filter(|p| p.id != rule.id) {
        if patch.part(&other.id).is_none() {
            continue;
        }
        let Some(i) = other.current_layout(patch) else {
            continue;
        };
        for cable_rule in other.layout(i).cables {
            if cable_rule
                .from()
                .iter()
                .chain(cable_rule.to())
                .any(entering)
            {
                add_cable(patch, cable_rule, rng);
            }
        }
    }
}

/// The cable rules in force: shared ones, then each present part's current layout.
fn live_rules<'a>(patch: &Patch, rb: &'a Rulebook) -> Vec<&'a CableRule> {
    let mut rules: Vec<_> = rb.cables.iter().collect();
    for part in &rb.parts {
        if patch.part(&part.id).is_none() {
            continue;
        }
        if let Some(i) = part.current_layout(patch) {
            rules.extend(part.layout(i).cables.iter());
        }
    }
    rules
}

/// Moves pooled cables to another option and toggles optional ones, never
/// touching a cable whose modules are locked.
fn rewire(patch: &mut Patch, rb: &Rulebook, strength: f32, rng: &mut Rng) {
    for rule in live_rules(patch, rb).into_iter().filter(|r| r.is_choice()) {
        let present = patch
            .cables
            .iter()
            .position(|c| rule.matches(&c.from, &c.to));
        let mut candidates = options(patch, rule);
        candidates.retain(|c| {
            !patch.cables.contains(c)
                && !module_frozen(patch, &c.from.module)
                && !module_frozen(patch, &c.to.module)
        });
        match present {
            Some(at) => {
                let c = &patch.cables[at];
                if module_frozen(patch, &c.from.module) || module_frozen(patch, &c.to.module) {
                    continue;
                }
                if rule.chance < 1.0 && chance(rng, 0.3 * strength) {
                    patch.cables.remove(at);
                } else if !candidates.is_empty() && chance(rng, 0.5 * strength) {
                    patch.cables[at] = candidates.swap_remove(index(rng, candidates.len()));
                }
            }
            None => {
                if rule.chance < 1.0 && !candidates.is_empty() && chance(rng, 0.3 * strength) {
                    patch
                        .cables
                        .push(candidates.swap_remove(index(rng, candidates.len())));
                }
            }
        }
    }
}
