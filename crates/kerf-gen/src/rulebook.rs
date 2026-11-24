//! Style rulebooks: TOML data that says how to build a patch in one style.
//!
//! ```toml
//! name = "acid-line"                   # the id used by `random` and `kerf new --style`
//! title = "Acid Line"
//! summary = "Drums and a squelchy bass."
//! tempo = [126, 136]                   # bpm range
//! keys = ["C minor", "A minor"]        # one is chosen; root note first
//!
//! [[modules]]                          # shared modules, outside any part
//! id = "tally1"
//! kind = "Tally"                       # any kind in the catalogue
//! [modules.params]                     # unlisted params keep their defaults
//! swing = [0.0, 0.25]                  # see `Spec` for the six value forms
//!
//! [[cables]]                           # cables between shared modules
//! from = "mix.mix_l"                   # a jack, or a list: one is chosen
//! to = "out.l"
//! chance = 0.5                         # optional, default 1
//!
//! [[parts]]                            # a row on the workbench
//! id = "bass"
//! name = "Bass"
//! colour = "#3d63c9"
//! chance = 1                           # chance the part appears, default 1
//! weight = 1                           # weight of this layout among the alts
//! [[parts.modules]]                    # the part's modules and cables
//! id = "seq1"
//! kind = "Dovetail"
//! [parts.modules.params]
//! steps_a = { steps = [8, 16] }
//! [[parts.cables]]
//! from = "tally1.clock"
//! to = "seq1.clock"
//!
//! [[parts.alt]]                        # optional: other whole layouts for the
//! weight = 1                           # part; same fields as the part's own
//! [[parts.alt.modules]]                # `modules` and `cables`
//! ```
//!
//! Rhythm rule: lane lengths and chance are ordinary params, so a part's
//! ranges for `steps_*` and `chance` are its rhythm rules. An anchor (a
//! four-on-the-floor kick on Ruler lane A) pins `steps_a` to 1 (16 steps) and
//! needs no chance, since lane A always plays; the other lanes get short
//! lengths and a chance below 1.
//!
//! Key: a melody keeps to the patch key by going through a Fret whose `root`
//! is `{ root = true }` and whose `scale` picks a mode that sits inside the
//! style's keys (minor pentatonic or natural minor for `minor` keys).
//!
//! Effect sends are parts too: a part holding Bandsaw or Kiln with a cable
//! from `mix.send`, and a cable back into a free mixer channel.

use std::collections::BTreeMap;

use kerf_core::{Colour, JackRef, Key, ModuleId, PartId, Patch};
use kerf_dsp::Rng;
use serde::Deserialize;

use crate::spec::Spec;
use crate::util::weighted;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct RulebookError(String);

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rulebook {
    pub name: String,
    pub title: String,
    pub summary: String,
    pub tempo: [f32; 2],
    pub keys: Vec<String>,
    #[serde(default)]
    pub modules: Vec<ModuleSlot>,
    #[serde(default)]
    pub cables: Vec<CableRule>,
    pub parts: Vec<PartRule>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleSlot {
    pub id: ModuleId,
    pub kind: String,
    #[serde(default)]
    pub params: BTreeMap<Key, Spec>,
}

/// One end or both ends may be a list; one option is chosen per patch.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
enum Jacks {
    One(JackRef),
    Many(Vec<JackRef>),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CableRule {
    from: Jacks,
    to: Jacks,
    #[serde(default = "one")]
    pub chance: f32,
}

impl CableRule {
    pub fn from(&self) -> &[JackRef] {
        match &self.from {
            Jacks::One(j) => std::slice::from_ref(j),
            Jacks::Many(js) => js,
        }
    }

    pub fn to(&self) -> &[JackRef] {
        match &self.to {
            Jacks::One(j) => std::slice::from_ref(j),
            Jacks::Many(js) => js,
        }
    }

    /// True when the rule has options to choose between or may be absent.
    pub fn is_choice(&self) -> bool {
        self.from().len() > 1 || self.to().len() > 1 || self.chance < 1.0
    }

    pub fn matches(&self, from: &JackRef, to: &JackRef) -> bool {
        self.from().contains(from) && self.to().contains(to)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartRule {
    pub id: PartId,
    pub name: String,
    pub colour: Colour,
    #[serde(default = "one")]
    pub chance: f32,
    #[serde(default = "one")]
    weight: f32,
    #[serde(default)]
    modules: Vec<ModuleSlot>,
    #[serde(default)]
    cables: Vec<CableRule>,
    #[serde(default)]
    alt: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    #[serde(default = "one")]
    weight: f32,
    #[serde(default)]
    modules: Vec<ModuleSlot>,
    #[serde(default)]
    cables: Vec<CableRule>,
}

/// One whole layout of a part: its modules and the cables wiring them.
#[derive(Debug, Clone, Copy)]
pub struct Layout<'a> {
    pub weight: f32,
    pub modules: &'a [ModuleSlot],
    pub cables: &'a [CableRule],
}

impl PartRule {
    /// The part's own layout is number 0; `alt` layouts follow.
    pub fn layout_count(&self) -> usize {
        1 + self.alt.len()
    }

    pub fn layout(&self, i: usize) -> Layout<'_> {
        match i.checked_sub(1) {
            None => Layout {
                weight: self.weight,
                modules: &self.modules,
                cables: &self.cables,
            },
            Some(a) => Layout {
                weight: self.alt[a].weight,
                modules: &self.alt[a].modules,
                cables: &self.alt[a].cables,
            },
        }
    }

    /// A layout chosen by weight, other than `exclude`.
    pub(crate) fn pick_layout(&self, rng: &mut Rng, exclude: Option<usize>) -> usize {
        let weights: Vec<f32> = (0..self.layout_count())
            .map(|i| self.layout(i).weight)
            .collect();
        weighted(rng, &weights, exclude)
    }

    /// The layout whose modules (id and kind) are exactly those the patch
    /// holds for this part, if any.
    pub(crate) fn current_layout(&self, patch: &Patch) -> Option<usize> {
        let held: Vec<_> = patch
            .modules
            .iter()
            .filter(|m| m.part.as_ref() == Some(&self.id))
            .collect();
        (0..self.layout_count()).find(|&i| {
            let slots = self.layout(i).modules;
            slots.len() == held.len()
                && slots.iter().all(|s| {
                    held.iter()
                        .any(|m| m.id == s.id && m.kind.eq_ignore_ascii_case(&s.kind))
                })
        })
    }
}

impl Rulebook {
    /// Parses and checks a rulebook against the module catalogue.
    pub fn parse(src: &str) -> Result<Self, RulebookError> {
        let rb: Rulebook = toml::from_str(src).map_err(|e| RulebookError(e.to_string()))?;
        crate::validate::check(&rb)
            .map_err(|e| RulebookError(format!("rulebook `{}`: {e}", rb.name)))?;
        Ok(rb)
    }

    /// The slot that makes module `id` of `kind`, in any layout.
    pub fn slot(&self, id: &ModuleId, kind: &str) -> Option<&ModuleSlot> {
        self.modules
            .iter()
            .chain(
                self.parts.iter().flat_map(|p| {
                    (0..p.layout_count()).flat_map(move |i| p.layout(i).modules.iter())
                }),
            )
            .find(|s| &s.id == id && s.kind.eq_ignore_ascii_case(kind))
    }

    pub fn part(&self, id: &PartId) -> Option<&PartRule> {
        self.parts.iter().find(|p| &p.id == id)
    }
}
