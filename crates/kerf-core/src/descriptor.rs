//! Static module descriptions. Every param and jack carries its tooltip, so
//! GUI tooltips, `kerf describe` and MCP tool docs share one source of help.
//!
//! Params are normalised to `0.0..=1.0`; each module maps them to its own
//! units when it processes audio.

use serde::Serialize;

use crate::id::Key;
use crate::patch::ParamValue;

/// The six bands of the module catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    Time,
    Motion,
    Voices,
    Shape,
    Space,
    Out,
}

impl Band {
    pub const ALL: [Band; 6] = [
        Band::Time,
        Band::Motion,
        Band::Voices,
        Band::Shape,
        Band::Space,
        Band::Out,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Band::Time => "Time",
            Band::Motion => "Motion",
            Band::Voices => "Voices",
            Band::Shape => "Shape",
            Band::Space => "Space",
            Band::Out => "Out",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Band::Time => "Clocks, sequencers and quantisers: when notes happen and which notes",
            Band::Motion => "LFOs, random voltages, envelopes and macros: what moves",
            Band::Voices => "Oscillators, drums and samplers: what makes sound",
            Band::Shape => "Filters, VCAs, waveshapers and mixers: how sound is shaped",
            Band::Space => "Delay, granular and reverb: where sound sits",
            Band::Out => "Part mixing and the master bus",
        }
    }
}

/// What a jack carries. Cables are coloured by kind; any output may feed any
/// input, the kind only guides the generator and the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JackKind {
    Audio,
    Cv,
    Pitch,
    Gate,
    Clock,
}

impl JackKind {
    pub fn name(self) -> &'static str {
        match self {
            JackKind::Audio => "audio",
            JackKind::Cv => "cv",
            JackKind::Pitch => "pitch",
            JackKind::Gate => "gate",
            JackKind::Clock => "clock",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ParamDescriptor {
    pub key: &'static str,
    pub label: &'static str,
    pub default: f32,
    pub tooltip: &'static str,
}

#[derive(Debug, Serialize)]
pub struct JackDescriptor {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: JackKind,
    pub tooltip: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ModuleDescriptor {
    pub kind: &'static str,
    pub band: Band,
    /// Panel width in horizontal units.
    pub hp: u8,
    pub summary: &'static str,
    pub params: &'static [ParamDescriptor],
    pub inputs: &'static [JackDescriptor],
    pub outputs: &'static [JackDescriptor],
}

impl ModuleDescriptor {
    pub fn param(&self, key: &str) -> Option<&ParamDescriptor> {
        self.params.iter().find(|p| p.key == key)
    }

    pub fn input(&self, key: &str) -> Option<&JackDescriptor> {
        self.inputs.iter().find(|j| j.key == key)
    }

    pub fn output(&self, key: &str) -> Option<&JackDescriptor> {
        self.outputs.iter().find(|j| j.key == key)
    }

    /// Whether the module plays an audio file (`SetSample`, `sample="…"`).
    pub fn takes_sample(&self) -> bool {
        matches!(self.kind, "Veneer" | "Splinter")
    }

    /// Every param at its default, unlocked, in descriptor order.
    pub fn default_params(&self) -> Vec<ParamValue> {
        self.params
            .iter()
            .map(|p| ParamValue {
                key: Key::new(p.key).expect("catalogue keys are valid"),
                value: p.default,
                locked: false,
            })
            .collect()
    }
}

pub const fn param(
    key: &'static str,
    label: &'static str,
    default: f32,
    tooltip: &'static str,
) -> ParamDescriptor {
    ParamDescriptor {
        key,
        label,
        default,
        tooltip,
    }
}

pub const fn jack(
    key: &'static str,
    label: &'static str,
    kind: JackKind,
    tooltip: &'static str,
) -> JackDescriptor {
    JackDescriptor {
        key,
        label,
        kind,
        tooltip,
    }
}

/// Looks up module kinds. Implemented by `kerf-modules`; core only needs the
/// descriptors to validate commands.
pub trait Catalogue {
    fn get(&self, kind: &str) -> Option<&'static ModuleDescriptor>;
    fn all(&self) -> &'static [&'static ModuleDescriptor];
}
