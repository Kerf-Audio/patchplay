use std::f32::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};

use kerf_core::text::{parse, print};
use kerf_modules::Builtin;
use kerf_session::Session;

use crate::samples::{self, SampleError, load, peaks};
use crate::{Engine, diff};

const RATE: f32 = 48_000.0;

/// A scratch folder per test, removed at the end of the test.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("kerf-engine-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn sine(rate: u32, hz: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
    (0..(rate as f32 * seconds) as usize)
        .map(|n| amplitude * (TAU * hz * n as f32 / rate as f32).sin())
        .collect()
}

fn pcm16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

/// 16-bit PCM WAV from interleaved frames.
fn write_wav(path: &Path, rate: u32, channels: u16, interleaved: &[f32]) {
    let data = u32::try_from(interleaved.len() * 2).unwrap();
    let mut b = Vec::new();
    b.extend(b"RIFF");
    b.extend((36 + data).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(channels.to_le_bytes());
    b.extend(rate.to_le_bytes());
    b.extend((rate * u32::from(channels) * 2).to_le_bytes());
    b.extend((channels * 2).to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend(data.to_le_bytes());
    for &x in interleaved {
        b.extend(pcm16(x).to_le_bytes());
    }
    fs::write(path, b).unwrap();
}

/// 16-bit PCM AIFF, mono.
fn write_aiff(path: &Path, rate: u32, samples: &[f32]) {
    let data = u32::try_from(samples.len() * 2).unwrap();
    let exponent = 31 - rate.leading_zeros();
    let mut b = Vec::new();
    b.extend(b"FORM");
    b.extend((4 + 26 + 16 + data).to_be_bytes());
    b.extend(b"AIFFCOMM");
    b.extend(18u32.to_be_bytes());
    b.extend(1u16.to_be_bytes());
    b.extend(u32::try_from(samples.len()).unwrap().to_be_bytes());
    b.extend(16u16.to_be_bytes());
    b.extend(((16383 + exponent) as u16).to_be_bytes());
    b.extend((u64::from(rate) << (63 - exponent)).to_be_bytes());
    b.extend(b"SSND");
    b.extend((8 + data).to_be_bytes());
    b.extend(0u32.to_be_bytes());
    b.extend(0u32.to_be_bytes());
    for &x in samples {
        b.extend(pcm16(x).to_be_bytes());
    }
    fs::write(path, b).unwrap();
}

fn crossings(x: &[f32]) -> usize {
    x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count()
}

#[test]
fn wav_is_downmixed_resampled_and_normalised() {
    let dir = Dir::new("wav");
    let left = sine(44_100, 440.0, 1.0, 0.3);
    let stereo: Vec<f32> = left.iter().flat_map(|&x| [x, 0.0]).collect();
    let path = dir.file("a.wav");
    write_wav(&path, 44_100, 2, &stereo);

    let s = load(&path, RATE).unwrap();
    assert!(s.len().abs_diff(48_000) <= 1, "{} samples", s.len());
    let peak = s.iter().fold(0.0_f32, |p, x| p.max(x.abs()));
    assert!((peak - 0.9).abs() < 0.02, "peak {peak}");
    // 440 Hz stays 440 Hz: one rising crossing per cycle.
    assert!(crossings(&s).abs_diff(440) <= 1, "{}", crossings(&s));

    let cancel: Vec<f32> = left.iter().flat_map(|&x| [x, -x]).collect();
    write_wav(&dir.file("b.wav"), 44_100, 2, &cancel);
    let silent = load(&dir.file("b.wav"), RATE).unwrap();
    assert!(
        silent.iter().all(|x| x.abs() < 1e-4),
        "channels average to nothing"
    );
}

#[test]
fn downsampling_keeps_pitch_and_drops_what_cannot_be_kept() {
    let dir = Dir::new("down");
    // 1 kHz survives 48k -> 16k; the louder 12 kHz is above the new Nyquist
    // (8 kHz) and must not alias back in: a clean sine has rms = peak / sqrt 2.
    let mix: Vec<f32> = sine(48_000, 1_000.0, 1.0, 0.1)
        .iter()
        .zip(sine(48_000, 12_000.0, 1.0, 0.4))
        .map(|(a, b)| a + b)
        .collect();
    write_wav(&dir.file("mix.wav"), 48_000, 1, &mix);
    let s = load(&dir.file("mix.wav"), 16_000.0).unwrap();
    assert!(s.len().abs_diff(16_000) <= 1);
    let body = &s[200..s.len() - 200];
    let peak = body.iter().fold(0.0_f32, |p, x| p.max(x.abs()));
    let rms = (body.iter().map(|x| x * x).sum::<f32>() / body.len() as f32).sqrt();
    assert!(rms / peak > 0.68, "rms {rms}, peak {peak}");
}

#[test]
fn aiff_decodes_too() {
    let dir = Dir::new("aiff");
    write_aiff(&dir.file("a.aiff"), 22_050, &sine(22_050, 441.0, 0.5, 0.5));
    let s = load(&dir.file("a.aiff"), RATE).unwrap();
    assert!(s.len().abs_diff(24_000) <= 1, "{} samples", s.len());
    assert!(crossings(&s).abs_diff(220) <= 2, "{}", crossings(&s));
}

#[test]
fn long_files_are_capped_and_bad_files_say_why() {
    let dir = Dir::new("errors");
    write_wav(
        &dir.file("long.wav"),
        8_000,
        1,
        &sine(8_000, 100.0, 70.0, 0.5),
    );
    let long = load(&dir.file("long.wav"), 8_000.0).unwrap();
    assert_eq!(long.len(), 60 * 8_000);

    assert_eq!(
        load(&dir.file("gone.wav"), RATE),
        Err(SampleError::NotFound)
    );
    fs::write(
        dir.file("notes.wav"),
        "this is not audio at all, not even close",
    )
    .unwrap();
    assert!(matches!(
        load(&dir.file("notes.wav"), RATE),
        Err(SampleError::Unsupported(_))
    ));
    write_wav(&dir.file("tiny.wav"), 48_000, 1, &[0.1; 8]);
    assert_eq!(
        load(&dir.file("tiny.wav"), RATE),
        Err(SampleError::TooShort)
    );
}

#[test]
fn peaks_cover_the_buffer_in_min_max_pairs() {
    let x: Vec<f32> = (0..1000)
        .map(|n| if n == 600 { -0.8 } else { 0.1 })
        .collect();
    let p = peaks(&x, 10);
    assert_eq!(p.len(), 10);
    assert_eq!(p[0], [0.1, 0.1]);
    assert_eq!(p[6], [-0.8, 0.1]);
    assert_eq!(peaks(&x[..3], 256).len(), 3);
    assert!(peaks(&[], 256).is_empty());
}

#[test]
fn relative_paths_resolve_against_the_patch_folder() {
    let mut p = parse(
        "kerf 1\nmodule a Veneer sample=\"kits/one.wav\"\nmodule b Veneer sample=\"/abs/two.wav\"\n",
    )
    .unwrap();
    samples::resolve_paths(&mut p, Path::new("/songs"));
    assert_eq!(p.modules[0].sample.as_deref(), Some("/songs/kits/one.wav"));
    assert_eq!(p.modules[1].sample.as_deref(), Some("/abs/two.wav"));
}

/// A Veneer on every clock pulse (120 bpm), into the speakers.
fn veneer_patch(sample: &Path) -> kerf_core::Patch {
    let text = format!(
        "kerf 1\ntempo 120\nmodule clock1 Tally\nmodule v1 Veneer slice=0 sample=\"{}\"\nmodule out Finish\ncable clock1.clock -> v1.trig\ncable v1.l -> out.l\n",
        sample.display()
    );
    let s = Session::open(Builtin, parse(&text).unwrap()).unwrap();
    assert_eq!(parse(&print(s.patch())).unwrap(), *s.patch());
    s.patch().clone()
}

/// Rising zero crossings in the 40 ms after the sound starts, as Hz.
fn pitch(engine: &mut Engine) -> f32 {
    let left = engine.render(RATE as usize / 2).left;
    let onset = left.iter().position(|x| x.abs() > 0.01).expect("audible");
    let window = &left[onset + 200..onset + 200 + (0.04 * RATE) as usize];
    crossings(window) as f32 / 0.04
}

#[test]
fn a_loaded_sample_plays_at_its_own_pitch() {
    let dir = Dir::new("pitch");
    write_wav(
        &dir.file("a.wav"),
        44_100,
        1,
        &sine(44_100, 440.0, 1.0, 0.5),
    );
    write_wav(
        &dir.file("b.wav"),
        48_000,
        1,
        &sine(48_000, 880.0, 1.0, 0.5),
    );

    let mut engine = Engine::new(&veneer_patch(&dir.file("a.wav")), RATE).unwrap();
    assert!(engine.warnings().is_empty());
    let hz = pitch(&mut engine);
    assert!((hz - 440.0).abs() < 30.0, "{hz} Hz");

    let mut b = Engine::new(&veneer_patch(&dir.file("b.wav")), RATE).unwrap();
    assert!((pitch(&mut b) - 880.0).abs() < 30.0);
}

#[test]
fn a_splinter_grains_a_loaded_sample_at_its_own_pitch() {
    let dir = Dir::new("grains");
    write_wav(
        &dir.file("a.wav"),
        44_100,
        1,
        &sine(44_100, 440.0, 1.0, 0.5),
    );
    let text = format!(
        "kerf 1\nmodule s1 Splinter spray=0 size=0.65 density=0 shape=0.5 spread=0 sample=\"{}\"\nmodule out Finish\ncable s1.l -> out.l\n",
        dir.file("a.wav").display()
    );
    let session = Session::open(Builtin, parse(&text).unwrap()).unwrap();
    let mut engine = Engine::new(session.patch(), RATE).unwrap();
    assert!(engine.warnings().is_empty());
    let hz = pitch(&mut engine);
    assert!((hz - 440.0).abs() < 30.0, "{hz} Hz");
}

#[test]
fn a_changed_sample_is_a_rebuild_that_does_not_keep_the_old_audio() {
    let dir = Dir::new("swap");
    write_wav(
        &dir.file("a.wav"),
        48_000,
        1,
        &sine(48_000, 440.0, 1.0, 0.5),
    );
    write_wav(
        &dir.file("b.wav"),
        48_000,
        1,
        &sine(48_000, 880.0, 1.0, 0.5),
    );
    let (a, b) = (
        veneer_patch(&dir.file("a.wav")),
        veneer_patch(&dir.file("b.wav")),
    );
    assert_eq!(diff(&a, &a), Some(vec![]));
    assert_eq!(diff(&a, &b), None, "sample files differ");

    let mut old = Engine::new(&a, RATE).unwrap();
    old.render(100);
    let mut new = Engine::new(&b, RATE).unwrap();
    new.take_state(&mut old);
    assert!((pitch(&mut new) - 880.0).abs() < 30.0);
}

#[test]
fn a_missing_file_falls_back_to_the_demo_sample_with_a_warning() {
    let dir = Dir::new("missing");
    let mut engine = Engine::new(&veneer_patch(&dir.file("nowhere.wav")), RATE).unwrap();
    assert_eq!(engine.warnings().len(), 1);
    let w = &engine.warnings()[0];
    assert!(
        w.contains("v1") && w.contains("nowhere.wav") && w.contains("not found"),
        "{w}"
    );
    assert!(w.contains("demo"), "{w}");
    let r = engine.render(RATE as usize / 2);
    assert!(r.peak() > 0.05, "the demo sample still plays");
}
