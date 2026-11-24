//! What an editor needs to help with `.kerf` text: completion data and the
//! language docs. Module facts come from the descriptors and style facts from
//! the rulebooks, so they cannot drift from what the engine accepts; only the
//! grammar keywords are written here.

use std::collections::BTreeSet;
use std::fmt::Write;

use kerf_core::{Band, Catalogue, ModuleDescriptor};
use serde::Serialize;

/// The hand-written guide; its `## ` headings become doc sections.
const FORMAT_GUIDE: &str = include_str!("../../../docs/KERF-FORMAT.md");
const WRITING_GUIDE: &str = include_str!("../../../docs/WRITING-PATCHES.md");

#[derive(Debug, Serialize)]
pub struct Keyword {
    pub word: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<&'static str>,
    pub doc: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ParamInfo {
    pub key: &'static str,
    pub label: &'static str,
    pub default: f32,
    pub help: &'static str,
}

#[derive(Debug, Serialize)]
pub struct JackInfo {
    pub key: &'static str,
    pub kind: &'static str,
    pub help: &'static str,
}

#[derive(Debug, Serialize)]
pub struct KindInfo {
    pub kind: &'static str,
    pub band: &'static str,
    pub summary: &'static str,
    pub params: Vec<ParamInfo>,
    pub inputs: Vec<JackInfo>,
    pub outputs: Vec<JackInfo>,
}

#[derive(Debug, Serialize)]
pub struct StyleName {
    pub name: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Serialize)]
pub struct Completions {
    pub keywords: Vec<Keyword>,
    pub kinds: Vec<KindInfo>,
    pub styles: Vec<StyleName>,
    pub keys: Vec<String>,
    pub scales: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub markdown: String,
}

#[derive(Debug, Serialize)]
pub struct LanguageDocs {
    pub sections: Vec<Section>,
}

const fn kw(word: &'static str, snippet: Option<&'static str>, doc: &'static str) -> Keyword {
    Keyword { word, snippet, doc }
}

/// The grammar of the text format, one line each.
fn keywords() -> Vec<Keyword> {
    vec![
        kw("kerf", Some("kerf 1"), "The first line: the format version."),
        kw("tempo", Some("tempo ${1:120}"), "Beats per minute, 20 to 300."),
        kw("key", Some("key ${1:A minor}"), "The musical key, as a label for people and generators."),
        kw("style", Some("style ${1:dub-chamber} seed ${2:1}"), "The style this patch grew from, and its seed."),
        kw("tags", Some("tags ${1:four-four}"), "Category words for the starting-point browser, such as `four-four`, `abstract` or `crazy`."),
        kw(
            "part",
            Some("part ${1:bass} \"${2:Bass}\" colour #${3:3d63c9}"),
            "A named section of modules with a colour; add `level=0.8`, `lock` or `mute`.",
        ),
        kw(
            "module",
            Some("module ${1:id} ${2:Kind}"),
            "A module: `module <id> <Kind> key=value ...`; params run 0 to 1.",
        ),
        kw(
            "cable",
            Some("cable ${1:from}.${2:out} -> ${3:to}.${4:in}"),
            "A cable from an output jack to an input jack.",
        ),
        kw(
            "scene",
            Some("scene ${1:A} ${2:module}.${3:param}=${4:0.5}"),
            "A stored set of param values, A to D, to morph between.",
        ),
        kw("lock", None, "On a module line, `lock` freezes the module; `lock=a,b` freezes those params. On a part line it freezes the part."),
        kw("mute", None, "On a part line: mutes the part."),
        kw("colour", None, "On a part line: `colour #rrggbb`."),
        kw("seed", None, "In a `style` line: the whole number the generator started from."),
        kw("level=", Some("level=${1:0.8}"), "On a part line: its level, 0 to 1 (1 is unity, 0.5 is -6 dB); scales audio leaving the part."),
        kw("sample=", Some("sample=\"${1:samples/loop.wav}\""), "On a Veneer or Splinter line: the audio file it plays."),
    ]
}

fn jack_info(j: &'static kerf_core::JackDescriptor) -> JackInfo {
    JackInfo {
        key: j.key,
        kind: j.kind.name(),
        help: j.tooltip,
    }
}

fn kind_info(d: &'static ModuleDescriptor) -> KindInfo {
    KindInfo {
        kind: d.kind,
        band: d.band.name(),
        summary: d.summary,
        params: d
            .params
            .iter()
            .map(|p| ParamInfo {
                key: p.key,
                label: p.label,
                default: p.default,
                help: p.tooltip,
            })
            .collect(),
        inputs: d.inputs.iter().map(jack_info).collect(),
        outputs: d.outputs.iter().map(jack_info).collect(),
    }
}

/// Keywords, module kinds with their params and jacks, styles, and the keys
/// and scales the styles use.
pub fn completions(cat: &impl Catalogue) -> Completions {
    let styles = kerf_gen::styles();
    let mut keys = BTreeSet::new();
    let mut scales = BTreeSet::new();
    for key in styles.iter().flat_map(|s| &s.keys) {
        keys.insert(key.clone());
        if let Some(scale) = key.split_whitespace().nth(1) {
            scales.insert(scale.to_owned());
        }
    }
    Completions {
        keywords: keywords(),
        kinds: cat.all().iter().map(|d| kind_info(d)).collect(),
        styles: styles
            .iter()
            .map(|s| StyleName {
                name: s.name.clone(),
                title: s.title.clone(),
                summary: s.summary.clone(),
            })
            .collect(),
        keys: keys.into_iter().collect(),
        scales: scales.into_iter().collect(),
    }
}

fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// Splits the guide on `## ` headings (never inside a fenced block). Text
/// before the first heading becomes the first section, titled by its `# `
/// line.
fn guide_sections(guide: &str) -> Vec<Section> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut fenced = false;
    for line in guide.lines() {
        if line.starts_with("```") {
            fenced = !fenced;
        }
        match line.strip_prefix("## ").filter(|_| !fenced) {
            Some(title) => sections.push((title.trim().to_owned(), String::new())),
            None => {
                if sections.is_empty() {
                    sections.push((String::new(), String::new()));
                }
                let body = &mut sections.last_mut().expect("just pushed").1;
                body.push_str(line);
                body.push('\n');
            }
        }
    }
    sections
        .into_iter()
        .map(|(title, body)| {
            let title = if title.is_empty() {
                // The intro: title it by the document's `# ` line.
                body.lines()
                    .find_map(|l| l.strip_prefix("# "))
                    .unwrap_or("The .kerf format")
                    .to_owned()
            } else {
                title
            };
            Section {
                id: slug(&title),
                title,
                markdown: body.trim().to_owned(),
            }
        })
        .collect()
}

fn band_section(cat: &impl Catalogue, band: Band) -> Section {
    let mut md = format!("{}.\n", band.summary());
    for d in cat.all().iter().filter(|d| d.band == band) {
        let _ = write!(md, "\n### {}\n\n{}.\n", d.kind, d.summary);
        if !d.params.is_empty() {
            md.push_str("\nParams (0 to 1):\n\n");
            for p in d.params {
                let _ = writeln!(md, "- `{}` (default {}): {}", p.key, p.default, p.tooltip);
            }
        }
        for (title, jacks) in [("Inputs", d.inputs), ("Outputs", d.outputs)] {
            if jacks.is_empty() {
                continue;
            }
            let _ = write!(md, "\n{title}:\n\n");
            for j in jacks {
                let _ = writeln!(md, "- `{}` ({}): {}", j.key, j.kind.name(), j.tooltip);
            }
        }
    }
    Section {
        id: format!("modules-{}", slug(band.name())),
        title: format!("Modules: {}", band.name()),
        markdown: md.trim().to_owned(),
    }
}

/// The tutorial, the format reference, the snippet library, then one
/// generated section per band.
pub fn language_docs(cat: &impl Catalogue) -> LanguageDocs {
    let mut sections = guide_sections(WRITING_GUIDE);
    sections.extend(guide_sections(FORMAT_GUIDE));
    sections.push(Section {
        id: "snippets".to_owned(),
        title: "Snippets".to_owned(),
        markdown: crate::snippets::INTRO.to_owned(),
    });
    sections.extend(crate::snippets::CATEGORIES.iter().map(|c| Section {
        id: format!("snippets-{}", c.dir),
        title: format!("Snippets: {}", c.title),
        markdown: crate::snippets::category_markdown(c).trim().to_owned(),
    }));
    sections.extend(Band::ALL.into_iter().map(|b| band_section(cat, b)));
    LanguageDocs { sections }
}

/// The hand-written guide as one text, for tests that check its examples.
pub fn format_guide() -> &'static str {
    FORMAT_GUIDE
}

/// The patch-writing tutorial as one text, for tests that check its examples.
pub fn writing_guide() -> &'static str {
    WRITING_GUIDE
}

/// The bodies of the fenced blocks in `markdown` whose opening line is exactly
/// "```" followed by `info` (for example `kerf complete`). For tests that
/// check the examples in the guides.
pub fn fenced_blocks(markdown: &str, info: &str) -> Vec<String> {
    let opener = format!("```{info}");
    let mut blocks = Vec::new();
    let mut open: Option<String> = None;
    for line in markdown.lines() {
        match (&mut open, line) {
            (None, l) if l == opener => open = Some(String::new()),
            (Some(_), "```") => blocks.extend(open.take()),
            (Some(b), l) => {
                b.push_str(l);
                b.push('\n');
            }
            _ => {}
        }
    }
    blocks
}
