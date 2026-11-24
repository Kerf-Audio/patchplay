//! Diagnostics for editors: every problem in a `.kerf` text with its line and
//! column, without applying anything.
//!
//! Syntax errors come from the parser (one per bad line). On top of that this
//! module checks what the parser cannot know: module kinds, params and jacks
//! against the catalogue, cables and scenes against the modules written in the
//! text. Lines the parser rejected are not checked again, and modules on them
//! are not reported as missing when a cable names them.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::parse::{parse_all, strip_bom};
use super::token::{Token, tokenize};
use crate::{Catalogue, JackKind, ModuleDescriptor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The text cannot be applied.
    Error,
    /// The text applies, but is probably not what was meant.
    Warning,
}

/// One problem. `line` and `col` are 1-based; the span is the characters
/// `col..end_col` (`end_col` is one past the last).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub line: usize,
    pub col: usize,
    pub end_col: usize,
    pub severity: Severity,
    pub message: String,
}

struct Line<'a> {
    no: usize,
    raw: &'a str,
    indent: usize,
    text: &'a str,
    tokens: Vec<Token<'a>>,
}

impl Line<'_> {
    /// 1-based character column of a byte offset in the trimmed text.
    fn col(&self, byte: usize) -> usize {
        self.raw[..self.indent].chars().count() + self.text[..byte].chars().count() + 1
    }

    fn diag(&self, start: usize, end: usize, severity: Severity, message: String) -> Diagnostic {
        let col = self.col(start);
        Diagnostic {
            line: self.no,
            col,
            end_col: self.col(end).max(col + 1),
            severity,
            message,
        }
    }

    fn at(&self, t: &Token, severity: Severity, message: String) -> Diagnostic {
        self.diag(t.start, t.end, severity, message)
    }
}

/// Checks `src` and lists every problem, in line order.
pub fn diagnose(cat: &impl Catalogue, src: &str) -> Vec<Diagnostic> {
    let (_, errors) = parse_all(src);
    let bad_lines: HashSet<usize> = errors.iter().map(|e| e.line).collect();
    let mut out: Vec<Diagnostic> = errors
        .into_iter()
        .map(|e| Diagnostic {
            line: e.line,
            col: e.col,
            end_col: e.end_col,
            severity: Severity::Error,
            message: e.message,
        })
        .collect();

    let lines = lines(src);
    let mut kinds: HashMap<&str, Option<&'static ModuleDescriptor>> = HashMap::new();
    for l in lines.iter().skip(1) {
        if l.tokens[0].text == "module" && l.tokens.len() >= 3 {
            // A bad line still counts as a module, so cables to it stay quiet.
            let known = (!bad_lines.contains(&l.no))
                .then(|| cat.get(l.tokens[2].text))
                .flatten();
            kinds.entry(l.tokens[1].text).or_insert(known);
        }
    }

    let mut feeds: HashMap<String, usize> = HashMap::new();
    for l in lines.iter().skip(1).filter(|l| !bad_lines.contains(&l.no)) {
        match l.tokens[0].text {
            "module" => check_module(cat, l, &mut out),
            "cable" => check_cable(l, &kinds, &mut feeds, &mut out),
            "scene" => check_scene(l, &kinds, &mut out),
            _ => {}
        }
    }
    out.sort_by_key(|d| (d.line, d.col));
    out
}

/// The non-blank, non-comment lines from the `kerf` header on, tokenised.
/// Lines that do not tokenise are left to the parser's error.
fn lines(src: &str) -> Vec<Line<'_>> {
    let mut out: Vec<Line> = Vec::new();
    for (i, raw) in strip_bom(src).lines().enumerate() {
        let text = raw.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        let Ok(tokens) = tokenize(text) else {
            continue;
        };
        if out.is_empty() && tokens[0].text != "kerf" {
            return Vec::new();
        }
        out.push(Line {
            no: i + 1,
            raw,
            indent: raw.len() - raw.trim_start().len(),
            text,
            tokens,
        });
    }
    out
}

fn check_module(cat: &impl Catalogue, l: &Line, out: &mut Vec<Diagnostic>) {
    let [_, _, kind, params @ ..] = l.tokens.as_slice() else {
        return;
    };
    let Some(d) = cat.get(kind.text) else {
        let known: Vec<_> = cat.all().iter().map(|d| d.kind).collect();
        out.push(l.at(
            kind,
            Severity::Error,
            format!(
                "no module kind `{}`; kinds: {}",
                kind.text,
                known.join(", ")
            ),
        ));
        return;
    };
    for t in params {
        let Some((key, _)) = t.text.split_once('=') else {
            continue;
        };
        if key == "lock" {
            continue;
        }
        if key == "sample" {
            if !d.takes_sample() {
                out.push(l.at(
                    t,
                    Severity::Error,
                    format!(
                        "{} does not play a sample; only a Veneer or Splinter does",
                        d.kind
                    ),
                ));
            }
        } else if d.param(key).is_none() {
            let keys: Vec<_> = d.params.iter().map(|p| p.key).collect();
            out.push(l.diag(
                t.body(),
                t.body() + key.len(),
                Severity::Error,
                format!(
                    "{} has no param `{key}`; params: {}",
                    d.kind,
                    keys.join(", ")
                ),
            ));
        }
    }
}

/// Which side of a cable a jack is on.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    Out,
    In,
}

/// Checks one `module.jack` token, returning the jack's kind when it exists.
fn check_jack(
    l: &Line,
    t: &Token,
    side: Side,
    kinds: &HashMap<&str, Option<&'static ModuleDescriptor>>,
    out: &mut Vec<Diagnostic>,
) -> Option<JackKind> {
    let (module, jack) = t.text.split_once('.')?;
    let Some(known) = kinds.get(module) else {
        out.push(l.diag(
            t.body(),
            t.body() + module.len(),
            Severity::Error,
            format!("no module `{module}` in this patch"),
        ));
        return None;
    };
    let d = (*known)?;
    let (mine, other, word, other_word) = match side {
        Side::Out => (d.outputs, d.inputs, "output", "input"),
        Side::In => (d.inputs, d.outputs, "input", "output"),
    };
    if let Some(j) = mine.iter().find(|j| j.key == jack) {
        return Some(j.kind);
    }
    let start = t.body() + module.len() + 1;
    let keys: Vec<_> = mine.iter().map(|j| j.key).collect();
    let message = if other.iter().any(|j| j.key == jack) {
        format!(
            "`{jack}` is an {other_word} on {}, not an {word}; {word}s: {}",
            d.kind,
            keys.join(", ")
        )
    } else if keys.is_empty() {
        format!("{} has no {word}s", d.kind)
    } else {
        format!(
            "{} has no {word} `{jack}`; {word}s: {}",
            d.kind,
            keys.join(", ")
        )
    };
    out.push(l.diag(start, t.end, Severity::Error, message));
    None
}

fn check_cable(
    l: &Line,
    kinds: &HashMap<&str, Option<&'static ModuleDescriptor>>,
    feeds: &mut HashMap<String, usize>,
    out: &mut Vec<Diagnostic>,
) {
    let [_, from, _, to] = l.tokens.as_slice() else {
        return;
    };
    let from_ok = check_jack(l, from, Side::Out, kinds, out).is_some();
    let to_kind = check_jack(l, to, Side::In, kinds, out);
    if let (true, Some(kind)) = (from_ok, to_kind) {
        let n = feeds.entry(to.text.to_owned()).or_insert(0);
        *n += 1;
        // Audio and CV inputs add their cables on purpose (a mixer, two LFOs);
        // adding two clocks, pitches or gates is rarely meant.
        if *n == 2 && !matches!(kind, JackKind::Audio | JackKind::Cv) {
            out.push(l.at(
                to,
                Severity::Warning,
                format!(
                    "more than one cable feeds `{}`; their voltages are added together",
                    to.text
                ),
            ));
        }
    }
}

fn check_scene(
    l: &Line,
    kinds: &HashMap<&str, Option<&'static ModuleDescriptor>>,
    out: &mut Vec<Diagnostic>,
) {
    for t in l.tokens.iter().skip(2) {
        let Some((target, _)) = t.text.split_once('=') else {
            continue;
        };
        let Some((module, param)) = target.split_once('.') else {
            continue;
        };
        match kinds.get(module) {
            None => out.push(l.diag(
                t.body(),
                t.body() + module.len(),
                Severity::Error,
                format!("no module `{module}` in this patch"),
            )),
            Some(Some(d)) if d.param(param).is_none() => out.push(l.diag(
                t.body() + module.len() + 1,
                t.body() + target.len(),
                Severity::Error,
                format!("{} has no param `{param}`", d.kind),
            )),
            _ => {}
        }
    }
}
