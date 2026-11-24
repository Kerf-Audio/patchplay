use kerf_core::text::parse;
use kerf_core::{Command, Patch};
use kerf_dsp::db_to_gain;
use kerf_modules::Builtin;
use kerf_session::Session;

use crate::part_gain::PartGain;
use crate::{Change, Engine, diff};

const RATE: f32 = 48_000.0;

fn patch(text: &str) -> Patch {
    parse(text).unwrap()
}

fn render(p: &Patch, seconds: f32) -> crate::Render {
    Engine::new(p, RATE)
        .unwrap()
        .render((seconds * RATE) as usize)
}

/// A kick through a saturator, both inside `drums`, to the output outside it.
fn drums(level: &str) -> Patch {
    patch(&format!(
        "kerf 1\ntempo 120\nmodule clock Tally\nmodule out Finish\n\
         part drums \"Drums\" colour #c94a3d {level}\n  module kick Mortise\n  module grit Rasp drive=0.8\n\
         cable clock.clock -> kick.trig\ncable kick.out -> grit.in\n\
         cable grit.out -> out.l\ncable grit.out -> out.r\n"
    ))
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

#[test]
fn level_one_is_unity_and_levels_scale_audio_leaving_the_part() {
    let plain = render(&drums(""), 2.0);
    let unity = render(&drums("level=1"), 2.0);
    assert_eq!(plain, unity, "level 1 changes nothing");
    // Quiet levels keep the output stage in its linear range: half the
    // level is half the amplitude, 6 dB down.
    let quiet = render(&drums("level=0.2"), 2.0);
    let half = render(&drums("level=0.1"), 2.0);
    assert!(quiet.rms() > db_to_gain(-60.0), "rms {}", quiet.rms());
    let ratio = half.rms() / quiet.rms();
    assert!((ratio - 0.5).abs() < 0.01, "ratio {ratio}");
    // The saturator is inside the part: its curve is not scaled, only its output.
    let scaled: Vec<f32> = quiet.left.iter().map(|x| x * 0.5).collect();
    assert!(max_diff(&scaled, &half.left) < 0.01);
}

#[test]
fn level_zero_silences_only_its_part() {
    let both = |level: &str| {
        patch(&format!(
            "kerf 1\ntempo 120\nmodule clock Tally\nmodule out Finish\n\
             part drums \"Drums\" colour #c94a3d {level}\n  module kick Mortise\n\
             part bass \"Bass\" colour #3d63c9\n  module osc Chisel\n\
             cable clock.clock -> kick.trig\ncable kick.out -> out.l\n\
             cable osc.saw -> out.r\n"
        ))
    };
    let silenced = render(&both("level=0"), 1.0);
    let bass_only = render(
        &patch(
            "kerf 1\ntempo 120\nmodule out Finish\n\
             part bass \"Bass\" colour #3d63c9\n  module osc Chisel\n\
             cable osc.saw -> out.r\n",
        ),
        1.0,
    );
    assert_eq!(
        silenced.left.iter().fold(0.0_f32, |m, x| m.max(x.abs())),
        0.0
    );
    assert_eq!(silenced.right, bass_only.right, "the bass is untouched");
    let normal = render(&both(""), 1.0);
    assert!(normal.left.iter().any(|x| x.abs() > 0.01));
}

#[test]
fn modules_outside_any_part_are_unaffected() {
    let loose = patch(
        "kerf 1\nmodule clock Tally\nmodule kick Mortise\nmodule out Finish\n\
         cable clock.clock -> kick.trig\ncable kick.out -> out.l\n",
    );
    let mut with_part = loose.clone();
    let mut s = Session::open(Builtin, with_part.clone()).unwrap();
    s.apply(Command::AddPart {
        id: "idle".parse().unwrap(),
        name: "Idle".into(),
        colour: "#112233".parse().unwrap(),
    })
    .unwrap();
    s.apply(Command::SetPartLevel {
        part: "idle".parse().unwrap(),
        level: 0.0,
    })
    .unwrap();
    with_part = s.patch().clone();
    assert_eq!(render(&loose, 1.0), render(&with_part, 1.0));
}

#[test]
fn a_running_engine_takes_level_changes_without_a_rebuild() {
    // Quiet levels keep the output stage in its linear range.
    let old = drums("level=0.2");
    let mut s = Session::open(Builtin, old.clone()).unwrap();
    s.apply(Command::SetPartLevel {
        part: "drums".parse().unwrap(),
        level: 0.1,
    })
    .unwrap();
    let changes = diff(&old, s.patch()).expect("a level is not a graph change");
    assert_eq!(changes, vec![Change::PartLevel(0, 0.1)]);

    let mut live = Engine::new(&old, RATE).unwrap();
    let before = live.render(2 * RATE as usize).rms();
    for c in changes {
        live.apply(c);
    }
    live.render((0.1 * RATE) as usize); // settle
    let after = live.render(2 * RATE as usize).rms();
    assert!((after / before - 0.5).abs() < 0.03, "{}", after / before);
}

#[test]
fn level_moves_are_smoothed() {
    let mut gain = PartGain::new(1.0);
    gain.set(0.0);
    gain.tick(PartGain::coefficient(RATE));
    assert!(
        gain.current > 0.99,
        "first sample moves {}",
        1.0 - gain.current
    );
    for _ in 0..RATE as usize {
        gain.tick(PartGain::coefficient(RATE));
    }
    assert_eq!(gain.current, 0.0);
}

const BASE: &str = "kerf 1\ntempo 120\nmodule clock Tally\nmodule rhythm Ruler\n\
    module seq Dovetail\nmodule mix Router\nmodule out Finish\n\
    cable clock.clock -> rhythm.clock\ncable clock.clock -> seq.clock\n\
    cable mix.mix_l -> out.l\ncable mix.mix_r -> out.r\n";

#[test]
fn auto_patched_modules_make_sound() {
    for kind in ["Mortise", "Veneer", "Chisel"] {
        let mut s = Session::open(Builtin, patch(BASE)).unwrap();
        s.apply(Command::AddModule {
            kind: kind.into(),
            id: Some("new".parse().unwrap()),
            part: None,
            auto_patch: true,
            near: None,
        })
        .unwrap();
        let r = render(s.patch(), 2.0);
        assert!(r.rms() > db_to_gain(-40.0), "{kind}: rms {}", r.rms());
    }
}

#[test]
fn an_auto_patched_bandsaw_makes_sound() {
    let mut s = Session::open(Builtin, patch(BASE)).unwrap();
    for kind in ["Mortise", "Bandsaw"] {
        s.apply(Command::AddModule {
            kind: kind.into(),
            id: None,
            part: None,
            auto_patch: true,
            near: None,
        })
        .unwrap();
    }
    let r = render(s.patch(), 2.0);
    assert!(r.rms() > db_to_gain(-40.0), "rms {}", r.rms());
}
