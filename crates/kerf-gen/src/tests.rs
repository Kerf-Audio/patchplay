use kerf_core::text::{parse, print};
use kerf_core::{Module, Patch};

use super::*;

const STYLES: [&str; 6] = [
    "acid-line",
    "foundry",
    "dub-chamber",
    "slow-grain",
    "broken-joinery",
    "tape-keys",
];
/// Styles whose bass goes Dovetail to Chisel, with the key in Chisel's tune.
pub(super) const TUNED_STYLES: [&str; 2] = ["acid-line", "foundry"];
/// Styles whose pitches go through a Fret that carries the key.
pub(super) const NEW_STYLES: [&str; 4] = [
    "dub-chamber",
    "slow-grain",
    "broken-joinery",
    "tape-keys",
];

fn text(p: &Patch) -> String {
    print(p)
}

pub(super) fn param(p: &Patch, module: &str, key: &str) -> f32 {
    p.modules
        .iter()
        .find(|m| m.id.as_str() == module)
        .and_then(|m| m.params.iter().find(|v| v.key.as_str() == key))
        .unwrap_or_else(|| panic!("{module}.{key}"))
        .value
}

fn steps(v: f32) -> i32 {
    1 + (v * 15.0).round() as i32
}

#[test]
fn a_hundred_seeds_per_style_pass_safe_check() {
    for style in STYLES {
        for seed in 1..=100 {
            let patch = random(style, seed).unwrap_or_else(|e| panic!("{style} {seed}: {e}"));
            let report = safe_check(&patch);
            assert!(report.pass, "{style} {seed}: {:?}", report.problems);
            assert!(report.silent_kinds.is_empty(), "{style} {seed}");
        }
    }
}

#[test]
fn random_is_deterministic_and_round_trips_as_text() {
    for style in STYLES {
        let a = random(style, 7).unwrap();
        assert_eq!(text(&a), text(&random(style, 7).unwrap()));
        assert_ne!(text(&a), text(&random(style, 8).unwrap()));
        assert_eq!(parse(&text(&a)).unwrap(), a);
    }
}

#[test]
fn unknown_styles_and_strengths_are_errors() {
    assert_eq!(
        random("nope", 1),
        Err(GenError::UnknownStyle("nope".into()))
    );
    let p = random("foundry", 1).unwrap();
    assert_eq!(evolve(&p, 1.5, 1), Err(GenError::BadStrength(1.5)));
}

#[test]
fn evolve_is_deterministic_and_changes_things() {
    let p = random("acid-line", 3).unwrap();
    let a = evolve(&p, 0.6, 5).unwrap();
    assert_eq!(text(&a), text(&evolve(&p, 0.6, 5).unwrap()));
    assert_ne!(text(&a), text(&p));
    assert_eq!(text(&evolve(&p, 0.0, 5).unwrap()), text(&p));
}

fn lock_everything_but(p: &mut Patch, free: &str) {
    for m in &mut p.modules {
        m.locked = m.id.as_str() != free;
    }
}

#[test]
fn evolve_leaves_locked_things_byte_identical() {
    for style in STYLES {
        let mut p = random(style, 11).unwrap();
        // A locked module, a locked param and a locked part.
        p.modules
            .iter_mut()
            .find(|m| m.kind == "Dovetail")
            .unwrap()
            .locked = true;
        p.modules
            .iter_mut()
            .find(|m| matches!(m.kind.as_str(), "Plane" | "Ply"))
            .unwrap()
            .params[0]
            .locked = true;
        p.parts[0].locked = true;
        let before = p.clone();
        for seed in 1..=8 {
            let child = evolve(&p, 1.0, seed).unwrap();
            for (m, was) in child.modules.iter().zip(&before.modules) {
                for (v, w) in m.params.iter().zip(&was.params) {
                    if before.is_locked(was, w) {
                        assert_eq!(
                            v.value.to_bits(),
                            w.value.to_bits(),
                            "{style} {}.{}",
                            m.id,
                            v.key
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn evolve_with_everything_locked_changes_no_param() {
    let mut p = random("foundry", 4).unwrap();
    lock_everything_but(&mut p, "no-such-module");
    let child = evolve(&p, 1.0, 9).unwrap();
    assert_eq!(text(&child), text(&p));
}

#[test]
fn evolve_never_rewires_a_locked_module() {
    let mut p = random("acid-line", 21).unwrap();
    for m in p.modules.iter_mut().filter(|m| m.id.as_str() == "osc1") {
        m.locked = true;
    }
    for seed in 1..=20 {
        let child = evolve(&p, 1.0, seed).unwrap();
        let into_osc = |p: &Patch| {
            p.cables
                .iter()
                .filter(|c| c.from.module.as_str() == "osc1" || c.to.module.as_str() == "osc1")
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(into_osc(&child), into_osc(&p), "seed {seed}");
    }
}

#[test]
fn evolved_patches_still_pass_and_keep_the_seed() {
    let p = random("foundry", 2).unwrap();
    for seed in 1..=10 {
        let child = evolve(&p, 0.8, seed).unwrap();
        assert!(safe_check(&child).pass);
        assert_eq!(child.style, p.style);
        assert_eq!(child.tempo, p.tempo);
    }
}

#[test]
fn evolve_can_swap_a_layout() {
    let p = (1..40)
        .map(|s| random("acid-line", s).unwrap())
        .find(|p| p.modules.iter().all(|m| m.id.as_str() != "env2"))
        .unwrap();
    let swapped = (1..60).any(|seed| {
        evolve(&p, 1.0, seed)
            .unwrap()
            .modules
            .iter()
            .any(|m| m.id.as_str() == "env2")
    });
    assert!(swapped);
}

fn values(p: &Patch) -> Vec<(String, String, f32)> {
    p.modules
        .iter()
        .flat_map(|m| {
            m.params
                .iter()
                .map(|v| (m.id.to_string(), v.key.to_string(), v.value))
        })
        .collect()
}

#[test]
fn breed_children_hold_only_parent_values() {
    for style in STYLES {
        let a = random(style, 31).unwrap();
        let b = random(style, 32).unwrap();
        let (va, vb) = (values(&a), values(&b));
        for seed in 1..=12 {
            let child = breed(&a, &b, seed).unwrap();
            assert!(safe_check(&child).pass);
            assert!([a.tempo, b.tempo].contains(&child.tempo));
            assert!([&a.key, &b.key].contains(&&child.key));
            for (m, k, v) in values(&child) {
                let ok = va
                    .iter()
                    .chain(&vb)
                    .any(|(pm, pk, pv)| *pm == m && *pk == k && pv.to_bits() == v.to_bits());
                assert!(ok, "{style} {seed}: {m}.{k}={v} is from neither parent");
            }
        }
    }
}

#[test]
fn breed_respects_locks_and_is_deterministic() {
    let mut a = random("foundry", 41).unwrap();
    let b = random("foundry", 42).unwrap();
    a.parts[0].locked = true;
    a.modules
        .iter_mut()
        .find(|m| m.kind == "Plane")
        .unwrap()
        .params[0]
        .locked = true;
    let child = breed(&a, &b, 3).unwrap();
    assert_eq!(text(&child), text(&breed(&a, &b, 3).unwrap()));
    for (m, was) in child.modules.iter().zip(&a.modules) {
        for (v, w) in m.params.iter().zip(&was.params) {
            if a.is_locked(was, w) {
                assert_eq!(v.value.to_bits(), w.value.to_bits());
            }
        }
    }
}

#[test]
fn the_rhythm_rule_holds_for_random_and_evolved_patches() {
    for style in STYLES {
        for seed in 1..=40 {
            let random = random(style, seed).unwrap();
            let evolved = evolve(&random, 1.0, seed).unwrap();
            for p in [random, evolved] {
                for m in &p.modules {
                    check_rhythm(style, m);
                }
            }
        }
    }
}

fn check_rhythm(style: &str, m: &Module) {
    let get = |k: &str| m.params.iter().find(|v| v.key.as_str() == k).unwrap().value;
    match m.kind.as_str() {
        "Ruler" => {
            if get("hits_a") > 0.0 {
                assert!([8, 16].contains(&steps(get("steps_a"))), "{style} {}", m.id);
            }
            let others: Vec<i32> = ["b", "c", "d"]
                .iter()
                .filter(|l| get(&format!("hits_{l}")) > 0.0)
                .map(|l| steps(get(&format!("steps_{l}"))))
                .collect();
            assert!(
                others.iter().all(|&n| n != 16),
                "{style} {}: {others:?}",
                m.id
            );
            let mut sorted = others.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                others.len(),
                "{style} {}: repeated lane length",
                m.id
            );
            if !others.is_empty() {
                assert!(get("chance") < 1.0, "{style} {}", m.id);
            }
        }
        "Dovetail" => {
            assert_ne!(
                steps(get("steps_a")),
                steps(get("steps_b")),
                "{style} {}",
                m.id
            );
        }
        _ => {}
    }
}

#[test]
fn check_reports_serialise_and_name_problems() {
    let report = safe_check(&Patch::default());
    assert!(!report.pass && !report.problems.is_empty());
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["pass"], false);
}

#[test]
fn presets_pass_the_check() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "kerf"))
        .collect();
    files.sort();
    assert!(files.len() >= 78, "only {} presets found", files.len());
    for file in files {
        let name = file.file_stem().unwrap().to_string_lossy().into_owned();
        let src = std::fs::read_to_string(&file).unwrap();
        let report = safe_check(&parse(&src).unwrap());
        assert!(report.pass, "{name}: {:?}", report.problems);
    }
}

#[test]
fn evolve_and_breed_work_on_every_new_style() {
    for style in NEW_STYLES {
        let a = random(style, 51).unwrap();
        let b = random(style, 52).unwrap();
        let evolved = evolve(&a, 0.7, 3).unwrap();
        assert_ne!(text(&evolved), text(&a), "{style}");
        assert_eq!(
            text(&evolved),
            text(&evolve(&a, 0.7, 3).unwrap()),
            "{style}"
        );
        assert!(safe_check(&evolved).pass, "{style}");
        assert_eq!(evolved.style, a.style, "{style}");
        let child = breed(&a, &b, 3).unwrap();
        assert!(safe_check(&child).pass, "{style}");
        assert_eq!(text(&child), text(&breed(&a, &b, 3).unwrap()), "{style}");
        assert_eq!(child.style, a.style, "{style}");
    }
}

#[test]
fn styles_with_drums_keep_a_straight_anchor() {
    for style in STYLES {
        for seed in 1..=20 {
            let p = random(style, seed).unwrap();
            if p.modules.iter().all(|m| m.kind != "Mortise") {
                continue;
            }
            let anchored = p.modules.iter().filter(|m| m.kind == "Ruler").any(|m| {
                let get = |k: &str| m.params.iter().find(|v| v.key.as_str() == k).unwrap().value;
                get("hits_a") > 0.0 && steps(get("steps_a")) == 16
            });
            assert!(anchored, "{style} {seed}: no 16-step lane A");
        }
    }
}
