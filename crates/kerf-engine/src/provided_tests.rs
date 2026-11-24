//! Audio a host hands over with `samples::provide`: no file is read.

use std::f32::consts::TAU;

use kerf_core::text::parse;
use kerf_modules::Builtin;
use kerf_session::Session;

use crate::samples::{self, SampleError};
use crate::{Engine, diff};

const RATE: f32 = 48_000.0;

fn veneer_patch(name: &str) -> kerf_core::Patch {
    let text = format!(
        "kerf 1\ntempo 120\nmodule clock1 Tally\nmodule v1 Veneer slice=0 sample=\"{name}\"\nmodule out Finish\ncable clock1.clock -> v1.trig\ncable v1.l -> out.l\n"
    );
    Session::open(Builtin, parse(&text).unwrap())
        .unwrap()
        .patch()
        .clone()
}

fn rising_crossings(x: &[f32]) -> usize {
    x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
}

#[test]
fn provided_audio_plays_at_its_own_pitch_and_forgetting_it_warns() {
    let name = "web:provided-pitch.wav";
    let tone: Vec<f32> = (0..44_100)
        .map(|n| 0.5 * (TAU * 440.0 * n as f32 / 44_100.0).sin())
        .collect();
    samples::provide(name, &tone, 44_100).unwrap();

    let mut engine = Engine::new(&veneer_patch(name), RATE).unwrap();
    assert!(engine.warnings().is_empty(), "{:?}", engine.warnings());
    let left = engine.render(RATE as usize / 2).left;
    let onset = left.iter().position(|x| x.abs() > 0.01).expect("audible");
    let window = &left[onset + 200..onset + 200 + (0.04 * RATE) as usize];
    let hz = rising_crossings(window) as f32 / 0.04;
    assert!((hz - 440.0).abs() < 30.0, "{hz} Hz");

    samples::forget(name);
    let engine = Engine::new(&veneer_patch(name), RATE).unwrap();
    assert_eq!(engine.warnings().len(), 1);
    assert!(engine.warnings()[0].contains("demo"));
}

#[test]
fn providing_bad_audio_says_why() {
    assert_eq!(
        samples::provide("web:empty.wav", &[], 48_000),
        Err(SampleError::Empty)
    );
    assert_eq!(
        samples::prepare(&[0.1; 4], 48_000, 48_000),
        Err(SampleError::TooShort)
    );
}

#[test]
fn a_different_provided_name_is_a_rebuild() {
    assert_eq!(
        diff(&veneer_patch("web:a.wav"), &veneer_patch("web:b.wav")),
        None
    );
}
