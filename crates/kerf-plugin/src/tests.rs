//! The plugin's logic, driven directly with no host: the rack renders blocks the way `process`
//! does, and the loader builds and queues engines.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use nih_plug::params::persist::PersistentField;

use crate::doc::{PatchCell, PatchDoc};
use crate::rack::{CHUNK, Controls, Host, Rack, Report};
use crate::shared::{Shared, controls_of};

/// A Jig and a Finish around a free-running oscillator: audible with nothing else patched.
const HUM: &str = "kerf 1\ntempo 100\n\
    module tally1 Tally\n\
    module jig Jig m1=0.2 m2=0.6 m3=0.1 m4=0.9\n\
    module hum Lathe tune=0.5\n\
    module out Finish volume=0.7\n\
    cable jig.m1 -> hum.turn\n\
    cable hum.out -> out.l\n\
    cable hum.out -> out.r\n";

const PLAYING: Host = Host {
    playing: true,
    tempo: None,
};

const CONTROLS: Controls = Controls {
    macros: [0.5; 4],
    level: 0.7,
    follow_tempo: true,
    follow_transport: true,
};

fn setup(sample_rate: f32, text: &str) -> (Arc<Shared>, Rack) {
    let (shared, mut rack) = Shared::new(Arc::new(PatchCell::default()));
    shared.set_sample_rate(sample_rate);
    rack.set_sample_rate(sample_rate);
    shared
        .load_text("test", text)
        .expect("the test patch opens");
    (shared, rack)
}

/// Renders `frames` in host blocks of `block`, each split into `CHUNK`s as `process` does.
fn render(
    rack: &mut Rack,
    frames: usize,
    block: usize,
    host: Host,
    controls: Controls,
) -> (Vec<f32>, Vec<f32>, Report) {
    let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
    let mut report = Report::default();
    let mut start = 0;
    while start < frames {
        let end = (start + block).min(frames);
        let mut at = start;
        while at < end {
            let to = (at + CHUNK).min(end);
            report.merge(rack.render(&mut left[at..to], &mut right[at..to], host, &controls));
            at = to;
        }
        start = end;
    }
    (left, right, report)
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, x| m.max(x.abs()))
}

#[test]
fn state_survives_a_json_round_trip() {
    // nih-plug stores a persisted field as JSON through the cell's `map` and `set`.
    let doc = PatchDoc {
        name: "Hum".to_owned(),
        text: HUM.to_owned(),
    };
    let cell = Arc::new(PatchCell::default());
    cell.store(doc.clone());
    let json = cell.map(|d| serde_json::to_string(d).expect("serialises"));
    let restored = Arc::new(PatchCell::default());
    PersistentField::set(&restored, serde_json::from_str(&json).expect("parses"));
    assert_eq!(restored.get(), doc);
    // Older or partial state still loads.
    let partial: PatchDoc = serde_json::from_str("{}").expect("parses");
    assert_eq!(partial, PatchDoc::default());
}

#[test]
fn restoring_state_raises_the_flag_once() {
    let cell = Arc::new(PatchCell::default());
    assert!(!cell.take_restored());
    PersistentField::set(&cell, PatchDoc::default());
    assert!(cell.take_restored());
    assert!(!cell.take_restored());
    // The plugin's own loads are not restores.
    cell.store(PatchDoc::default());
    assert!(!cell.take_restored());
}

#[test]
fn a_loaded_patch_is_saved_and_reloads_to_the_same_text() {
    let (shared, _rack) = setup(48_000.0, HUM);
    let doc = shared.cell().get();
    assert_eq!(doc.name, "test");
    assert!(doc.text.starts_with("kerf 1"));

    let cell = Arc::new(PatchCell::default());
    PersistentField::set(&cell, doc.clone());
    let (again, _rack) = Shared::new(cell);
    again.reload();
    assert_eq!(again.cell().get(), doc);
}

#[test]
fn empty_or_broken_state_falls_back_to_the_default_preset() {
    for text in ["", "this is not a patch"] {
        let cell = Arc::new(PatchCell::default());
        PersistentField::set(
            &cell,
            PatchDoc {
                name: "x".to_owned(),
                text: text.to_owned(),
            },
        );
        let (shared, _rack) = Shared::new(cell);
        shared.reload();
        let doc = shared.cell().get();
        assert!(doc.text.starts_with("kerf 1"), "{text:?} gave {doc:?}");
        assert_eq!(doc.name, crate::shared::DEFAULT_PRESET);
    }
}

#[test]
fn a_patch_that_does_not_open_changes_nothing() {
    let (shared, _rack) = setup(48_000.0, HUM);
    let before = shared.cell().get();
    assert!(shared.load_text("bad", "nonsense").is_err());
    assert_eq!(shared.cell().get(), before);
}

#[test]
fn audio_is_finite_and_sounds_at_every_rate_and_block_size() {
    for sample_rate in [22_050.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0] {
        for block in [1, 7, 32, 33, 64, 100, 512, 1000, 4096] {
            let (_shared, mut rack) = setup(sample_rate, HUM);
            let frames = (sample_rate / 4.0) as usize;
            let (left, right, report) = render(&mut rack, frames, block, PLAYING, CONTROLS);
            let all = left.iter().chain(&right);
            assert!(
                all.clone().all(|x| x.is_finite() && x.abs() <= 1.0),
                "{sample_rate} Hz, block {block}"
            );
            assert!(
                peak(&left) > 0.01 && peak(&right) > 0.01,
                "silent at {sample_rate} Hz, block {block}"
            );
            assert!(report.running && report.peak > 0.01);
        }
    }
}

#[test]
fn every_factory_preset_renders_finite_audio() {
    let (shared, mut rack) = Shared::new(Arc::new(PatchCell::default()));
    shared.set_sample_rate(48_000.0);
    rack.set_sample_rate(48_000.0);
    let presets = shared.presets();
    assert!(!presets.is_empty());
    for preset in presets {
        shared.load_preset(&preset.name).expect("preset opens");
        let (left, right, _) = render(&mut rack, 12_000, 256, PLAYING, CONTROLS);
        assert!(
            left.iter().chain(&right).all(|x| x.is_finite()),
            "{} is not finite",
            preset.name
        );
    }
}

#[test]
fn level_reaches_the_first_finish() {
    let (_shared, mut rack) = setup(48_000.0, HUM);
    let quiet = Controls {
        level: 0.0,
        ..CONTROLS
    };
    // The engine starts with the host's values taken as sent; the next change is applied.
    render(&mut rack, 64, 64, PLAYING, CONTROLS);
    let (left, ..) = render(&mut rack, 4800, 64, PLAYING, quiet);
    assert!(
        peak(&left[480..]) < 1e-4,
        "level 0 should silence the patch"
    );
    let (left, ..) = render(&mut rack, 4800, 64, PLAYING, CONTROLS);
    assert!(peak(&left) > 0.01, "level back up should sound again");
}

#[test]
fn a_new_patch_keeps_its_own_values_until_the_host_moves_a_control() {
    // The host's level (0.2) is not the patch's (0.7): installing must not apply it.
    let low = Controls {
        level: 0.2,
        ..CONTROLS
    };
    let (_shared, mut rack) = setup(48_000.0, HUM);
    let (kept, ..) = render(&mut rack, 4800, 64, PLAYING, low);
    let (_shared, mut rack) = setup(48_000.0, HUM);
    let (patch_level, ..) = render(&mut rack, 4800, 64, PLAYING, CONTROLS);
    assert_eq!(kept, patch_level);
    // Moving the control then takes effect.
    let (moved, ..) = render(&mut rack, 4800, 64, PLAYING, low);
    assert!(peak(&moved[480..]) < peak(&patch_level) * 0.5);
}

#[test]
fn a_load_that_cannot_be_queued_changes_nothing() {
    let (shared, mut rack) = setup(48_000.0, HUM);
    // The audio thread is not draining: fill the inbox (one engine is already in it).
    while shared.load_text("filler", HUM).is_ok() {}
    let (cell, patch) = (shared.cell().get(), shared.patch());
    let other = HUM.replace("tempo 100", "tempo 140");
    assert!(shared.load_text("other", &other).is_err());
    assert!(shared.publish().is_err());
    assert_eq!(shared.cell().get(), cell);
    assert_eq!(shared.patch(), patch);
    assert_eq!(
        f32::from_bits(shared.patch_tempo.load(Ordering::Relaxed)),
        100.0
    );
    // Once the audio thread catches up the same load succeeds.
    render(&mut rack, 64, 64, PLAYING, CONTROLS);
    shared.load_text("other", &other).expect("room again");
    assert_eq!(shared.patch().tempo, 140.0);
}

#[test]
fn the_editor_is_told_once_per_loaded_patch_to_take_its_values() {
    let (shared, _rack) = Shared::new(Arc::new(PatchCell::default()));
    assert!(!shared.take_unsynced());
    shared.load_text("a", HUM).unwrap();
    assert!(shared.take_unsynced());
    assert!(!shared.take_unsynced());
    shared.load_text("b", HUM).unwrap();
    assert!(shared.take_unsynced());
}

#[test]
fn the_cell_version_changes_with_the_name_and_the_text() {
    let cell = PatchCell::default();
    let v = cell.version();
    cell.set_name("x");
    assert_ne!(cell.version(), v);
    let v = cell.version();
    cell.store(PatchDoc::default());
    assert_ne!(cell.version(), v);
    assert_eq!(cell.name(), "");
}

#[test]
fn macros_find_the_first_jig_and_patches_without_one_ignore_them() {
    let (shared, _rack) = setup(48_000.0, HUM);
    let controls = shared.patch_controls();
    assert_eq!(controls.macros, Some([0.2, 0.6, 0.1, 0.9]));
    assert_eq!(controls.level, Some(0.7));

    let bare = kerf_core::text::parse("kerf 1\nmodule out Finish volume=0.4\n").unwrap();
    let controls = controls_of(&bare);
    assert_eq!(controls.macros, None);
    assert_eq!(controls.level, Some(0.4));

    // Macros on a patch with no Jig are accepted and change nothing.
    let (_shared, mut rack) = setup(48_000.0, "kerf 1\nmodule out Finish\n");
    let moved = Controls {
        macros: [0.9; 4],
        ..CONTROLS
    };
    let (left, ..) = render(&mut rack, 1000, 64, PLAYING, moved);
    assert!(left.iter().all(|x| x.is_finite()));
}

#[test]
fn a_macro_moves_the_sound() {
    // The oscillator's `turn` comes from Jig m1: render with it low and high.
    let sound = |m1: f32| {
        let (_shared, mut rack) = setup(48_000.0, HUM);
        let controls = Controls {
            macros: [m1, 0.5, 0.5, 0.5],
            ..CONTROLS
        };
        render(&mut rack, 64, 64, PLAYING, CONTROLS);
        render(&mut rack, 9600, 64, PLAYING, controls).0
    };
    assert_ne!(sound(0.0), sound(1.0));
}

#[test]
fn tempo_follows_the_host_within_range_and_falls_back_to_the_patch() {
    let tempo = |host: Option<f64>, follow: bool| {
        let (_shared, mut rack) = setup(48_000.0, HUM);
        let controls = Controls {
            follow_tempo: follow,
            ..CONTROLS
        };
        let host = Host {
            playing: true,
            tempo: host,
        };
        render(&mut rack, 64, 64, host, controls);
        rack.tempo().expect("an engine is running")
    };
    assert_eq!(tempo(None, true), 100.0, "no host tempo: the patch's own");
    assert_eq!(tempo(Some(140.0), true), 140.0);
    assert_eq!(tempo(Some(140.0), false), 100.0, "not following");
    assert_eq!(tempo(Some(5.0), true), 20.0);
    assert_eq!(tempo(Some(900.0), true), 300.0);
    assert_eq!(tempo(Some(f64::NAN), true), 100.0);
}

#[test]
fn the_patch_runs_with_the_transport_and_restarts_from_the_top() {
    let (shared, mut rack) = setup(48_000.0, HUM);
    let stopped = Host {
        playing: false,
        tempo: None,
    };
    let (left, _, report) = render(&mut rack, 4800, 256, stopped, CONTROLS);
    assert!(
        peak(&left) == 0.0 && !report.running,
        "silent while stopped"
    );

    let (left, _, report) = render(&mut rack, 4800, 256, PLAYING, CONTROLS);
    assert!(peak(&left) > 0.01 && report.running);

    // Stopping asks for a fresh engine; the background task builds it.
    let (_, _, report) = render(&mut rack, 4800, 256, stopped, CONTROLS);
    assert!(report.rearm && report.retired);
    shared.publish().expect("rebuilds");
    let (_, _, report) = render(&mut rack, 4800, 256, stopped, CONTROLS);
    assert!(!report.running);
    let (left, ..) = render(&mut rack, 4800, 256, PLAYING, CONTROLS);
    assert!(peak(&left) > 0.01, "plays again from a fresh engine");

    // With Follow transport off the patch runs whatever the host does.
    let free = Controls {
        follow_transport: false,
        ..CONTROLS
    };
    let (left, ..) = render(&mut rack, 4800, 256, stopped, free);
    assert!(peak(&left) > 0.01);
}

#[test]
fn an_engine_built_for_another_rate_is_dropped_and_rebuilt() {
    let (shared, mut rack) = setup(48_000.0, HUM);
    // The host switches to 44.1 kHz; the queued 48 kHz engine is out of date.
    shared.set_sample_rate(44_100.0);
    rack.set_sample_rate(44_100.0);
    let (left, _, report) = render(&mut rack, 512, 256, PLAYING, CONTROLS);
    assert!(report.rearm, "asks for an engine at the new rate");
    assert_eq!(peak(&left), 0.0);
    shared.publish().expect("builds at the new rate");
    let (left, ..) = render(&mut rack, 4410, 256, PLAYING, CONTROLS);
    assert!(peak(&left) > 0.01);
}

#[test]
fn initialising_again_before_any_audio_does_not_fill_the_inbox() {
    let (shared, mut rack) = setup(48_000.0, HUM);
    // A host that activates and deactivates the plugin without rendering: each `initialize`
    // resets the rack and reloads the patch.
    for round in 0..20 {
        rack.set_sample_rate(48_000.0);
        shared
            .load_text("again", HUM)
            .unwrap_or_else(|e| panic!("round {round}: {e}"));
    }
    let (left, ..) = render(&mut rack, 4800, 256, PLAYING, CONTROLS);
    assert!(peak(&left) > 0.01);
}

#[test]
fn loading_a_new_patch_while_playing_crossfades_without_a_gap() {
    let (shared, mut rack) = setup(48_000.0, HUM);
    render(&mut rack, 4800, 256, PLAYING, CONTROLS);
    shared.load_text("again", HUM).expect("opens");
    let (left, ..) = render(&mut rack, 4800, 256, PLAYING, CONTROLS);
    assert!(left.iter().all(|x| x.is_finite()));
    assert!(peak(&left) > 0.01);
}
