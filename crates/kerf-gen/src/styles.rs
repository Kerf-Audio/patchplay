//! The registry of embedded style rulebooks.
//!
//! A new style is one TOML file in `styles/` plus one line in `SOURCES`; the
//! `every_style_file_is_registered` test fails if the line is forgotten.

use std::sync::LazyLock;

use kerf_core::PartId;
use serde::Serialize;

use crate::rulebook::Rulebook;

const SOURCES: [&str; 6] = [
    include_str!("../styles/acid-line.toml"),
    include_str!("../styles/foundry.toml"),
    include_str!("../styles/dub-chamber.toml"),
    include_str!("../styles/slow-grain.toml"),
    include_str!("../styles/broken-joinery.toml"),
    include_str!("../styles/tape-keys.toml"),
];

static BOOKS: LazyLock<Vec<Rulebook>> = LazyLock::new(|| {
    SOURCES
        .iter()
        .map(|src| Rulebook::parse(src).expect("embedded rulebooks are valid"))
        .collect()
});

pub(crate) fn rulebooks() -> &'static [Rulebook] {
    &BOOKS
}

pub(crate) fn rulebook(name: &str) -> Option<&'static Rulebook> {
    rulebooks().iter().find(|r| r.name == name)
}

/// What a UI or CLI shows about a style.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StyleInfo {
    pub name: String,
    pub title: String,
    pub summary: String,
    pub tempo: [f32; 2],
    pub keys: Vec<String>,
    pub parts: Vec<PartInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartInfo {
    pub id: PartId,
    pub name: String,
    /// True when a generated patch may leave this part out.
    pub optional: bool,
}

/// Every style, in registry order.
pub fn styles() -> Vec<StyleInfo> {
    rulebooks()
        .iter()
        .map(|r| StyleInfo {
            name: r.name.clone(),
            title: r.title.clone(),
            summary: r.summary.clone(),
            tempo: r.tempo,
            keys: r.keys.clone(),
            parts: r
                .parts
                .iter()
                .map(|p| PartInfo {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    optional: p.chance < 1.0,
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_file_is_registered() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/styles");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                e.unwrap()
                    .path()
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        files.sort();
        let mut names: Vec<_> = rulebooks().iter().map(|r| r.name.clone()).collect();
        names.sort();
        assert_eq!(files, names, "file names must match rulebook names");
    }

    #[test]
    fn style_info_lists_parts_and_tempo() {
        let all = styles();
        let foundry = all.iter().find(|s| s.name == "foundry").unwrap();
        assert!(foundry.tempo[0] < foundry.tempo[1]);
        assert!(foundry.parts.iter().any(|p| p.optional));
        assert!(foundry.parts.iter().any(|p| p.id.as_str() == "drums"));
    }
}
