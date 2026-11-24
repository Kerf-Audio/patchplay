//! Putting rulebook modules and cables into a patch.

use kerf_core::{Cable, Catalogue, JackRef, Module, ModuleId, PartId, Patch};
use kerf_dsp::Rng;
use kerf_modules::Builtin;

use crate::rulebook::{CableRule, ModuleSlot};
use crate::util::index;

/// Adds one module per slot at the end of `part`'s modules, with every param
/// at its default unless the slot's spec sets it.
pub(crate) fn add_modules(
    patch: &mut Patch,
    slots: &[ModuleSlot],
    part: Option<&PartId>,
    root: i32,
    rng: &mut Rng,
) {
    for slot in slots {
        let d = Builtin
            .get(&slot.kind)
            .expect("rulebook kinds are validated");
        let mut params = d.default_params();
        for p in &mut params {
            if let Some(spec) = slot.params.get(&p.key) {
                p.value = spec.sample(rng, root);
            }
        }
        patch.modules.insert(
            patch.insertion_index(part),
            Module {
                id: slot.id.clone(),
                kind: d.kind.to_owned(),
                part: part.cloned(),
                params,
                locked: false,
                sample: None,
            },
        );
    }
}

/// Every cable the rule allows whose two modules are in the patch.
pub(crate) fn options(patch: &Patch, rule: &CableRule) -> Vec<Cable> {
    let live = |j: &&JackRef| patch.module(&j.module).is_some();
    rule.from()
        .iter()
        .filter(live)
        .flat_map(|from| {
            rule.to().iter().filter(live).map(move |to| Cable {
                from: from.clone(),
                to: to.clone(),
            })
        })
        .collect()
}

/// Adds the cable a rule yields, unless its chance says no, an end is absent
/// or the patch already has it.
pub(crate) fn add_cable(patch: &mut Patch, rule: &CableRule, rng: &mut Rng) {
    if rule.chance < 1.0 && rng.unit() >= rule.chance {
        return;
    }
    let mut options = options(patch, rule);
    options.retain(|c| !patch.cables.contains(c));
    if !options.is_empty() {
        let pick = index(rng, options.len());
        patch.cables.push(options.swap_remove(pick));
    }
}

pub(crate) fn add_cables(patch: &mut Patch, rules: &[CableRule], rng: &mut Rng) {
    for rule in rules {
        add_cable(patch, rule, rng);
    }
}

/// Removes a module and every cable and scene value touching it.
pub(crate) fn remove_module(patch: &mut Patch, id: &ModuleId) {
    patch.modules.retain(|m| &m.id != id);
    patch
        .cables
        .retain(|c| &c.from.module != id && &c.to.module != id);
    for values in patch.scenes.values_mut() {
        values.retain(|v| &v.module != id);
    }
}
