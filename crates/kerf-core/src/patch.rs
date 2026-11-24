//! The patch: parts, module instances, cables and scenes.
//!
//! Canonical order: modules without a part come first, then each part's
//! modules in part order. The text format prints in this order and the
//! session keeps it, so `parse(print(p)) == p`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::id::{Colour, JackRef, Key, ModuleId, PartId, Scene};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Patch {
    pub tempo: f32,
    pub key: Option<String>,
    pub style: Option<StyleRef>,
    /// Category words such as `four-four` or `abstract`; the starting-point
    /// browser groups by them. Not read by the engine.
    #[serde(default)]
    pub tags: Vec<String>,
    pub parts: Vec<Part>,
    pub modules: Vec<Module>,
    pub cables: Vec<Cable>,
    pub scenes: BTreeMap<Scene, Vec<SceneValue>>,
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            tempo: 120.0,
            key: None,
            style: None,
            tags: Vec::new(),
            parts: Vec::new(),
            modules: Vec::new(),
            cables: Vec::new(),
            scenes: BTreeMap::new(),
        }
    }
}

impl Patch {
    pub const FORMAT_VERSION: u32 = 1;
    pub const TEMPO_RANGE: std::ops::RangeInclusive<f32> = 20.0..=300.0;

    pub fn module(&self, id: &ModuleId) -> Option<&Module> {
        self.modules.iter().find(|m| &m.id == id)
    }

    pub fn module_mut(&mut self, id: &ModuleId) -> Option<&mut Module> {
        self.modules.iter_mut().find(|m| &m.id == id)
    }

    pub fn part(&self, id: &PartId) -> Option<&Part> {
        self.parts.iter().find(|p| &p.id == id)
    }

    /// Keys are the rest of a `key` line: not empty, no spaces at either end,
    /// no double quotes (the line is split into words first) or line breaks.
    pub fn valid_key(key: &str) -> bool {
        !key.is_empty() && key == key.trim() && !key.contains(['"', '\n', '\r'])
    }

    /// Tags are lower-case letters, digits and `-`, starting with a letter.
    pub fn valid_tag(tag: &str) -> bool {
        tag.starts_with(|c: char| c.is_ascii_lowercase())
            && tag
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }

    pub fn part_mut(&mut self, id: &PartId) -> Option<&mut Part> {
        self.parts.iter_mut().find(|p| &p.id == id)
    }

    /// Where a new module in `part` goes to keep canonical order.
    pub fn insertion_index(&self, part: Option<&PartId>) -> usize {
        let rank = |p: Option<&PartId>| match p {
            None => 0,
            Some(id) => {
                1 + self
                    .parts
                    .iter()
                    .position(|x| &x.id == id)
                    .unwrap_or(usize::MAX - 1)
            }
        };
        let target = rank(part);
        self.modules
            .iter()
            .position(|m| rank(m.part.as_ref()) > target)
            .unwrap_or(self.modules.len())
    }

    /// True when the module, its part or the param itself is locked.
    pub fn is_locked(&self, module: &Module, param: &ParamValue) -> bool {
        module.locked
            || param.locked
            || module
                .part
                .as_ref()
                .and_then(|p| self.part(p))
                .is_some_and(|p| p.locked)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleRef {
    pub name: String,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub id: PartId,
    pub name: String,
    pub colour: Colour,
    pub locked: bool,
    pub muted: bool,
    /// Gain on audio leaving the part, `0.0..=1.0`; 1.0 is unity.
    #[serde(default = "Part::unity")]
    pub level: f32,
}

impl Part {
    pub const fn unity() -> f32 {
        1.0
    }

    /// Display names are printed in double quotes, one line each.
    pub fn valid_name(name: &str) -> bool {
        !name.is_empty() && !name.contains(['"', '\n', '\r'])
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub id: ModuleId,
    pub kind: String,
    pub part: Option<PartId>,
    pub params: Vec<ParamValue>,
    pub locked: bool,
    /// Audio file for sample modules (Veneer, Splinter), as given: absolute, or
    /// relative to the `.kerf` file. Printed as `sample="path"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<String>,
}

impl Module {
    /// Sample paths are printed in double quotes, one line each.
    pub fn valid_sample_path(path: &str) -> bool {
        !path.trim().is_empty() && !path.contains(['"', '\n', '\r'])
    }

    pub fn param(&self, key: &Key) -> Option<&ParamValue> {
        self.params.iter().find(|p| &p.key == key)
    }

    pub fn param_mut(&mut self, key: &Key) -> Option<&mut ParamValue> {
        self.params.iter_mut().find(|p| &p.key == key)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamValue {
    pub key: Key,
    pub value: f32,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cable {
    pub from: JackRef,
    pub to: JackRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneValue {
    pub module: ModuleId,
    pub param: Key,
    pub value: f32,
}
