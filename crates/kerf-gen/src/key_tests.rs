//! Generated pitches and Fret quantisers follow the patch's key.

use kerf_core::{Module, ParamValue, Patch};
use kerf_dsp::hash;
use kerf_modules::{Io, instantiate};

use super::*;
use crate::tests::{NEW_STYLES, TUNED_STYLES, param};

/// Collects the pitch (volts, before Chisel's tune) at every gate rise of a
/// Dovetail driven by a clock, and returns semitone classes relative to the
/// key root once the oscillator's tune is added.
fn played_classes(p: &Patch) -> Vec<i32> {
    let seq = p
        .modules
        .iter()
        .find(|m| m.kind == "Dovetail")
        .expect("a sequencer");
    let seed = p.style.as_ref().unwrap().seed;
    let rate = 48_000.0;
    let mut dsp = instantiate("Dovetail", hash(seq.id.as_str()) ^ seed, rate).unwrap();
    // Glide only lags the pitch behind its target; the grid is what is tested.
    let values: Vec<f32> = seq
        .params
        .iter()
        .map(|v: &ParamValue| {
            if v.key.as_str() == "glide" {
                0.0
            } else {
                v.value
            }
        })
        .collect();
    let osc = p
        .modules
        .iter()
        .find(|m| m.kind == "Chisel" && m.part == seq.part)
        .unwrap();
    let tune = osc
        .params
        .iter()
        .find(|v| v.key.as_str() == "tune")
        .unwrap()
        .value;
    let offset = (tune - 0.5) * 4.0;
    let root = match p.key.as_deref().unwrap().split_whitespace().next().unwrap() {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        other => panic!("{other}"),
    };

    let mut classes = Vec::new();
    let (mut gate_was, mut last_pitch, mut inputs, mut outputs) =
        (false, 0.0f32, [0.0f32; 3], [0.0f32; 3]);
    let samples_per_step = (rate * 60.0 / 130.0 / 4.0) as usize;
    for n in 0..samples_per_step * 256 {
        let into_step = n % samples_per_step;
        inputs[0] = if into_step < 200 { 10.0 } else { 0.0 };
        let connected = [true, false, false];
        dsp.process(&mut Io {
            sample_rate: rate,
            tempo: 130.0,
            params: &values,
            inputs: &inputs,
            connected: &connected,
            outputs: &mut outputs,
        });
        let gate = outputs[1] > 1.0;
        if gate_was && !gate {
            // Glide is zeroed above, so the pitch sits on the scale grid.
            let semis = ((last_pitch + offset) * 12.0).round() as i32;
            classes.push((semis - root).rem_euclid(12));
        }
        gate_was = gate;
        last_pitch = outputs[0];
    }
    classes
}

#[test]
fn generated_notes_are_in_the_key() {
    for style in TUNED_STYLES {
        for seed in 1..=30 {
            let p = random(style, seed).unwrap();
            let classes = played_classes(&p);
            assert!(classes.len() > 8, "{style} {seed}: too few notes");
            for c in classes {
                assert!(
                    [0, 3, 5, 7, 10].contains(&c),
                    "{style} {seed} {:?}: class {c}",
                    p.key
                );
            }
        }
    }
}

#[test]
fn bass_pitch_follows_the_key() {
    let tune = |key: &str| {
        (1..200)
            .map(|s| random("foundry", s).unwrap())
            .find(|p| p.key.as_deref() == Some(key) && param(p, "bosc", "tune") > 0.2)
            .map(|p| param(&p, "bosc", "tune"))
            .unwrap()
    };
    // Same octave: A is nine semitones above C, and one semitone is 1/48.
    let semitones = (tune("A minor") - tune("C minor")) * 48.0;
    assert!((semitones - 9.0).abs() < 0.01, "{semitones}");
}

#[test]
fn fret_roots_and_scales_follow_the_key_through_evolve_and_breed() {
    for style in NEW_STYLES {
        for seed in 1..=15 {
            let p = random(style, seed).unwrap();
            let other = random(style, seed + 100).unwrap();
            let evolved = evolve(&p, 1.0, seed).unwrap();
            let child = breed(&p, &other, seed).unwrap();
            for patch in [&p, &evolved, &child] {
                check_frets(style, patch);
            }
        }
    }
}

/// Every Fret holds the key's root and a scale that sits inside its mode.
fn check_frets(style: &str, p: &Patch) {
    let key = p.key.as_deref().unwrap();
    let mode = key.split_whitespace().nth(1).unwrap();
    let scales: &[i32] = match mode {
        "minor" => &[1, 3],
        "dorian" => &[1, 4],
        "major" => &[0, 2],
        other => panic!("{style}: mode {other}"),
    };
    let root = util::key_root(key).unwrap();
    let frets: Vec<&Module> = p.modules.iter().filter(|m| m.kind == "Fret").collect();
    assert!(!frets.is_empty(), "{style}: no Fret");
    for m in frets {
        let get = |k: &str| m.params.iter().find(|v| v.key.as_str() == k).unwrap().value;
        assert_eq!(
            (get("root") * 12.0).round() as i32,
            root,
            "{style} {key} {}",
            m.id
        );
        let scale = (get("scale") * 7.0).round() as i32;
        assert!(
            scales.contains(&scale),
            "{style} {key} {}: scale {scale}",
            m.id
        );
    }
}
