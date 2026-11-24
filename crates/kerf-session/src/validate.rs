//! Checks a whole patch against the catalogue when it is opened, and brings it
//! into canonical form (catalogue kind names, descriptor param order, modules
//! grouped by part).

use std::collections::HashSet;

use kerf_core::{Catalogue, ParamValue, Part, Patch};

use crate::SessionError as E;
use crate::edit::check_cable;

pub(crate) fn normalise(cat: &impl Catalogue, p: &mut Patch) -> Result<(), E> {
    if !Patch::TEMPO_RANGE.contains(&p.tempo) {
        return Err(E::OutOfRange {
            what: "tempo",
            value: p.tempo,
            range: "20..=300 bpm",
        });
    }
    if let Some(key) = p.key.as_deref().filter(|k| !Patch::valid_key(k)) {
        return Err(E::BadRequest(format!(
            "key {key:?} must not be empty, padded with spaces, or contain quotes or line breaks"
        )));
    }
    if let Some(tag) = p.tags.iter().find(|t| !Patch::valid_tag(t)) {
        return Err(E::BadRequest(format!(
            "tag {tag:?} must be lowercase letters, digits and `-`, starting with a letter"
        )));
    }

    let mut part_ids = HashSet::new();
    for part in &p.parts {
        if !part_ids.insert(&part.id) {
            return Err(E::DuplicatePart(part.id.clone()));
        }
        if !Part::valid_name(&part.name) {
            return Err(E::BadName(part.name.clone()));
        }
        if !(0.0..=1.0).contains(&part.level) {
            return Err(E::OutOfRange {
                what: "part level",
                value: part.level,
                range: "0..=1",
            });
        }
    }

    let mut module_ids = HashSet::new();
    for m in &mut p.modules {
        if !module_ids.insert(m.id.clone()) {
            return Err(E::DuplicateModule(m.id.clone()));
        }
        if let Some(part) = &m.part {
            if !part_ids.contains(part) {
                return Err(E::UnknownPart(part.clone()));
            }
        }
        let d = cat
            .get(&m.kind)
            .ok_or_else(|| E::UnknownKind(m.kind.clone()))?;
        m.kind = d.kind.to_owned();
        if let Some(path) = &m.sample {
            if !d.takes_sample() {
                return Err(E::NoSample(m.id.clone(), m.kind.clone()));
            }
            if !kerf_core::Module::valid_sample_path(path) {
                return Err(E::BadSamplePath(path.clone()));
            }
        }
        let mut params = d.default_params();
        for given in m.params.drain(..) {
            check_unit(given.value)?;
            let slot = params
                .iter_mut()
                .find(|v| v.key == given.key)
                .ok_or_else(|| E::UnknownParam {
                    kind: d.kind.to_owned(),
                    param: given.key.clone(),
                })?;
            *slot = ParamValue {
                locked: given.locked,
                ..given
            };
        }
        m.params = params;
    }

    let rank = |m: &kerf_core::Module| {
        m.part.as_ref().map_or(0, |id| {
            1 + p.parts.iter().position(|x| &x.id == id).unwrap_or(0)
        })
    };
    let mut modules = std::mem::take(&mut p.modules);
    modules.sort_by_key(|m| rank(m));
    p.modules = modules;

    for (i, c) in p.cables.iter().enumerate() {
        check_cable(cat, p, &c.from, &c.to)?;
        if p.cables[..i].contains(c) {
            return Err(E::DuplicateCable(c.from.clone(), c.to.clone()));
        }
    }

    for values in p.scenes.values() {
        for v in values {
            check_unit(v.value)?;
            let m = p
                .module(&v.module)
                .ok_or_else(|| E::UnknownModule(v.module.clone()))?;
            if m.param(&v.param).is_none() {
                return Err(E::UnknownParam {
                    kind: m.kind.clone(),
                    param: v.param.clone(),
                });
            }
        }
    }
    Ok(())
}

fn check_unit(value: f32) -> Result<(), E> {
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(E::OutOfRange {
            what: "param value",
            value,
            range: "0..=1",
        })
    }
}
