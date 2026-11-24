use kerf_core::text::{Severity, diagnose, parse};
use kerf_core::{Command, Patch};
use kerf_modules::Builtin;
use serde_json::json;

use crate::{Session, SessionError, Suggest};

const BASE: &str = "kerf 1\ntempo 120\n\
module clock Tally\n\
module rhythm Ruler\n\
module out Finish\n\
cable clock.clock -> rhythm.clock\n";

fn open(text: &str) -> Session<Builtin> {
    Session::open(Builtin, parse(text).unwrap()).unwrap()
}

fn cmd(v: serde_json::Value) -> Command {
    serde_json::from_value(v).unwrap()
}

fn add(s: &mut Session<Builtin>, kind: &str, id: &str, near: Option<&str>) {
    s.apply(cmd(
        json!({"op":"add_module","kind":kind,"id":id,"auto_patch":true,"near":near}),
    ))
    .unwrap();
}

fn has(s: &Session<Builtin>, from: &str, to: &str) -> bool {
    s.patch()
        .cables
        .iter()
        .any(|c| c.from.to_string() == from && c.to.to_string() == to)
}

fn cables(s: &Session<Builtin>) -> Vec<String> {
    s.patch()
        .cables
        .iter()
        .map(|c| format!("{} -> {}", c.from, c.to))
        .collect()
}

/// True when no module can reach itself through the cables.
fn acyclic(p: &Patch) -> bool {
    let reach = |from: &str| {
        let mut seen = vec![from.to_owned()];
        let mut i = 0;
        while i < seen.len() {
            let here = seen[i].clone();
            for c in p.cables.iter().filter(|c| c.from.module.as_str() == here) {
                if c.to.module.as_str() == from {
                    return true;
                }
                if !seen.iter().any(|s| s == c.to.module.as_str()) {
                    seen.push(c.to.module.to_string());
                }
            }
            i += 1;
        }
        false
    };
    !p.modules.iter().any(|m| reach(m.id.as_str()))
}

#[test]
fn a_mortise_takes_the_clock_and_goes_to_the_output() {
    let mut s = open(BASE);
    add(&mut s, "Mortise", "kick", None);
    assert!(has(&s, "rhythm.a", "kick.trig") || has(&s, "clock.clock", "kick.trig"));
    assert!(
        has(&s, "kick.out", "out.l") && has(&s, "kick.out", "out.r"),
        "{:?}",
        cables(&s)
    );
    assert!(acyclic(s.patch()));
    assert_eq!(s.last_plan().len(), cables(&s).len() - 1);
}

#[test]
fn a_mortise_in_an_empty_patch_has_nothing_to_wire() {
    let mut s = Session::new(Builtin);
    add(&mut s, "Mortise", "kick", None);
    assert!(s.patch().cables.is_empty());
    assert!(s.last_plan().is_empty());
}

#[test]
fn a_veneer_prefers_the_router() {
    let mut s = open(&format!(
        "{BASE}module mix Router\ncable mix.mix_l -> out.l\ncable mix.mix_r -> out.r\n"
    ));
    add(&mut s, "Veneer", "loop", None);
    assert!(has(&s, "loop.l", "mix.in1"), "{:?}", cables(&s));
    // A stereo pair shares one channel rather than using up two.
    assert!(has(&s, "loop.r", "mix.in1"), "{:?}", cables(&s));
    assert!(acyclic(s.patch()));
}

#[test]
fn a_chisel_takes_pitch_and_a_gate_less_voice_goes_to_the_output() {
    let text = format!("{BASE}module seq Dovetail\ncable clock.clock -> seq.clock\n");
    let mut s = open(&text);
    add(&mut s, "Chisel", "osc", None);
    assert!(has(&s, "seq.pitch", "osc.voct"), "{:?}", cables(&s));
    assert!(
        cables(&s)
            .iter()
            .any(|c| c.starts_with("osc.") && c.contains("out."))
    );
    assert!(acyclic(s.patch()));
}

#[test]
fn a_bandsaw_takes_audio_and_the_clock() {
    let text = format!(
        "{BASE}module kick Mortise\ncable rhythm.a -> kick.trig\ncable kick.out -> out.l\n"
    );
    let mut s = open(&text);
    add(&mut s, "Bandsaw", "echo", None);
    assert!(has(&s, "clock.clock", "echo.clock"), "{:?}", cables(&s));
    assert!(
        cables(&s).iter().any(|c| c.ends_with("-> echo.in")),
        "{:?}",
        cables(&s)
    );
    assert!(
        cables(&s)
            .iter()
            .any(|c| c.starts_with("echo.") && c.contains("out."))
    );
    assert!(acyclic(s.patch()));
}

#[test]
fn near_wires_only_between_the_two_modules() {
    let text = format!("{BASE}module kick Mortise\nmodule mix Router\n");
    let mut s = open(&text);
    s.apply(cmd(
        json!({"op":"auto_patch","module":"rhythm","near":"kick"}),
    ))
    .unwrap();
    assert!(has(&s, "rhythm.a", "kick.trig"), "{:?}", cables(&s));
    assert!(!cables(&s).iter().any(|c| c.contains("out.")));

    s.apply(cmd(json!({"op":"auto_patch","module":"kick","near":"mix"})))
        .unwrap();
    assert!(
        cables(&s)
            .iter()
            .any(|c| c.starts_with("kick.out -> mix.in")),
        "{:?}",
        cables(&s)
    );
}

#[test]
fn auto_patch_is_idempotent_and_never_overwrites() {
    let mut s = open(BASE);
    add(&mut s, "Mortise", "kick", None);
    let before = cables(&s);
    let steps = s.undo.len();
    s.apply(cmd(json!({"op":"auto_patch","module":"kick"})))
        .unwrap();
    assert_eq!(cables(&s), before);
    assert_eq!(s.undo.len(), steps, "a no-op is not an undo step");
    assert!(s.last_plan().is_empty());
}

#[test]
fn add_and_patch_is_one_undo_step() {
    let mut s = open(BASE);
    add(&mut s, "Mortise", "kick", None);
    s.apply(Command::Undo).unwrap();
    assert!(s.patch().module(&"kick".parse().unwrap()).is_none());
    assert_eq!(cables(&s).len(), 1);
}

#[test]
fn suggestions_match_what_auto_patch_adds() {
    let s = open(BASE);
    let ask = Suggest {
        kind: Some("Mortise".into()),
        ..Suggest::default()
    };
    let plan = s.suggest_cables(&ask).unwrap();
    assert!(!plan.is_empty());
    assert!(
        plan.iter()
            .all(|c| c.to.to_string().starts_with("out.") || c.to.module.as_str() == "new")
    );
    let mut applied = open(BASE);
    applied
        .apply(cmd(
            json!({"op":"add_module","kind":"Mortise","id":"new","auto_patch":true}),
        ))
        .unwrap();
    let mut a: Vec<_> = plan
        .iter()
        .map(|c| format!("{} -> {}", c.from, c.to))
        .collect();
    let mut b: Vec<_> = applied
        .last_plan()
        .iter()
        .map(|c| format!("{} -> {}", c.from, c.to))
        .collect();
    a.sort();
    b.sort();
    assert_eq!(a, b);
    assert!(
        s.patch().module(&"new".parse().unwrap()).is_none(),
        "read-only"
    );
}

#[test]
fn replace_text_is_one_undoable_step() {
    let mut s = open(BASE);
    let text = "kerf 1\ntempo 90\nmodule solo Mortise\n";
    s.apply(Command::ReplaceText { text: text.into() }).unwrap();
    assert_eq!(s.patch().modules.len(), 1);
    assert_eq!(s.patch().tempo, 90.0);
    s.apply(Command::Undo).unwrap();
    assert_eq!(s.patch().modules.len(), 3);
}

#[test]
fn replace_text_errors_name_line_and_column() {
    let mut s = open(BASE);
    let before = s.patch().clone();
    let err = s
        .apply(Command::ReplaceText {
            text: "kerf 1\nmodule a Mortise decay=oops\n".into(),
        })
        .unwrap_err();
    let SessionError::BadText(message) = err else {
        panic!("{err:?}")
    };
    assert!(message.starts_with("line 2, column "), "{message}");
    assert_eq!(s.patch(), &before);
}

#[test]
fn move_module_changes_its_part() {
    let mut s = open("kerf 1\nmodule a Mortise\npart p \"P\" colour #112233\n  module b Ruler\n");
    s.apply(cmd(json!({"op":"move_module","module":"a","part":"p"})))
        .unwrap();
    assert_eq!(
        s.patch().module(&"a".parse().unwrap()).unwrap().part,
        Some("p".parse().unwrap())
    );
    s.apply(cmd(json!({"op":"move_module","module":"a","part":null})))
        .unwrap();
    assert_eq!(s.patch().module(&"a".parse().unwrap()).unwrap().part, None);
}

#[test]
fn part_level_is_validated_and_undoable() {
    let mut s = open("kerf 1\npart p \"P\" colour #112233\n  module b Ruler\n");
    s.apply(cmd(json!({"op":"set_part_level","part":"p","level":0.5})))
        .unwrap();
    assert_eq!(s.patch().parts[0].level, 0.5);
    assert!(
        s.apply(cmd(json!({"op":"set_part_level","part":"p","level":1.5})))
            .is_err()
    );
    assert!(
        s.apply(cmd(json!({"op":"set_part_level","part":"q","level":0.5})))
            .is_err()
    );
    s.apply(Command::Undo).unwrap();
    assert_eq!(s.patch().parts[0].level, 1.0);
}

#[test]
fn diagnostics_point_at_line_and_column() {
    let text =
        "kerf 1\nmodule a Mortise\nmodule b Foo\ncable a.out -> c.in\ncable a.nope -> a.trig\n";
    let d = diagnose(&Builtin, text);
    let at = |line| {
        d.iter()
            .find(|d| d.line == line)
            .unwrap_or_else(|| panic!("{d:?}"))
    };
    assert_eq!((at(3).col, at(3).end_col), (10, 13));
    assert_eq!(at(3).severity, Severity::Error);
    assert!(at(4).message.contains('c'));
    assert!(at(5).message.contains("nope"));
}

#[test]
fn presets_have_no_error_diagnostics() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let errors: Vec<_> = diagnose(&Builtin, &text)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{}: {errors:?}", path.display());
    }
}

#[test]
fn the_format_guide_examples_open() {
    let mut open_blocks = 0;
    let mut block: Option<String> = None;
    for line in crate::format_guide().lines() {
        match (&mut block, line) {
            (None, "```kerf complete") => block = Some(String::new()),
            (Some(b), "```") => {
                let patch = parse(b).unwrap_or_else(|e| panic!("{e}\n{b}"));
                Session::open(Builtin, patch).unwrap_or_else(|e| panic!("{e}\n{b}"));
                let errors = diagnose(&Builtin, b);
                assert!(errors.is_empty(), "{errors:?}\n{b}");
                open_blocks += 1;
                block = None;
            }
            (Some(b), _) => {
                b.push_str(line);
                b.push('\n');
            }
            _ => {}
        }
    }
    assert_eq!(open_blocks, 3);
}

#[test]
fn completions_and_docs_come_from_the_catalogue() {
    let c = crate::completions(&Builtin);
    let words: Vec<_> = c.keywords.iter().map(|k| k.word).collect();
    for w in [
        "kerf", "tempo", "part", "cable", "scene", "sample=", "level=",
    ] {
        assert!(words.contains(&w), "{w}");
    }
    assert_eq!(c.kinds.len(), kerf_core::Catalogue::all(&Builtin).len());
    let mortise = c.kinds.iter().find(|k| k.kind == "Mortise").unwrap();
    assert!(mortise.params.iter().all(|p| !p.help.is_empty()));
    assert_eq!(mortise.outputs[0].kind, "audio");
    assert!(!c.styles.is_empty() && !c.keys.is_empty() && !c.scales.is_empty());
    let docs = crate::language_docs(&Builtin);
    let ids: Vec<_> = docs.sections.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"file-anatomy") && ids.contains(&"modules-time"));
    assert!(docs.sections.iter().all(|s| !s.markdown.is_empty()));
}
