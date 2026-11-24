//! Everything but the device: the control side feeding a voice directly.

use std::sync::Arc;

use kerf_core::text::parse;
use kerf_core::{Command, Patch};
use kerf_engine::Engine;
use kerf_modules::Builtin;
use kerf_session::Session;
use rtrb::{Consumer, RingBuffer};

use crate::AudioError;
use crate::control::Control;
use crate::voice::{Peaks, Voice};

const RATE: u32 = 48_000;
/// Frames per fake device callback.
const BLOCK: usize = 480;
/// `Voice` fades for 25 ms.
const FADE: usize = 1200;

struct Rig {
    control: Control,
    voice: Voice,
    retired: Consumer<Box<Engine>>,
    peaks: Arc<Peaks>,
}

impl Rig {
    fn new() -> Self {
        Self::with_queue(64)
    }

    fn with_queue(capacity: usize) -> Self {
        let (messages_tx, messages_rx) = RingBuffer::new(capacity);
        let (retired_tx, retired) = RingBuffer::new(8);
        let peaks = Arc::new(Peaks::default());
        Self {
            control: Control::new(messages_tx, RATE),
            voice: Voice::new(messages_rx, retired_tx, Arc::clone(&peaks), RATE),
            retired,
            peaks,
        }
    }

    /// Interleaved stereo for `frames` frames, in device-sized blocks.
    fn render(&mut self, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; 2 * frames];
        for block in out.chunks_mut(2 * BLOCK) {
            self.voice.render(block);
        }
        out
    }
}

fn first_cut() -> Session<Builtin> {
    let text = include_str!("../../../presets/first-cut.kerf");
    Session::open(Builtin, parse(text).unwrap()).unwrap()
}

fn steel_shop() -> Patch {
    let text = include_str!("../../../presets/steel-shop.kerf");
    Session::open(Builtin, parse(text).unwrap())
        .unwrap()
        .patch()
        .clone()
}

fn engine(patch: &Patch) -> Engine {
    Engine::new(patch, RATE as f32).unwrap()
}

fn frames(engine: &mut Engine, n: usize) -> Vec<f32> {
    let mut out = vec![0.0; 2 * n];
    engine.process_block(&mut out);
    out
}

#[test]
fn play_fades_in_then_matches_the_engine() {
    let patch = first_cut().patch().clone();
    let mut rig = Rig::new();
    rig.control.play(&patch).unwrap();
    let out = rig.render(24_000);
    let expected = frames(&mut engine(&patch), 24_000);

    assert_eq!(out[..2], [0.0, 0.0]);
    for i in 0..FADE {
        let t = i as f32 / FADE as f32;
        assert!((out[2 * i] - expected[2 * i] * t).abs() < 1e-6, "frame {i}");
    }
    assert_eq!(out[2 * FADE..], expected[2 * FADE..]);
}

#[test]
fn param_edits_reach_the_running_engine_without_a_swap() {
    let mut session = first_cut();
    let mut rig = Rig::new();
    rig.control.play(session.patch()).unwrap();
    let mut out = rig.render(24_000);

    session
        .apply(Command::SetParam {
            module: "flt1".parse().unwrap(),
            param: "cutoff".parse().unwrap(),
            value: 0.9,
        })
        .unwrap();
    rig.control.update(session.patch()).unwrap();
    out.extend(rig.render(4800));

    let mut reference = engine(first_cut().patch());
    let mut expected = frames(&mut reference, 24_000);
    reference.set_param("flt1", "cutoff", 0.9).unwrap();
    expected.extend(frames(&mut reference, 4800));
    assert_eq!(out[2 * FADE..], expected[2 * FADE..]);
    assert!(rig.retired.pop().is_err(), "no engine was swapped");
}

#[test]
fn structural_edits_keep_module_state() {
    let mut session = first_cut();
    let mut rig = Rig::new();
    rig.control.play(session.patch()).unwrap();
    let mut out = rig.render(24_000);

    session
        .apply(Command::AddModule {
            kind: "Plane".into(),
            id: None,
            part: None,
            auto_patch: false,
            near: None,
        })
        .unwrap();
    rig.control.update(session.patch()).unwrap();
    out.extend(rig.render(4800));

    let mut reference = engine(first_cut().patch());
    let expected = frames(&mut reference, 28_800);
    assert_eq!(out[2 * FADE..], expected[2 * FADE..], "no restart, no fade");
    assert!(rig.retired.pop().is_ok(), "the old engine came back");
}

#[test]
fn playing_another_patch_cross_fades() {
    let first = first_cut().patch().clone();
    let second = steel_shop();
    let mut rig = Rig::new();
    rig.control.play(&first).unwrap();
    rig.render(24_000);
    rig.control.play(&second).unwrap();
    let out = rig.render(4800);

    let expected = frames(&mut engine(&second), 4800);
    assert_eq!(out[2 * FADE..], expected[2 * FADE..]);
    assert_ne!(out[..2 * FADE], expected[..2 * FADE]);
    assert!(rig.retired.pop().is_ok(), "the faded-out engine came back");
}

#[test]
fn stop_fades_to_silence_and_updates_do_nothing_after() {
    let session = first_cut();
    let mut rig = Rig::new();
    rig.control.play(session.patch()).unwrap();
    rig.render(24_000);
    assert!(rig.control.is_playing());

    rig.control.stop().unwrap();
    assert!(!rig.control.is_playing());
    let out = rig.render(4800);
    assert!(out[..2 * FADE].iter().any(|&x| x != 0.0));
    assert!(out[2 * FADE..].iter().all(|&x| x == 0.0));
    assert!(rig.retired.pop().is_ok());

    rig.control.update(session.patch()).unwrap();
    assert!(rig.render(4800).iter().all(|&x| x == 0.0));
}

#[test]
fn the_meter_holds_peaks_until_read() {
    let mut rig = Rig::new();
    rig.control.play(first_cut().patch()).unwrap();
    let out = rig.render(24_000);
    let peak = out.iter().fold(0.0f32, |p, x| p.max(x.abs()));

    let (left, right) = rig.peaks.take();
    assert_eq!(left.max(right), peak);
    assert!(peak > 0.05);
    assert_eq!(rig.peaks.take(), (0.0, 0.0));
}

#[test]
fn a_full_queue_is_busy_not_a_panic() {
    let patch = first_cut().patch().clone();
    let mut rig = Rig::with_queue(1);
    rig.control.play(&patch).unwrap();
    assert!(matches!(rig.control.play(&patch), Err(AudioError::Busy)));
    rig.render(BLOCK);
    rig.control.play(&patch).unwrap();
}

/// The mono mix of interleaved stereo, as the tap records it.
fn mono(stereo: &[f32]) -> Vec<f32> {
    stereo
        .chunks_exact(2)
        .map(|f| 0.5 * (f[0] + f[1]))
        .collect()
}

#[test]
fn the_scope_tap_returns_the_latest_output_in_order() {
    let mut rig = Rig::new();
    rig.control.play(first_cut().patch()).unwrap();
    // 10 000 frames in 480-frame blocks never ends on a ring boundary.
    let out = mono(&rig.render(10_000));
    let mut tap = vec![0.0; crate::SCOPE_SAMPLES];
    assert_eq!(rig.peaks.tap.read_latest(&mut tap), crate::SCOPE_SAMPLES);
    assert_eq!(tap, out[out.len() - crate::SCOPE_SAMPLES..]);
    assert!(
        tap.iter().any(|&x| x.abs() > 0.01),
        "the scope should hear the patch"
    );
}

#[test]
fn the_scope_tap_hears_the_fade_and_then_silence() {
    let mut rig = Rig::new();
    rig.control.play(first_cut().patch()).unwrap();
    rig.render(4800);
    rig.control.stop().unwrap();
    let out = mono(&rig.render(FADE + 3000));
    let mut tap = vec![1.0; 1000];
    assert_eq!(rig.peaks.tap.read_latest(&mut tap), 1000);
    assert_eq!(tap, out[out.len() - 1000..]);
    assert!(tap.iter().all(|&x| x == 0.0));
}

#[test]
fn rendering_and_reading_the_scope_allocate_nothing() {
    let mut rig = Rig::new();
    rig.control.play(first_cut().patch()).unwrap();
    let mut block = [0.0f32; 2 * BLOCK];
    let mut tap = [0.0f32; crate::SCOPE_SAMPLES];
    // The first blocks take the message and may set things up; the guard covers the steady state.
    for _ in 0..50 {
        rig.voice.render(&mut block);
    }
    let before = counting::allocations();
    for _ in 0..200 {
        rig.voice.render(&mut block);
        rig.peaks.tap.read_latest(&mut tap);
    }
    assert_eq!(counting::allocations() - before, 0);
}

#[test]
fn the_allocation_counter_sees_an_allocation() {
    let before = counting::allocations();
    let v = std::hint::black_box(vec![1u8; 64]);
    assert!(counting::allocations() > before);
    drop(v);
}

/// Counts this thread's heap allocations, so a test can show a stretch of code makes none.
#[allow(unsafe_code)]
mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static COUNT: Cell<usize> = const { Cell::new(0) };
    }

    struct Counting;

    // SAFETY: every call is passed straight to the system allocator; the counter only reads
    // and writes a plain thread-local `Cell`, which allocates nothing.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            COUNT.with(|c| c.set(c.get() + 1));
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            COUNT.with(|c| c.set(c.get() + 1));
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: Counting = Counting;

    pub fn allocations() -> usize {
        COUNT.with(Cell::get)
    }
}
