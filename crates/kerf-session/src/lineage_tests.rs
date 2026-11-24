use kerf_core::text::print;
use kerf_core::{BreedWith, Command};
use kerf_modules::Builtin;

use crate::{Origin, Session, SessionError};

fn random(style: &str, seed: u64) -> Command {
    Command::Random {
        style: style.into(),
        seed: Some(seed),
    }
}

fn evolve(seed: u64) -> Command {
    Command::Evolve {
        strength: 0.5,
        seed: Some(seed),
    }
}

fn text(s: &Session<Builtin>) -> String {
    print(s.patch())
}

#[test]
fn checkout_restores_the_exact_patch() {
    let mut s = Session::new(Builtin);
    s.apply(random("foundry", 1)).unwrap();
    let first = text(&s);
    s.apply(evolve(2)).unwrap();
    s.apply(evolve(3)).unwrap();
    assert_ne!(text(&s), first);
    s.apply(Command::Checkout { node: 1 }).unwrap();
    assert_eq!(text(&s), first);
    assert_eq!(s.lineage().current, 1);
}

#[test]
fn nodes_record_parents_origins_and_snapshots() {
    let mut s = Session::new(Builtin);
    s.apply(random("acid-line", 4)).unwrap();
    s.apply(evolve(9)).unwrap();
    let l = s.lineage();
    assert_eq!(l.nodes.len(), 3);
    assert_eq!(l.nodes[0].origin, Origin::Root);
    assert_eq!(l.nodes[1].parents, [0]);
    assert_eq!(l.nodes[2].parents, [1]);
    assert_eq!(
        l.nodes[2].origin,
        Origin::Evolve {
            strength: 0.5,
            seed: 9
        }
    );
    assert_eq!(l.nodes[2].patch, *s.patch());
    assert_eq!(l.current, 2);
}

#[test]
fn an_omitted_seed_is_the_node_count_and_is_recorded() {
    let mut s = Session::new(Builtin);
    s.apply(Command::Random {
        style: "foundry".into(),
        seed: None,
    })
    .unwrap();
    s.apply(Command::Evolve {
        strength: 0.5,
        seed: None,
    })
    .unwrap();
    let l = s.lineage();
    assert_eq!(l.nodes[1].origin.seed(), Some(1));
    assert_eq!(l.nodes[2].origin.seed(), Some(2));

    // The same seed given explicitly makes the same patch.
    let mut t = Session::new(Builtin);
    t.apply(random("foundry", 1)).unwrap();
    assert_eq!(l.nodes[1].patch, t.lineage().nodes[1].patch);
}

#[test]
fn lineage_serialises_to_json_with_kerf_text() {
    let mut s = Session::new(Builtin);
    s.apply(random("foundry", 1)).unwrap();
    let json = serde_json::to_value(s.lineage()).unwrap();
    assert_eq!(json["current"], 1);
    assert_eq!(json["nodes"][0]["origin"]["kind"], "root");
    assert_eq!(json["nodes"][1]["origin"]["kind"], "random");
    assert_eq!(json["nodes"][1]["origin"]["style"], "foundry");
    assert!(
        json["nodes"][1]["patch"]
            .as_str()
            .unwrap()
            .starts_with("kerf 1")
    );
}

#[test]
fn lineage_is_deterministic() {
    let run = || {
        let mut s = Session::new(Builtin);
        s.apply(random("foundry", 5)).unwrap();
        s.apply(evolve(6)).unwrap();
        s.apply(Command::Checkout { node: 1 }).unwrap();
        serde_json::to_string(s.lineage()).unwrap()
    };
    assert_eq!(run(), run());
}

#[test]
fn undo_and_redo_cross_generator_commands() {
    let mut s = Session::new(Builtin);
    s.apply(random("foundry", 1)).unwrap();
    let first = text(&s);
    s.apply(evolve(2)).unwrap();
    let second = text(&s);
    s.apply(Command::Undo).unwrap();
    assert_eq!((text(&s), s.lineage().current), (first.clone(), 1));
    s.apply(Command::Redo).unwrap();
    assert_eq!((text(&s), s.lineage().current), (second, 2));
    s.apply(Command::Checkout { node: 1 }).unwrap();
    s.apply(Command::Undo).unwrap();
    assert_eq!(s.lineage().current, 2);
    s.apply(Command::Undo).unwrap();
    s.apply(Command::Undo).unwrap();
    assert_eq!(s.patch().modules.len(), 0);
    assert_eq!(s.lineage().current, 0);
}

#[test]
fn hand_edits_are_saved_as_a_node_before_a_generator_command() {
    let mut s = Session::new(Builtin);
    s.apply(random("foundry", 1)).unwrap();
    s.apply(Command::SetTempo { bpm: 141.0 }).unwrap();
    s.apply(evolve(2)).unwrap();
    let l = s.lineage();
    assert_eq!(l.nodes.len(), 4);
    assert_eq!(l.nodes[2].origin, Origin::Edited);
    assert_eq!(l.nodes[2].patch.tempo, 141.0);
    assert_eq!(l.nodes[3].parents, [2]);
}

#[test]
fn breed_takes_a_node_or_patch_text() {
    let mut s = Session::new(Builtin);
    s.apply(random("foundry", 1)).unwrap();
    s.apply(random("foundry", 2)).unwrap();
    s.apply(Command::Breed {
        with: BreedWith::Node(1),
        seed: Some(3),
    })
    .unwrap();
    assert_eq!(s.lineage().current_node().parents, [2, 1]);

    let other = kerf_gen::random("acid-line", 8).unwrap();
    s.apply(Command::Breed {
        with: BreedWith::Patch(print(&other)),
        seed: Some(4),
    })
    .unwrap();
    let l = s.lineage();
    assert_eq!(l.nodes[4].origin, Origin::Imported);
    assert_eq!(l.current_node().parents, [3, 4]);
}

#[test]
fn failures_leave_no_trace() {
    let mut s = Session::new(Builtin);
    s.apply(random("foundry", 1)).unwrap();
    let before = (text(&s), s.lineage().clone());
    assert_eq!(
        s.apply(Command::Checkout { node: 9 }),
        Err(SessionError::UnknownNode(9))
    );
    assert!(matches!(
        s.apply(random("nope", 1)),
        Err(SessionError::Gen(_))
    ));
    assert!(matches!(
        s.apply(Command::Breed {
            with: BreedWith::Patch("not a patch".into()),
            seed: Some(1)
        }),
        Err(SessionError::BadPatch(_))
    ));
    assert_eq!((text(&s), s.lineage().clone()), before);
    assert_eq!(s.undo.len(), 1);
}
