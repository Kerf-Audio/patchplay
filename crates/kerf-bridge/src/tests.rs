use std::sync::{Arc, Mutex};

use kerf_core::{Command, Patch};
use serde_json::{Value, json};

use crate::{Bridge, BridgeError, Meter, NoAudio, Scope, State, Transport, dispatch};

fn bridge() -> Bridge {
    Bridge::new(Box::new(NoAudio("no sound card".into())))
}

fn open_preset(b: &Bridge, name: &str) -> State {
    b.open_preset(name).unwrap()
}

fn call(b: &Bridge, name: &str, args: Value) -> Value {
    dispatch(b, name, args)
        .expect("a known command")
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn param(state: &State, module: &str, key: &str) -> f32 {
    state
        .patch
        .module(&module.parse().unwrap())
        .unwrap()
        .param(&key.parse().unwrap())
        .unwrap()
        .value
}

/// Records what the audio side was told.
#[derive(Clone, Default)]
struct Spy(Arc<Mutex<Vec<&'static str>>>);

impl Transport for Spy {
    fn play(&mut self, _patch: &Patch) -> Result<(), String> {
        self.0.lock().unwrap().push("play");
        Ok(())
    }
    fn update(&mut self, _patch: &Patch) {
        self.0.lock().unwrap().push("update");
    }
    fn stop(&mut self) {
        self.0.lock().unwrap().push("stop");
    }
    fn is_playing(&self) -> bool {
        self.0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|&&e| e != "update")
            == Some(&"play")
    }
    fn meter(&self) -> Meter {
        Meter::default()
    }
    fn scope(&self) -> Scope {
        Scope {
            rate: 48_000,
            samples: vec![0.5, -0.25],
        }
    }
}

#[test]
fn set_param_through_the_bridge_reads_back_and_undoes() {
    let b = bridge();
    open_preset(&b, "steel-shop");
    let before = param(&b.get_state(), "bflt", "cutoff");

    let cmd: Command = serde_json::from_value(
        json!({"op":"set_param","module":"bflt","param":"cutoff","value":0.9}),
    )
    .unwrap();
    let state = b.apply(cmd).unwrap();
    assert_eq!(param(&state, "bflt", "cutoff"), 0.9);
    assert!(state.can_undo);

    let state = b.apply(Command::Undo).unwrap();
    assert_eq!(param(&state, "bflt", "cutoff"), before);
    assert!(state.can_redo);
}

#[test]
fn a_rejected_command_changes_nothing() {
    let b = bridge();
    open_preset(&b, "steel-shop");
    let before = b.get_state().text;
    let err = b
        .apply(Command::SetParam {
            module: "nothing".parse().unwrap(),
            param: "cutoff".parse().unwrap(),
            value: 0.5,
        })
        .unwrap_err();
    assert!(matches!(err, BridgeError::Session(_)), "{err}");
    let after = b.get_state();
    assert_eq!(after.text, before);
    assert!(!after.can_undo);
}

#[test]
fn describe_lists_every_module_with_tooltips() {
    let d = serde_json::to_value(bridge().describe()).unwrap();
    let all = kerf_core::Catalogue::all(&kerf_modules::Builtin).len();
    assert_eq!(d["modules"].as_array().unwrap().len(), all);
    assert_eq!(d["bands"].as_array().unwrap().len(), 6);
    let tally = &d["modules"][0];
    assert!(!tally["params"][0]["tooltip"].as_str().unwrap().is_empty());
    assert!(!tally["outputs"][0]["tooltip"].as_str().unwrap().is_empty());
}

#[test]
fn every_embedded_preset_is_listed_and_opens_unsaved() {
    let b = bridge();
    let found = b.list_presets();
    let names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
    assert!(
        names.contains(&"steel-shop") && names.contains(&"first-cut"),
        "{names:?}"
    );
    assert!(names.windows(2).all(|w| w[0] < w[1]), "sorted by name");
    for p in &found {
        assert_eq!(p.path, format!("preset:{}", p.name));
        let state = b.open_preset(&p.name).unwrap();
        assert!(!state.patch.modules.is_empty(), "{}", p.name);
        assert_eq!(
            state.path, None,
            "a factory preset opens as an unsaved session"
        );
    }
    assert!(b.open_preset("no-such-preset").is_err());
}

#[test]
fn open_text_remembers_where_the_text_came_from() {
    let b = bridge();
    let text = "kerf 1\ntempo 101\nmodule a Tally\n";
    let state = b
        .open_text("song.kerf", text, Some("song.kerf".into()))
        .unwrap();
    assert_eq!(state.path.as_deref(), Some("song.kerf"));
    assert_eq!(b.text(), state.text);
    assert_eq!(
        b.open_text("other.kerf", &b.text(), None).unwrap().patch,
        state.patch
    );
    assert_eq!(
        b.set_path(Some("saved.kerf".into())).path.as_deref(),
        Some("saved.kerf")
    );

    let err = b
        .open_text("bad.kerf", "this is not kerf\n", None)
        .unwrap_err();
    assert!(err.to_string().contains("bad.kerf"), "{err}");
    assert_eq!(
        b.path().as_deref(),
        Some("saved.kerf"),
        "a bad file changes nothing"
    );
}

#[test]
fn render_gives_an_audible_wav() {
    let b = bridge();
    open_preset(&b, "steel-shop");
    let (bytes, r) = b.render_wav_bytes(2.0).unwrap();
    assert!(r.peak_db > -40.0 && r.peak_db < 0.5, "peak {}", r.peak_db);
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(bytes.len(), 44 + 2 * 48_000 * 4);
    assert!(b.render_wav_bytes(9999.0).is_err());
}

#[test]
fn provided_audio_feeds_sample_peaks_and_the_render() {
    let name = "web:bridge-loop.wav";
    let tone: Vec<f32> = (0..48_000)
        .map(|n| 0.3 * (std::f32::consts::TAU * 440.0 * n as f32 / 48_000.0).sin())
        .collect();
    kerf_engine::samples::provide(name, &tone, 48_000).unwrap();
    let b = bridge();
    let text = format!(
        "kerf 1\nmodule v1 Veneer sample=\"{name}\"\nmodule out Finish\ncable v1.l -> out.l\n"
    );
    b.open_text("song.kerf", &text, None).unwrap();

    let p = b.sample_peaks("v1").unwrap();
    assert_eq!(p.peaks.len(), 256);
    assert_eq!(
        (p.slices, p.file.as_deref(), p.warning.as_deref()),
        (16, Some("bridge-loop.wav"), None)
    );
    assert!((p.seconds - 1.0).abs() < 0.001);
    let top = p.peaks.iter().fold(0.0_f32, |m, [_, hi]| m.max(*hi));
    assert!((top - 0.9).abs() < 0.02, "normalised to {top}");

    // Audio nobody provided: the demo sample, and a reason.
    let set = |path: Option<&str>| Command::SetSample {
        module: "v1".parse().unwrap(),
        path: path.map(str::to_owned),
    };
    b.apply(set(Some("web:gone.wav"))).unwrap();
    let p = b.sample_peaks("v1").unwrap();
    assert!(p.file.is_none() && p.warning.as_deref().unwrap().contains("not found"));
    assert!(
        (p.seconds - 4.0).abs() < 0.001,
        "the demo is 16 quarter-second slices"
    );
    assert_eq!(b.render_wav_bytes(0.1).unwrap().1.warnings.len(), 1);

    b.apply(set(None)).unwrap();
    assert!(b.sample_peaks("v1").unwrap().warning.is_none());
    assert!(b.sample_peaks("out").is_err(), "a Finish has no sample");
    assert!(b.sample_peaks("nobody").is_err());
    assert_eq!(
        call(&b, "sample_peaks", json!({"module": "v1"}))["slices"],
        16
    );
}

#[test]
fn the_demo_sample_has_peaks_without_a_module() {
    let p = Bridge::demo_sample_peaks();
    assert_eq!((p.peaks.len(), p.slices), (256, 16));
    assert!(p.file.is_none() && p.warning.is_none());
    assert!((p.seconds - 4.0).abs() < 0.001);
    assert!(p.peaks.iter().any(|[lo, hi]| *hi > 0.1 && *lo < -0.1));
    let json = call(&bridge(), "demo_sample_peaks", json!({}));
    assert_eq!(json["slices"], 16);
    assert_eq!(json["peaks"].as_array().map(Vec::len), Some(256));
    // The same waveform a Veneer without a file gets.
    let b = bridge();
    b.open_text(
        "v.kerf",
        "kerf 1\nmodule v1 Veneer\nmodule out Finish\ncable v1.l -> out.l\n",
        None,
    )
    .unwrap();
    assert_eq!(b.sample_peaks("v1").unwrap().peaks, p.peaks);
}

#[test]
fn a_splinter_gets_its_sample_waveform_and_plays_it() {
    let name = "web:bridge-grains.wav";
    let tone: Vec<f32> = (0..24_000)
        .map(|n| 0.5 * (std::f32::consts::TAU * 220.0 * n as f32 / 48_000.0).sin())
        .collect();
    kerf_engine::samples::provide(name, &tone, 48_000).unwrap();
    let b = bridge();
    let text = format!(
        "kerf 1\nmodule s1 Splinter sample=\"{name}\"\nmodule out Finish\ncable s1.l -> out.l\n"
    );
    b.open_text("grains.kerf", &text, None).unwrap();
    let p = b.sample_peaks("s1").unwrap();
    assert_eq!(
        (p.file.as_deref(), p.warning.as_deref()),
        (Some("bridge-grains.wav"), None)
    );
    assert!((p.seconds - 0.5).abs() < 0.001);
    let summary = b.render_wav_bytes(1.0).unwrap().1;
    assert!(summary.warnings.is_empty() && summary.peak_db > -30.0);
}

#[test]
fn playing_needs_an_audio_output() {
    let err = bridge().transport_play().unwrap_err();
    assert!(matches!(err, BridgeError::Audio(_)));
}

#[test]
fn edits_reach_the_audio_side_while_playing_and_previews_leave_undo_alone() {
    let spy = Spy::default();
    let b = Bridge::new(Box::new(spy.clone()));
    open_preset(&b, "steel-shop");
    assert!(
        spy.0.lock().unwrap().is_empty(),
        "nothing is told before play"
    );

    assert!(b.transport_play().unwrap().playing);
    b.apply(Command::SetTempo { bpm: 130.0 }).unwrap();
    let preview = b
        .preview(
            serde_json::from_value(
                json!({"op":"set_param","module":"bflt","param":"cutoff","value":0.7}),
            )
            .unwrap(),
        )
        .unwrap();
    let heard = preview.module(&"bflt".parse().unwrap()).unwrap();
    assert_eq!(heard.param(&"cutoff".parse().unwrap()).unwrap().value, 0.7);
    assert_eq!(*spy.0.lock().unwrap(), ["play", "update", "update"]);
    // The preview is not in the session: one command, one undo step.
    assert_ne!(param(&b.get_state(), "bflt", "cutoff"), 0.7);
    assert!(b.get_state().can_undo);
    b.apply(Command::Undo).unwrap();
    assert!(!b.get_state().can_undo);
    assert!(!b.transport_stop().playing);
}

#[test]
fn dispatch_takes_the_same_json_as_the_tauri_commands() {
    let b = bridge();
    open_preset(&b, "steel-shop");
    let state = call(
        &b,
        "apply",
        json!({"cmd": {"op":"mute","part":"drums","muted":true}}),
    );
    let drums = state["patch"]["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "drums")
        .unwrap();
    assert_eq!(drums["muted"], true);
    assert!(
        dispatch(&b, "apply", json!({"cmd": {"op": "nope"}}))
            .unwrap()
            .is_err()
    );
    assert!(dispatch(&b, "nope", json!({})).is_none());
}

#[test]
fn scope_answers_the_rate_and_samples_and_is_empty_without_audio() {
    let b = Bridge::new(Box::new(Spy::default()));
    assert_eq!(
        call(&b, "scope", json!({})),
        json!({"rate": 48_000, "samples": [0.5, -0.25]})
    );
    assert_eq!(
        call(&bridge(), "scope", json!({})),
        json!({"rate": 0, "samples": []})
    );
}

#[test]
fn check_text_reports_line_and_column() {
    let b = bridge();
    let good = call(
        &b,
        "check_text",
        json!({ "text": "kerf 1\nmodule a Tally\n" }),
    );
    assert_eq!(good["ok"], true);
    assert_eq!(good["diagnostics"], json!([]));

    let bad = call(
        &b,
        "check_text",
        json!({ "text": "kerf 1\nmodule a Foo\n" }),
    );
    assert_eq!(bad["ok"], false);
    let d = &bad["diagnostics"][0];
    assert_eq!(
        (d["line"].as_u64(), d["severity"].as_str()),
        (Some(2), Some("error"))
    );
    assert!(d["col"].as_u64().unwrap() < d["end_col"].as_u64().unwrap());
    assert!(d["message"].as_str().unwrap().contains("Foo"));
}

#[test]
fn completions_and_docs_come_from_the_descriptors() {
    let b = bridge();
    let c = call(&b, "completions", json!({}));
    assert!(
        c["keywords"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["word"] == "cable")
    );
    let kinds = c["kinds"].as_array().unwrap();
    let mortise = kinds.iter().find(|k| k["kind"] == "Mortise").unwrap();
    assert!(
        mortise["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|j| j["key"] == "trig")
    );
    assert!(!c["styles"].as_array().unwrap().is_empty());

    let docs = call(&b, "language_docs", json!({}));
    let sections = docs["sections"].as_array().unwrap();
    assert!(sections.iter().any(|s| s["id"] == "cables"));
    assert!(
        sections
            .iter()
            .all(|s| !s["markdown"].as_str().unwrap().is_empty())
    );
}

#[test]
fn apply_reports_auto_patched_cables_and_replace_text_undoes() {
    let b = bridge();
    call(
        &b,
        "apply",
        json!({ "cmd": { "op": "replace_text", "text": "kerf 1\nmodule clock Tally\n" } }),
    );
    let suggested = call(&b, "suggest_cables", json!({ "kind": "Mortise" }));
    assert!(!suggested["cables"].as_array().unwrap().is_empty());

    let state = call(
        &b,
        "apply",
        json!({ "cmd": { "op": "add_module", "kind": "Mortise", "id": "kick", "auto_patch": true } }),
    );
    let connected = state["connected"].as_array().unwrap();
    assert!(connected.iter().any(|c| c["to"] == "kick.trig"));

    let undone = call(&b, "apply", json!({ "cmd": { "op": "undo" } }));
    assert_eq!(undone["patch"]["modules"].as_array().unwrap().len(), 1);
}
