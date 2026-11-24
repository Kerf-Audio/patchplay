//! Editing a running engine: param and tempo changes that need no rebuild,
//! and the hand-over of module state to a rebuilt engine.
//!
//! None of this allocates except `diff`, which runs on the editing thread.

use std::mem;

use kerf_core::{Catalogue, Patch};
use kerf_modules::Builtin;

use crate::{Engine, EngineError, is_muted, param_value};

/// Where a param lives in an engine: the module's position in the patch and
/// the param's position in its descriptor. Valid for any engine built from a
/// patch with the same modules in the same order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamSlot {
    node: usize,
    param: usize,
}

/// An edit a running engine takes without being rebuilt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Change {
    /// A normalised param value.
    Param(ParamSlot, f32),
    /// Beats per minute.
    Tempo(f32),
    /// A part's level (`0..=1`), by the part's position in the patch.
    PartLevel(usize, f32),
}

/// What changed from `old` to `new`, when an engine built from `old` can take
/// it as-is: only param values, part levels and tempo differ. `None` means the
/// graph differs (modules, kinds, cables, muting, sample files or style seed)
/// and needs a rebuild.
pub fn diff(old: &Patch, new: &Patch) -> Option<Vec<Change>> {
    let seed = |p: &Patch| p.style.as_ref().map(|s| s.seed);
    if old.cables != new.cables || old.modules.len() != new.modules.len() || seed(old) != seed(new)
    {
        return None;
    }
    let mut changes = Vec::new();
    for (node, (a, b)) in old.modules.iter().zip(&new.modules).enumerate() {
        if a.id != b.id
            || a.kind != b.kind
            || a.sample != b.sample
            || a.part != b.part
            || is_muted(old, a) != is_muted(new, b)
        {
            return None;
        }
        let desc = Builtin.get(&a.kind)?;
        for (param, d) in desc.params.iter().enumerate() {
            let value = param_value(b, d);
            if param_value(a, d) != value {
                changes.push(Change::Param(ParamSlot { node, param }, value));
            }
        }
    }
    let same_parts = old.parts.len() == new.parts.len()
        && old.parts.iter().zip(&new.parts).all(|(a, b)| a.id == b.id);
    if !same_parts {
        return None;
    }
    for (part, (a, b)) in old.parts.iter().zip(&new.parts).enumerate() {
        if a.level != b.level {
            changes.push(Change::PartLevel(part, b.level));
        }
    }
    if old.tempo != new.tempo {
        changes.push(Change::Tempo(new.tempo));
    }
    Some(changes)
}

impl Engine {
    /// Where `param` of `module` lives in this engine.
    pub fn slot(&self, module: &str, param: &str) -> Result<ParamSlot, EngineError> {
        let (node, n) = self
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.id.as_str() == module)
            .ok_or_else(|| EngineError::UnknownModule(module.to_owned()))?;
        let param_index = n
            .desc
            .params
            .iter()
            .position(|p| p.key == param)
            .ok_or_else(|| EngineError::UnknownParam {
                kind: n.desc.kind,
                param: param.to_owned(),
            })?;
        Ok(ParamSlot {
            node,
            param: param_index,
        })
    }

    /// Sets a param from its normalised value (clamped to `0..=1`).
    pub fn set_param(&mut self, module: &str, param: &str, value: f32) -> Result<(), EngineError> {
        let slot = self.slot(module, param)?;
        self.apply(Change::Param(slot, value));
        Ok(())
    }

    /// Takes an edit from `diff`. The change must come from a patch pair whose
    /// old side built this engine; no allocation.
    pub fn apply(&mut self, change: Change) {
        match change {
            Change::Param(slot, value) => {
                if let Some(p) = self
                    .nodes
                    .get_mut(slot.node)
                    .and_then(|n| n.params.get_mut(slot.param))
                {
                    *p = value.clamp(0.0, 1.0);
                }
            }
            Change::Tempo(bpm) => self.tempo = bpm,
            Change::PartLevel(part, level) => {
                if let Some(gain) = self.part_gains.get_mut(part) {
                    gain.set(level);
                }
            }
        }
    }

    /// Takes over the running state (oscillator phases, envelopes, delay
    /// tails, sequencer positions, held outputs) of every module in `old` with
    /// the same id and kind, so a rebuilt engine carries on without a click.
    /// `old` is left holding the fresh state of this engine's modules, to be
    /// dropped. Does nothing when the sample rates differ. No allocation.
    pub fn take_state(&mut self, old: &mut Engine) {
        if self.sample_rate != old.sample_rate {
            return;
        }
        let Engine { nodes, values, .. } = self;
        for node in nodes.iter_mut() {
            let Some(prev) = old.nodes.iter_mut().find(|p| {
                p.id == node.id && p.desc.kind == node.desc.kind && p.sample == node.sample
            }) else {
                continue;
            };
            mem::swap(&mut node.dsp, &mut prev.dsp);
            values[node.outputs.clone()].copy_from_slice(&old.values[prev.outputs.clone()]);
        }
    }
}
