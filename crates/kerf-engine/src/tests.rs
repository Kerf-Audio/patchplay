use kerf_core::text::parse;
use kerf_core::{Catalogue, Command, Patch};
use kerf_dsp::db_to_gain;
use kerf_modules::Builtin;
use kerf_session::Session;

use crate::{Change, Engine, EngineError, diff};

const RATE: f32 = 48_000.0;

fn open(text: &str) -> Session<Builtin> {
    Session::open(Builtin, parse(text).unwrap()).unwrap()
}

fn first_cut() -> Session<Builtin> {
    open(include_str!("../../../presets/first-cut.kerf"))
}

fn render(patch: &Patch, seconds: f32) -> crate::Render {
    Engine::new(patch, RATE)
        .unwrap()
        .render((seconds * RATE) as usize)
}

#[test]
fn presets_are_audible_and_stay_under_the_ceiling() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "kerf"))
        .collect();
    files.sort();
    assert!(files.len() >= 78, "only {} presets found", files.len());
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let text = text.as_str();
        let s = open(text);
        let engine = Engine::new(s.patch(), RATE).unwrap();
        assert!(engine.silent_kinds().is_empty());
        let r = render(s.patch(), 4.0);
        let ceiling = db_to_gain(-12.0 + 0.8 * 12.0);
        assert!(r.peak() > db_to_gain(-20.0), "peak {}", r.peak());
        assert!(
            r.peak() <= ceiling + 1e-4,
            "peak {} over {ceiling}",
            r.peak()
        );
        assert!(r.rms() > db_to_gain(-40.0), "rms {}", r.rms());
        assert!(r.left.iter().all(|x| x.is_finite()));
    }
}

#[test]
fn same_patch_renders_the_same_samples() {
    let s = first_cut();
    assert_eq!(render(s.patch(), 1.0), render(s.patch(), 1.0));
}

#[test]
fn muting_the_only_part_silences_the_output() {
    let mut s = first_cut();
    s.apply(Command::Mute {
        part: "bass".parse().unwrap(),
        muted: true,
    })
    .unwrap();
    assert_eq!(render(s.patch(), 1.0).peak(), 0.0);
}

#[test]
fn every_module_kind_has_dsp() {
    let mut s = Session::new(Builtin);
    for d in Builtin.all() {
        s.apply(Command::AddModule {
            kind: d.kind.into(),
            id: None,
            part: None,
            auto_patch: false,
            near: None,
        })
        .unwrap();
    }
    let engine = Engine::new(s.patch(), RATE).unwrap();
    assert_eq!(engine.silent_kinds(), [] as [&str; 0]);
}

fn set_param(s: &mut Session<Builtin>, module: &str, param: &str, value: f32) {
    s.apply(Command::SetParam {
        module: module.parse().unwrap(),
        param: param.parse().unwrap(),
        value,
    })
    .unwrap();
}

#[test]
fn blocks_render_the_same_samples_as_frames() {
    let s = first_cut();
    let r = render(s.patch(), 1.0);
    let mut engine = Engine::new(s.patch(), RATE).unwrap();
    let mut out = vec![0.0; 2 * RATE as usize];
    // Uneven block sizes, like a device that changes its buffer.
    let mut done = 0;
    for frames in [64, 480, 1, 1023, 256].into_iter().cycle() {
        let frames = frames.min(out.len() / 2 - done);
        if frames == 0 {
            break;
        }
        engine.process_block(&mut out[2 * done..2 * (done + frames)]);
        done += frames;
    }
    for (i, frame) in out.chunks_exact(2).enumerate() {
        assert_eq!(frame, [r.left[i], r.right[i]], "frame {i}");
    }
}

#[test]
fn set_param_matches_a_rebuilt_engine() {
    let mut s = first_cut();
    let mut live = Engine::new(s.patch(), RATE).unwrap();
    live.set_param("flt1", "cutoff", 0.9).unwrap();
    set_param(&mut s, "flt1", "cutoff", 0.9);
    let rebuilt = Engine::new(s.patch(), RATE).unwrap().render(24_000);
    let unchanged = Engine::new(first_cut().patch(), RATE)
        .unwrap()
        .render(24_000);
    assert_eq!(live.render(24_000), rebuilt);
    assert_ne!(rebuilt, unchanged);
}

#[test]
fn set_param_names_what_is_missing() {
    let mut engine = Engine::new(first_cut().patch(), RATE).unwrap();
    assert_eq!(
        engine.set_param("nope", "cutoff", 0.5),
        Err(EngineError::UnknownModule("nope".into()))
    );
    assert_eq!(
        engine.set_param("flt1", "nope", 0.5),
        Err(EngineError::UnknownParam {
            kind: "Plane",
            param: "nope".into()
        })
    );
}

#[test]
fn diff_separates_param_edits_from_rebuilds() {
    let old = first_cut();
    let mut new = first_cut();
    assert_eq!(diff(old.patch(), new.patch()), Some(vec![]));

    set_param(&mut new, "flt1", "cutoff", 0.9);
    new.apply(Command::SetTempo { bpm: 140.0 }).unwrap();
    let changes = diff(old.patch(), new.patch()).unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[1], Change::Tempo(140.0));

    let mut live = Engine::new(old.patch(), RATE).unwrap();
    for c in changes {
        live.apply(c);
    }
    assert_eq!(
        live.render(4000),
        Engine::new(new.patch(), RATE).unwrap().render(4000)
    );

    new.apply(Command::Disconnect {
        from: "amp1.out".parse().unwrap(),
        to: "mix.in1".parse().unwrap(),
    })
    .unwrap();
    assert_eq!(diff(old.patch(), new.patch()), None);
}

#[test]
fn rebuilt_engine_carries_module_state_over() {
    let before = first_cut();
    let mut after = first_cut();
    after
        .apply(Command::AddModule {
            kind: "Plane".into(),
            id: None,
            part: None,
            auto_patch: false,
            near: None,
        })
        .unwrap();
    assert_eq!(diff(before.patch(), after.patch()), None);

    let mut running = Engine::new(before.patch(), RATE).unwrap();
    running.render(30_000);
    let mut reference = Engine::new(before.patch(), RATE).unwrap();
    reference.render(30_000);
    let expected = reference.render(4000);

    let mut fresh = Engine::new(after.patch(), RATE).unwrap();
    fresh.take_state(&mut running);
    assert_eq!(fresh.render(4000), expected);

    let mut cold = Engine::new(after.patch(), RATE).unwrap();
    assert_ne!(cold.render(4000), expected);
}

const SNIPPET_HEADER: &str = "kerf 1\ntempo 120\nkey A minor\n\n";

fn snippet(id: &str) -> &'static kerf_session::Snippet {
    kerf_session::snippets()
        .iter()
        .find(|s| s.id == id)
        .unwrap()
}

/// A snare on every beat, for effects that need something to process.
const TEST_SOURCE: &str = "part testsrc \"Test source\" colour #888888
  module tsrc_rhythm Ruler hits_a=0.25 steps_a=1 hits_b=0 hits_c=0 hits_d=0
  module tsrc_drum Mortise engine=0.25 pitch=0.45 punch=0.55 decay=0.4 tone=0.5

cable clock.clock -> tsrc_rhythm.clock
cable tsrc_rhythm.a -> tsrc_drum.trig
";

fn assert_sane(text: &str, what: &str) -> crate::Render {
    let s = open(text);
    let r = render(s.patch(), 4.0);
    let ceiling = db_to_gain(-12.0 + 0.8 * 12.0) + 1e-4;
    assert!(r.left.iter().all(|x| x.is_finite()), "{what}: not finite");
    assert!(r.peak() > db_to_gain(-30.0), "{what}: peak {}", r.peak());
    assert!(
        r.peak() <= ceiling && r.peak() < 1.0,
        "{what}: peak {}",
        r.peak()
    );
    assert!(r.rms() > db_to_gain(-50.0), "{what}: rms {}", r.rms());
    r
}

#[test]
fn snippets_render_audibly_under_the_ceiling() {
    let start = &snippet("start/clock-out").code;
    for s in kerf_session::snippets() {
        match s.category {
            "start" => {
                open(&format!("{SNIPPET_HEADER}{}", s.code));
            }
            "effects" => {
                let mut feeds = String::new();
                for jack in &s.feed {
                    feeds.push_str(&format!("cable tsrc_drum.out -> {jack}\n"));
                }
                let wet = format!("{SNIPPET_HEADER}{start}\n{TEST_SOURCE}{feeds}\n{}", s.code);
                let dry = format!(
                    "{SNIPPET_HEADER}{start}\n{TEST_SOURCE}cable tsrc_drum.out -> out.l\ncable tsrc_drum.out -> out.r\n"
                );
                let wet = assert_sane(&wet, s.id);
                let dry = render(open(&dry).patch(), 4.0);
                assert_ne!(wet.left, dry.left, "{}: no change to the source", s.id);
            }
            _ => {
                assert_sane(&format!("{SNIPPET_HEADER}{start}\n{}", s.code), s.id);
            }
        }
    }
}

#[test]
fn drums_bass_lead_and_echo_paste_together() {
    let mut text = format!("{SNIPPET_HEADER}{}", snippet("start/clock-out").code);
    for id in ["drums/kit", "bass/acid", "lead/saw", "effects/tape-echo"] {
        text.push('\n');
        text.push_str(&snippet(id).code);
    }
    text.push_str("\ncable lead_amp.out -> echo_tape.in\n");
    assert!(kerf_core::text::diagnose(&Builtin, &text).is_empty());
    assert_sane(&text, "combined");
}

#[test]
fn the_tutorial_patch_renders() {
    let guide = kerf_session::writing_guide();
    let mut text = kerf_session::fenced_blocks(guide, "kerf complete").remove(0);
    for step in kerf_session::fenced_blocks(guide, "kerf step") {
        text.push('\n');
        text.push_str(&step);
    }
    assert_sane(&text, "tutorial");
}

#[test]
fn a_runaway_feedback_loop_cannot_silence_the_output_for_good() {
    // The mixer feeds itself at about twice its own level, so the loop overflows to
    // infinity within a few hundred samples. A limiter that has seen infinity would
    // stay shut, and a filter that has seen NaN would stay NaN.
    let s = open(&format!(
        "{SNIPPET_HEADER}module osc Chisel\nmodule mix Router level1=1\nmodule out Finish\n\n\
         cable osc.saw -> mix.in2\ncable mix.mix_l -> mix.in1\n\
         cable mix.mix_l -> out.l\ncable mix.mix_r -> out.r\n"
    ));
    let r = render(s.patch(), 2.0);
    assert!(r.left.iter().chain(&r.right).all(|x| x.is_finite()));
    assert!(r.peak() > 0.0);
}
