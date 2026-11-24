//! Applies one edit command to a patch. Callers pass a scratch copy, so an
//! error part-way leaves the real patch untouched.

use kerf_core::text::{Severity, diagnose, parse};
use kerf_core::{
    Cable, Catalogue, Command, JackRef, Key, LockTarget, Module, ModuleDescriptor, ModuleId,
    ParamValue, Part, PartId, Patch, SceneValue,
};

use crate::SessionError as E;
use crate::autopatch::{self, PlannedCable};
use crate::validate;

pub(crate) fn edit(
    cat: &impl Catalogue,
    p: &mut Patch,
    cmd: Command,
) -> Result<Vec<PlannedCable>, E> {
    let mut planned = Vec::new();
    match cmd {
        Command::AddPart { id, name, colour } => {
            if p.part(&id).is_some() {
                return Err(E::DuplicatePart(id));
            }
            if !Part::valid_name(&name) {
                return Err(E::BadName(name));
            }
            p.parts.push(Part {
                id,
                name,
                colour,
                locked: false,
                muted: false,
                level: Part::unity(),
            });
        }
        Command::AddModule {
            kind,
            id,
            part,
            auto_patch,
            near,
        } => {
            let d = cat.get(&kind).ok_or(E::UnknownKind(kind))?;
            if let Some(part) = &part {
                p.part(part).ok_or_else(|| E::UnknownPart(part.clone()))?;
            }
            if let Some(near) = &near {
                p.module(near)
                    .ok_or_else(|| E::UnknownModule(near.clone()))?;
            }
            let id = match id {
                Some(id) if p.module(&id).is_some() => return Err(E::DuplicateModule(id)),
                Some(id) => id,
                None => fresh_id(p, d),
            };
            let at = p.insertion_index(part.as_ref());
            p.modules.insert(
                at,
                Module {
                    id: id.clone(),
                    kind: d.kind.to_owned(),
                    part,
                    params: d.default_params(),
                    locked: false,
                    sample: None,
                },
            );
            if auto_patch || near.is_some() {
                planned = autopatch::plan(cat, p, &id, near.as_ref(), false)?;
            }
        }
        Command::RemoveModule { id } => {
            let i = p
                .modules
                .iter()
                .position(|m| m.id == id)
                .ok_or_else(|| E::UnknownModule(id.clone()))?;
            p.modules.remove(i);
            p.cables
                .retain(|c| c.from.module != id && c.to.module != id);
            for values in p.scenes.values_mut() {
                values.retain(|v| v.module != id);
            }
        }
        Command::SetParam {
            module,
            param,
            value,
        } => {
            if !value.is_finite() {
                return Err(E::OutOfRange {
                    what: "param value",
                    value,
                    range: "0..=1",
                });
            }
            param_mut(p, &module, &param)?.value = value.clamp(0.0, 1.0);
        }
        Command::SetParams { module, params } => {
            // Check every key and value before changing any, so an error leaves the patch whole.
            for set in &params {
                if !set.value.is_finite() {
                    return Err(E::OutOfRange {
                        what: "param value",
                        value: set.value,
                        range: "0..=1",
                    });
                }
                param_mut(p, &module, &set.param)?;
            }
            for set in params {
                param_mut(p, &module, &set.param)?.value = set.value.clamp(0.0, 1.0);
            }
        }
        Command::SetSample { module, path } => {
            let m = module_mut(p, &module)?;
            let d = cat
                .get(&m.kind)
                .ok_or_else(|| E::UnknownKind(m.kind.clone()))?;
            if !d.takes_sample() {
                return Err(E::NoSample(module, m.kind.clone()));
            }
            if let Some(path) = &path {
                if !Module::valid_sample_path(path) {
                    return Err(E::BadSamplePath(path.clone()));
                }
            }
            m.sample = path;
        }
        Command::Connect { from, to } => {
            check_cable(cat, p, &from, &to)?;
            let cable = Cable { from, to };
            if p.cables.contains(&cable) {
                return Err(E::DuplicateCable(cable.from, cable.to));
            }
            p.cables.push(cable);
        }
        Command::Disconnect { from, to } => {
            let i = p
                .cables
                .iter()
                .position(|c| c.from == from && c.to == to)
                .ok_or(E::NoSuchCable(from, to))?;
            p.cables.remove(i);
        }
        Command::Lock { target, locked } => match target {
            LockTarget::Module { module } => module_mut(p, &module)?.locked = locked,
            LockTarget::Param { module, param } => param_mut(p, &module, &param)?.locked = locked,
            LockTarget::Part { part } => part_mut(p, &part)?.locked = locked,
        },
        Command::Mute { part, muted } => part_mut(p, &part)?.muted = muted,
        Command::SetPartLevel { part, level } => {
            if !(0.0..=1.0).contains(&level) {
                return Err(E::OutOfRange {
                    what: "part level",
                    value: level,
                    range: "0.0..=1.0",
                });
            }
            part_mut(p, &part)?.level = level;
        }
        Command::SetTempo { bpm } => {
            if !Patch::TEMPO_RANGE.contains(&bpm) {
                return Err(E::OutOfRange {
                    what: "tempo",
                    value: bpm,
                    range: "20..=300 bpm",
                });
            }
            p.tempo = bpm;
        }
        Command::StoreScene { scene } => {
            let snapshot = p
                .modules
                .iter()
                .flat_map(|m| {
                    m.params.iter().map(|v| SceneValue {
                        module: m.id.clone(),
                        param: v.key.clone(),
                        value: v.value,
                    })
                })
                .collect();
            p.scenes.insert(scene, snapshot);
        }
        Command::ClearScene { scene } => {
            p.scenes.remove(&scene).ok_or(E::EmptyScene(scene))?;
        }
        Command::Morph { a, b, t } => {
            if !(0.0..=1.0).contains(&t) {
                return Err(E::OutOfRange {
                    what: "morph position",
                    value: t,
                    range: "0..=1",
                });
            }
            let sa = p.scenes.get(&a).ok_or(E::EmptyScene(a))?;
            let sb = p.scenes.get(&b).ok_or(E::EmptyScene(b))?;
            let mut changes = Vec::new();
            for va in sa {
                let Some(vb) = sb
                    .iter()
                    .find(|v| v.module == va.module && v.param == va.param)
                else {
                    continue;
                };
                let Some(mi) = p.modules.iter().position(|m| m.id == va.module) else {
                    continue;
                };
                let m = &p.modules[mi];
                let Some(pi) = m.params.iter().position(|v| v.key == va.param) else {
                    continue;
                };
                if !p.is_locked(m, &m.params[pi]) {
                    changes.push((mi, pi, va.value + (vb.value - va.value) * t));
                }
            }
            for (mi, pi, value) in changes {
                p.modules[mi].params[pi].value = value;
            }
        }
        Command::AutoPatch { module, near } => {
            planned = autopatch::plan(cat, p, &module, near.as_ref(), false)?;
        }
        Command::MoveModule {
            module,
            part,
            index,
        } => {
            if let Some(part) = &part {
                p.part(part).ok_or_else(|| E::UnknownPart(part.clone()))?;
            }
            let from = p
                .modules
                .iter()
                .position(|m| m.id == module)
                .ok_or_else(|| E::UnknownModule(module.clone()))?;
            let mut moved = p.modules.remove(from);
            moved.part = part.clone();
            // The modules of the target part are the run just before `end`.
            let end = p.insertion_index(part.as_ref());
            let len = p.modules[..end]
                .iter()
                .rev()
                .take_while(|m| m.part == part)
                .count();
            p.modules
                .insert(end - len + index.unwrap_or(len).min(len), moved);
        }
        Command::ReplaceText { text } => {
            let first_error = diagnose(cat, &text)
                .into_iter()
                .find(|d| d.severity == Severity::Error);
            if let Some(d) = first_error {
                return Err(E::BadText(format!(
                    "line {}, column {}: {}",
                    d.line, d.col, d.message
                )));
            }
            let mut patch = parse(&text).map_err(|e| E::BadText(e.to_string()))?;
            validate::normalise(cat, &mut patch)?;
            *p = patch;
        }
        Command::Random { .. }
        | Command::Evolve { .. }
        | Command::Breed { .. }
        | Command::Checkout { .. }
        | Command::Undo
        | Command::Redo => unreachable!("handled by Session::apply"),
    }
    Ok(planned)
}

/// `plane1`, `plane2`, …: the first free id for the kind.
fn fresh_id(p: &Patch, d: &ModuleDescriptor) -> ModuleId {
    let stem = d.kind.to_ascii_lowercase();
    (1..)
        .map(|n| ModuleId::new(format!("{stem}{n}")).expect("kinds are ASCII letters"))
        .find(|id| p.module(id).is_none())
        .expect("an unused id exists")
}

pub(crate) fn check_cable(
    cat: &impl Catalogue,
    p: &Patch,
    from: &JackRef,
    to: &JackRef,
) -> Result<(), E> {
    let desc = |id: &ModuleId| -> Result<&'static ModuleDescriptor, E> {
        let m = p.module(id).ok_or_else(|| E::UnknownModule(id.clone()))?;
        cat.get(&m.kind)
            .ok_or_else(|| E::UnknownKind(m.kind.clone()))
    };
    if desc(&from.module)?.output(from.jack.as_str()).is_none() {
        return Err(E::NotAnOutput(from.clone()));
    }
    if desc(&to.module)?.input(to.jack.as_str()).is_none() {
        return Err(E::NotAnInput(to.clone()));
    }
    Ok(())
}

fn module_mut<'p>(p: &'p mut Patch, id: &ModuleId) -> Result<&'p mut Module, E> {
    p.module_mut(id).ok_or_else(|| E::UnknownModule(id.clone()))
}

fn part_mut<'p>(p: &'p mut Patch, id: &PartId) -> Result<&'p mut Part, E> {
    p.part_mut(id).ok_or_else(|| E::UnknownPart(id.clone()))
}

fn param_mut<'p>(
    p: &'p mut Patch,
    module: &ModuleId,
    param: &Key,
) -> Result<&'p mut ParamValue, E> {
    let m = module_mut(p, module)?;
    let kind = m.kind.clone();
    m.param_mut(param).ok_or_else(|| E::UnknownParam {
        kind,
        param: param.clone(),
    })
}
