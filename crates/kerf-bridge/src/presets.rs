//! The factory presets, embedded at build time.

use kerf_core::text::parse;

use crate::{Bridge, BridgeError, PresetInfo, State};

include!(concat!(env!("OUT_DIR"), "/presets.rs"));

/// The path prefix of a factory preset: `preset:<name>`.
pub const PREFIX: &str = "preset:";

/// The text of the factory preset `name`.
pub fn text(name: &str) -> Option<&'static str> {
    PRESETS.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

impl Bridge {
    pub fn list_presets(&self) -> Vec<PresetInfo> {
        PRESETS
            .iter()
            .filter_map(|(name, text)| {
                let patch = parse(text).ok()?;
                Some(PresetInfo {
                    name: (*name).to_owned(),
                    path: format!("{PREFIX}{name}"),
                    tempo: patch.tempo,
                    key: patch.key,
                    style: patch.style.map(|s| s.name),
                    tags: patch.tags,
                    modules: patch.modules.len(),
                })
            })
            .collect()
    }

    /// Opens a factory preset as an unsaved session, so Save asks for a new file instead of
    /// overwriting anything.
    pub fn open_preset(&self, name: &str) -> Result<State, BridgeError> {
        let text = text(name)
            .ok_or_else(|| BridgeError::Invalid(format!("no factory preset `{name}`")))?;
        self.open_text(&format!("{PREFIX}{name}"), text, None)
    }
}
