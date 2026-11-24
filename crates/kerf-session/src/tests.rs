use kerf_core::text::{parse, print};
use kerf_core::{Command, LockTarget, Patch, Scene};
use kerf_modules::Builtin;

use crate::{Session, SessionError};

fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}

fn add(kind: &str, part: Option<&str>) -> Command {
    Command::AddModule {
        kind: kind.into(),
        id: None,
        part: part.map(id),
        auto_patch: false,
        near: None,
    }
}

fn part(name: &str) -> Command {
    Command::AddPart {
        id: id(name),
        name: name.to_uppercase(),
        colour: id("#123456"),
    }
}

fn set(module: &str, param: &str, value: f32) -> Command {
    Command::SetParam {
        module: id(module),
        param: id(param),
        value,
    }
}

fn set_many(module: &str, pairs: &[(&str, f32)]) -> Command {
    Command::SetParams {
        module: id(module),
        params: pairs
            .iter()
            .map(|&(param, value)| kerf_core::ParamSet {
                param: id(param),
                value,
            })
            .collect(),
    }
}

fn value(s: &Session<Builtin>, module: &str, param: &str) -> f32 {
    s.patch()
        .module(&id(module))
        .unwrap()
        .param(&id(param))
        .unwrap()
        .value
}

fn session(cmds: impl IntoIterator<Item = Command>) -> Session<Builtin> {
    let mut s = Session::new(Builtin);
    for c in cmds {
        s.apply(c).unwrap();
    }
    s
}

#[test]
fn added_modules_get_fresh_ids_and_catalogue_defaults() {
    let s = session([add("plane", None), add("Plane", None)]);
    let ids: Vec<_> = s.patch().modules.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["plane1", "plane2"]);
    assert_eq!(s.patch().modules[0].kind, "Plane");
    assert_eq!(value(&s, "plane1", "cutoff"), 0.45);
}

#[test]
fn modules_stay_grouped_by_part_so_text_round_trips() {
    let s = session([
        part("a"),
        part("b"),
        add("Chisel", Some("b")),
        add("Plane", Some("a")),
        add("Tally", None),
    ]);
    let order: Vec<_> = s.patch().modules.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(order, ["tally1", "plane1", "chisel1"]);
    assert_eq!(&parse(&print(s.patch())).unwrap(), s.patch());
}

#[test]
fn rejected_commands_leave_patch_and_history_alone() {
    let mut s = session([add("Chisel", None), add("Plane", None)]);
    let before = s.patch().clone();
    let bad = [
        (
            Command::Connect {
                from: id("plane1.in"),
                to: id("chisel1.voct"),
            },
            "not an output",
        ),
        (
            Command::Connect {
                from: id("chisel1.saw"),
                to: id("plane1.out"),
            },
            "not an input",
        ),
        (set("plane1", "wobble", 0.1), "no param"),
        (set("plane1", "cutoff", f32::NAN), "outside"),
        (add("Planer", None), "no module kind"),
        (add("Plane", Some("nowhere")), "no part"),
        (Command::SetTempo { bpm: 900.0 }, "outside"),
        (
            Command::Morph {
                a: Scene::A,
                b: Scene::B,
                t: 0.5,
            },
            "empty",
        ),
    ];
    for (c, needle) in bad {
        let err = s.apply(c.clone()).unwrap_err().to_string();
        assert!(err.contains(needle), "{c:?}: {err}");
        assert_eq!(s.patch(), &before);
    }
    s.apply(Command::Undo).unwrap();
    assert_eq!(
        s.patch().modules.len(),
        1,
        "undo stepped over the rejected commands"
    );
}

#[test]
fn undo_and_redo_walk_history_and_new_edits_drop_redo() {
    let mut s = session([add("Plane", None), set("plane1", "cutoff", 0.9)]);
    s.apply(Command::Undo).unwrap();
    assert_eq!(value(&s, "plane1", "cutoff"), 0.45);
    s.apply(Command::Redo).unwrap();
    assert_eq!(value(&s, "plane1", "cutoff"), 0.9);
    s.apply(Command::Undo).unwrap();
    s.apply(set("plane1", "res", 0.7)).unwrap();
    assert_eq!(s.apply(Command::Redo), Err(SessionError::NothingToRedo));
    s.apply(Command::Undo).unwrap();
    s.apply(Command::Undo).unwrap();
    assert!(s.patch().modules.is_empty());
    assert_eq!(s.apply(Command::Undo), Err(SessionError::NothingToUndo));
}

#[test]
fn set_param_clamps_to_unit_range() {
    let s = session([
        add("Plane", None),
        set("plane1", "cutoff", 7.0),
        set("plane1", "res", -1.0),
    ]);
    assert_eq!(value(&s, "plane1", "cutoff"), 1.0);
    assert_eq!(value(&s, "plane1", "res"), 0.0);
}

#[test]
fn removing_a_module_removes_its_cables_and_scene_values() {
    let s = session([
        add("Chisel", None),
        add("Plane", None),
        Command::Connect {
            from: id("chisel1.saw"),
            to: id("plane1.in"),
        },
        Command::StoreScene { scene: Scene::A },
        Command::RemoveModule { id: id("chisel1") },
    ]);
    assert!(s.patch().cables.is_empty());
    assert!(
        s.patch().scenes[&Scene::A]
            .iter()
            .all(|v| v.module.as_str() == "plane1")
    );
}

#[test]
fn morph_glides_unlocked_params_only() {
    let mut s = session([
        part("bass"),
        add("Plane", None),
        add("Chisel", Some("bass")),
        set("plane1", "cutoff", 0.0),
        set("plane1", "res", 0.0),
        set("chisel1", "tune", 0.0),
        Command::StoreScene { scene: Scene::A },
        set("plane1", "cutoff", 1.0),
        set("plane1", "res", 1.0),
        set("chisel1", "tune", 1.0),
        Command::StoreScene { scene: Scene::B },
        Command::Lock {
            target: LockTarget::Param {
                module: id("plane1"),
                param: id("res"),
            },
            locked: true,
        },
        Command::Lock {
            target: LockTarget::Part { part: id("bass") },
            locked: true,
        },
    ]);
    s.apply(Command::Morph {
        a: Scene::A,
        b: Scene::B,
        t: 0.25,
    })
    .unwrap();
    assert_eq!(value(&s, "plane1", "cutoff"), 0.25);
    assert_eq!(value(&s, "plane1", "res"), 1.0, "param lock");
    assert_eq!(value(&s, "chisel1", "tune"), 1.0, "part lock");
}

#[test]
fn open_checks_patch_against_catalogue_and_fills_defaults() {
    let good = parse(
        "kerf 1\nmodule p1 plane res=0.9 cutoff=0.1\nmodule o1 Chisel\ncable o1.saw -> p1.in\n",
    )
    .unwrap();
    let s = Session::open(Builtin, good).unwrap();
    let m = &s.patch().modules[0];
    assert_eq!(m.kind, "Plane");
    let keys: Vec<_> = m.params.iter().map(|v| (v.key.as_str(), v.value)).collect();
    assert_eq!(
        keys,
        [("cutoff", 0.1), ("res", 0.9), ("mode", 0.0), ("drive", 0.2)]
    );

    for (src, needle) in [
        ("kerf 1\nmodule p1 Planer\n", "kind"),
        ("kerf 1\nmodule p1 Plane wobble=0.1\n", "wobble"),
        (
            "kerf 1\nmodule p1 Plane\nmodule o1 Chisel\ncable p1.in -> o1.voct\n",
            "not an output",
        ),
        ("kerf 1\nscene A p1.cutoff=0.1\n", "p1"),
    ] {
        let err = Session::open(Builtin, parse(src).unwrap())
            .err()
            .expect(src)
            .to_string();
        assert!(err.contains(needle), "{src:?}: {err}");
    }
    assert!(
        Session::open(
            Builtin,
            Patch {
                tempo: 1.0,
                ..Patch::default()
            }
        )
        .is_err()
    );
}

fn sample_of(s: &Session<Builtin>, module: &str) -> Option<String> {
    s.patch().module(&id(module)).unwrap().sample.clone()
}

fn set_sample(module: &str, path: Option<&str>) -> Command {
    Command::SetSample {
        module: id(module),
        path: path.map(Into::into),
    }
}

#[test]
fn set_sample_needs_a_sampler_and_undoes() {
    let mut s = session([
        add("Veneer", None),
        add("Splinter", None),
        add("Plane", None),
    ]);
    s.apply(set_sample("veneer1", Some("loops/break one.wav")))
        .unwrap();
    assert_eq!(
        sample_of(&s, "veneer1").as_deref(),
        Some("loops/break one.wav")
    );
    assert!(print(s.patch()).contains("sample=\"loops/break one.wav\""));

    for (cmd, needle) in [
        (
            set_sample("plane1", Some("a.wav")),
            "only a Veneer or Splinter",
        ),
        (set_sample("nope1", Some("a.wav")), "no module"),
        (set_sample("veneer1", Some("a\"b.wav")), "quotes"),
        (set_sample("veneer1", Some("  ")), "empty"),
    ] {
        let err = s.apply(cmd).unwrap_err().to_string();
        assert!(err.contains(needle), "{err}");
    }
    assert_eq!(
        sample_of(&s, "veneer1").as_deref(),
        Some("loops/break one.wav")
    );

    s.apply(set_sample("veneer1", None)).unwrap();
    assert_eq!(sample_of(&s, "veneer1"), None);
    s.apply(Command::Undo).unwrap();
    assert_eq!(
        sample_of(&s, "veneer1").as_deref(),
        Some("loops/break one.wav")
    );
    s.apply(Command::Undo).unwrap();
    assert_eq!(sample_of(&s, "veneer1"), None);

    // A Splinter takes a sample the same way.
    s.apply(set_sample("splinter1", Some("grains/pad.wav")))
        .unwrap();
    assert_eq!(
        sample_of(&s, "splinter1").as_deref(),
        Some("grains/pad.wav")
    );
    assert!(print(s.patch()).contains("sample=\"grains/pad.wav\""));
}

#[test]
fn open_rejects_a_sample_on_other_modules() {
    let bad = parse("kerf 1\nmodule p1 Plane sample=\"a.wav\"\n").unwrap();
    let err = Session::open(Builtin, bad).err().unwrap().to_string();
    assert!(err.contains("only a Veneer or Splinter"), "{err}");
    let ok = parse("kerf 1\nmodule v1 Veneer sample=\"a.wav\"\n").unwrap();
    let s = Session::open(Builtin, ok).unwrap();
    assert_eq!(sample_of(&s, "v1").as_deref(), Some("a.wav"));
}

#[test]
fn generators_leave_samples_alone() {
    let mut s = session([Command::Random {
        style: "foundry".into(),
        seed: Some(7),
    }]);
    for (module, path) in [("v1", "a.wav"), ("v2", "b.wav")] {
        s.apply(Command::AddModule {
            kind: "Veneer".into(),
            id: Some(id(module)),
            part: None,
            auto_patch: false,
            near: None,
        })
        .unwrap();
        s.apply(set_sample(module, Some(path))).unwrap();
    }
    s.apply(Command::Lock {
        target: LockTarget::Module { module: id("v2") },
        locked: true,
    })
    .unwrap();
    for seed in 0..5 {
        s.apply(Command::Evolve {
            strength: 1.0,
            seed: Some(seed),
        })
        .unwrap();
        assert_eq!(sample_of(&s, "v1").as_deref(), Some("a.wav"));
        assert_eq!(sample_of(&s, "v2").as_deref(), Some("b.wav"));
    }
    s.apply(Command::Breed {
        with: kerf_core::BreedWith::Node(0),
        seed: Some(3),
    })
    .unwrap();
    assert_eq!(sample_of(&s, "v1").as_deref(), Some("a.wav"));
    assert_eq!(sample_of(&s, "v2").as_deref(), Some("b.wav"));
}

#[test]
fn open_refuses_a_patch_that_the_text_format_cannot_hold() {
    let open = |edit: fn(&mut Patch)| {
        let mut p = parse("kerf 1\npart a \"A\" colour #123456\n").unwrap();
        edit(&mut p);
        Session::open(Builtin, p).err().map(|e| e.to_string())
    };
    let level = open(|p| p.parts[0].level = f32::NAN).unwrap();
    assert!(level.contains("part level"), "{level}");
    assert!(open(|p| p.parts[0].level = 1.5).is_some());
    assert!(open(|p| p.key = Some("A \"minor\"".into())).is_some());
    assert!(open(|p| p.key = Some(" A minor".into())).is_some());
    assert!(open(|p| p.tags = vec!["Four Four".into()]).is_some());
    assert!(open(|p| p.parts[0].level = 0.5).is_none());
    assert!(open(|p| p.key = Some("A minor".into())).is_none());
}

#[test]
fn set_params_is_one_undo_step() {
    let mut s = session([add("plane", None)]);
    s.apply(set_many("plane1", &[("cutoff", 0.1), ("res", 0.9)]))
        .unwrap();
    assert_eq!(value(&s, "plane1", "cutoff"), 0.1);
    assert_eq!(value(&s, "plane1", "res"), 0.9);
    s.apply(Command::Undo).unwrap();
    assert_eq!(value(&s, "plane1", "cutoff"), 0.45);
    assert_ne!(value(&s, "plane1", "res"), 0.9);
}

#[test]
fn set_params_with_a_bad_key_or_value_changes_nothing() {
    let mut s = session([add("plane", None)]);
    let before = s.patch().clone();
    assert!(
        s.apply(set_many("plane1", &[("cutoff", 0.1), ("nope", 0.5)]))
            .is_err()
    );
    assert!(
        s.apply(set_many("plane1", &[("cutoff", 0.1), ("res", f32::NAN)]))
            .is_err()
    );
    assert_eq!(s.patch(), &before);
    // Neither failure left an undo step behind: one undo removes the module added above.
    s.apply(Command::Undo).unwrap();
    assert!(s.patch().modules.is_empty());
}
