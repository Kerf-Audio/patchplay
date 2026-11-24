//! Checks a parsed rulebook against the module catalogue, so a bad style file
//! fails at load rather than as a strange patch.

use std::collections::HashMap;

use kerf_core::{Catalogue, ModuleDescriptor, ModuleId, PartId, Patch};
use kerf_modules::Builtin;

use crate::rulebook::{CableRule, ModuleSlot, Rulebook};
use crate::util::key_root;

/// Who owns a module id, and the descriptors of every kind it may take.
struct Owner<'a> {
    part: Option<&'a PartId>,
    kinds: Vec<&'static ModuleDescriptor>,
}

pub(crate) fn check(rb: &Rulebook) -> Result<(), String> {
    let [lo, hi] = rb.tempo;
    if !(lo <= hi && Patch::TEMPO_RANGE.contains(&lo) && Patch::TEMPO_RANGE.contains(&hi)) {
        return Err(format!("tempo {lo}..{hi} is not a range inside 20..=300"));
    }
    if rb.keys.is_empty() {
        return Err("keys is empty".into());
    }
    for key in &rb.keys {
        if key_root(key).is_none() {
            return Err(format!(
                "key `{key}` must start with a note, e.g. `A minor`"
            ));
        }
    }
    if rb.parts.first().is_none_or(|p| p.chance < 1.0) {
        return Err("the first part must always appear (chance 1)".into());
    }

    let mut owners: HashMap<&ModuleId, Owner> = HashMap::new();
    for slot in &rb.modules {
        add_slot(&mut owners, slot, None)?;
    }
    for part in &rb.parts {
        if !(part.chance > 0.0 && part.chance <= 1.0) {
            return Err(format!("part `{}`: chance must be in (0, 1]", part.id));
        }
        for i in 0..part.layout_count() {
            let layout = part.layout(i);
            if layout.weight <= 0.0 {
                return Err(format!("part `{}`: weight must be above 0", part.id));
            }
            for (n, slot) in layout.modules.iter().enumerate() {
                if layout.modules[..n].iter().any(|s| s.id == slot.id) {
                    return Err(format!(
                        "module id `{}` is used twice in part `{}`",
                        slot.id, part.id
                    ));
                }
                add_slot(&mut owners, slot, Some(&part.id))?;
            }
        }
    }

    for rule in &rb.cables {
        check_cable(rule, &owners, |id| owners[id].part.is_none())?;
    }
    for part in &rb.parts {
        for i in 0..part.layout_count() {
            let layout = part.layout(i);
            // A layout may wire its own modules, shared ones and other parts'.
            let reachable = |id: &ModuleId| {
                owners[id].part != Some(&part.id) || layout.modules.iter().any(|s| &s.id == id)
            };
            for rule in layout.cables {
                check_cable(rule, &owners, reachable)?;
            }
        }
    }
    Ok(())
}

fn add_slot<'a>(
    owners: &mut HashMap<&'a ModuleId, Owner<'a>>,
    slot: &'a ModuleSlot,
    part: Option<&'a PartId>,
) -> Result<(), String> {
    let d = Builtin
        .get(&slot.kind)
        .ok_or_else(|| format!("module `{}`: no module kind `{}`", slot.id, slot.kind))?;
    for (key, spec) in &slot.params {
        if d.param(key.as_str()).is_none() {
            return Err(format!(
                "module `{}`: {} has no param `{key}`",
                slot.id, d.kind
            ));
        }
        spec.validate()
            .map_err(|e| format!("module `{}`, param `{key}`: {e}", slot.id))?;
    }
    let owner = owners.entry(&slot.id).or_insert_with(|| Owner {
        part,
        kinds: Vec::new(),
    });
    if owner.part != part || (part.is_none() && !owner.kinds.is_empty()) {
        return Err(format!("module id `{}` is used twice", slot.id));
    }
    owner.kinds.push(d);
    Ok(())
}

fn check_cable(
    rule: &CableRule,
    owners: &HashMap<&ModuleId, Owner>,
    reachable: impl Fn(&ModuleId) -> bool,
) -> Result<(), String> {
    if !(0.0..=1.0).contains(&rule.chance) {
        return Err("cable chance must be in 0..=1".into());
    }
    for (ends, output) in [(rule.from(), true), (rule.to(), false)] {
        for j in ends {
            let owner = owners
                .get(&j.module)
                .ok_or_else(|| format!("cable names unknown module `{}`", j.module))?;
            if !reachable(&j.module) {
                return Err(format!(
                    "cable names `{}`, which this cable's layout does not hold",
                    j.module
                ));
            }
            for d in &owner.kinds {
                let found = if output {
                    d.output(j.jack.as_str()).is_some()
                } else {
                    d.input(j.jack.as_str()).is_some()
                };
                if !found {
                    return Err(format!(
                        "cable names {j}, but {} has no {} `{}`",
                        d.kind,
                        if output { "output" } else { "input" },
                        j.jack
                    ));
                }
            }
        }
    }
    Ok(())
}
