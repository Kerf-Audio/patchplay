use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kerf_core::text::{Severity, diagnose, parse};
use kerf_modules::Builtin;

use crate::snippets::{CATEGORIES, SOURCES, doc_files, snippets};
use crate::{Session, fenced_blocks, language_docs, writing_guide};

const HEADER: &str = "kerf 1\ntempo 120\nkey A minor\n\n";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn start() -> &'static str {
    &snippets().iter().find(|s| s.id == "start/clock-out").unwrap().code
}

fn assert_clean(text: &str, what: &str) {
    let patch = parse(text).unwrap_or_else(|e| panic!("{what}: {e}\n{text}"));
    Session::open(Builtin, patch).unwrap_or_else(|e| panic!("{what}: {e}\n{text}"));
    let found = diagnose(&Builtin, text);
    assert!(found.is_empty(), "{what}: {found:?}\n{text}");
}

#[test]
fn every_file_in_the_folder_is_listed_and_back() {
    let mut on_disk = BTreeSet::new();
    for dir in std::fs::read_dir(repo().join("snippets")).unwrap() {
        let dir = dir.unwrap().path();
        for file in std::fs::read_dir(&dir).unwrap() {
            let file = file.unwrap().path();
            assert_eq!(file.extension().and_then(|e| e.to_str()), Some("kerf"), "{file:?}");
            on_disk.insert(format!(
                "{}/{}",
                dir.file_name().unwrap().to_string_lossy(),
                file.file_stem().unwrap().to_string_lossy()
            ));
        }
    }
    let listed: BTreeSet<String> = SOURCES.iter().map(|(id, _)| (*id).to_owned()).collect();
    assert_eq!(on_disk, listed);
    let dirs: BTreeSet<&str> = CATEGORIES.iter().map(|c| c.dir).collect();
    assert!(snippets().iter().all(|s| dirs.contains(s.category)));
    assert!(CATEGORIES.iter().all(|c| snippets().iter().any(|s| s.category == c.dir)));
}

#[test]
fn every_snippet_opens_with_the_start_snippet_and_has_no_diagnostics() {
    for s in snippets() {
        assert!(!s.title.is_empty() && !s.about.is_empty(), "{}", s.id);
        assert!(s.code.lines().all(|l| l.len() <= 90 || !l.starts_with('#')), "{}: long comment", s.id);
        let text = if s.category == "start" {
            format!("{HEADER}{}", s.code)
        } else {
            assert!(!s.needs.is_empty() && s.needs.iter().all(|n| n == "clock" || n == "out"), "{}", s.id);
            format!("{HEADER}{}\n{}", start(), s.code)
        };
        assert_clean(&text, s.id);
    }
}

#[test]
fn effects_name_inputs_and_sounds_do_not() {
    for s in snippets() {
        assert_eq!(s.feed.is_empty(), s.category != "effects", "{}", s.id);
        for jack in &s.feed {
            assert!(s.body.contains(jack.split('.').next().unwrap()), "{}: {jack}", s.id);
        }
    }
}

#[test]
fn all_snippets_paste_together() {
    let mut text = HEADER.to_owned();
    text.push_str(start());
    for s in snippets().iter().filter(|s| s.category != "start") {
        text.push('\n');
        text.push_str(&s.code);
    }
    assert_clean(&text, "everything pasted together");
}

#[test]
fn generated_docs_match_the_files() {
    let bless = std::env::var_os("KERF_BLESS_DOCS").is_some();
    for (path, content) in doc_files() {
        let file = repo().join("docs").join(&path);
        if bless {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, &content).unwrap();
        }
        let on_disk = std::fs::read_to_string(&file).unwrap_or_default();
        assert!(
            on_disk == content,
            "docs/{path} is out of date; run `KERF_BLESS_DOCS=1 cargo test -p kerf-session generated_docs`"
        );
        assert!(content.lines().count() < 500, "docs/{path} is too long");
    }
}

#[test]
fn doc_sections_have_unique_ids() {
    let docs = language_docs(&Builtin);
    let mut seen = BTreeSet::new();
    for s in &docs.sections {
        assert!(seen.insert(s.id.clone()), "duplicate section id {}", s.id);
    }
    assert!(seen.contains("snippets") && seen.contains("snippets-effects") && seen.contains("step-1-a-clock-and-a-kick"));
}

#[test]
fn the_tutorial_builds_a_patch() {
    let guide = writing_guide();
    let complete = fenced_blocks(guide, "kerf complete");
    assert_eq!(complete.len(), 1);
    assert_clean(&complete[0], "tutorial step 1");
    let steps = fenced_blocks(guide, "kerf step");
    assert_eq!(steps.len(), 3);
    let mut text = complete[0].clone();
    for step in &steps {
        text.push('\n');
        text.push_str(step);
        assert_clean(&text, "tutorial steps");
    }
    let _ = Severity::Error;
}
