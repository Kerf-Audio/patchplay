use std::f32::consts::TAU;
use std::sync::Arc;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, CV_MAX, Edge, Gate, Rng};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Veneer",
    band: Band::Voices,
    hp: 12,
    summary: "Sampler and slicer",
    params: &[
        param("slice", "Slice", 0.2, "Which slice plays."),
        param("speed", "Speed", 0.5, "Playback speed and pitch."),
        param(
            "reverse",
            "Reverse",
            0.0,
            "Plays slices backwards above 0.5.",
        ),
        param("looped", "Loop", 0.0, "Loops the slice while triggered."),
    ],
    inputs: &[
        jack("trig", "Trig", Gate, "Plays the current slice."),
        jack("slice", "Slice", Cv, "Picks the slice from a voltage."),
        jack("voct", "V/Oct", Pitch, "Repitches playback."),
    ],
    outputs: &[
        jack("l", "L", Audio, "Left."),
        jack("r", "R", Audio, "Right."),
    ],
};

/// The sample is cut into this many equal slices.
pub const SLICES: usize = 16;
/// Fade at the edges of a slice, in seconds, so cuts do not click.
const FADE_SECONDS: f32 = 0.002;
/// Fade when a looped slice is released, in seconds.
const RELEASE_SECONDS: f32 = 0.005;

/// A built-in sample for when none is loaded: sixteen quarter-second
/// slices, alternating tuned pings that climb in pitch with noise bursts
/// that get shorter.
pub fn demo_sample(sample_rate: f32) -> Arc<[f32]> {
    let len = (0.25 * sample_rate) as usize;
    let mut rng = Rng::new(0x5A3B1E);
    let mut out = Vec::with_capacity(SLICES * len);
    for slice in 0..SLICES {
        let hz = 110.0 * (slice as f32 / 6.0).exp2();
        let fall = 10.0 + slice as f32 * 4.0;
        for n in 0..len {
            let t = n as f32 / sample_rate;
            let click = (-t / 0.002).exp() * (rng.unit() * 2.0 - 1.0);
            out.push(if slice % 2 == 0 {
                0.8 * (TAU * hz * t).sin() * (-t / 0.06).exp() + 0.2 * click
            } else {
                0.8 * (rng.unit() * 2.0 - 1.0) * (-t * fall).exp()
            });
        }
    }
    out.into()
}

pub struct State {
    /// Mono, at the engine's sample rate.
    sample: Arc<[f32]>,
    gate: Gate,
    playing: bool,
    /// First sample and length of the slice being played.
    start: f32,
    length: f32,
    reverse: bool,
    /// Source samples travelled through the slice.
    travelled: f32,
    /// Fades out when a looped slice is released.
    hold: f32,
}

impl State {
    pub fn new(sample: Arc<[f32]>) -> Self {
        Self {
            sample,
            gate: Gate::default(),
            playing: false,
            start: 0.0,
            length: 0.0,
            reverse: false,
            travelled: 0.0,
            hold: 1.0,
        }
    }

    /// Linear read at a fractional sample index.
    fn read(&self, at: f32) -> f32 {
        let i = at as usize;
        let f = at - i as f32;
        let last = self.sample.len() - 1;
        let a = self.sample[i.min(last)];
        let b = self.sample[(i + 1).min(last)];
        a + (b - a) * f
    }

    fn trigger(&mut self, slice: f32, reverse: bool) {
        let length = (self.sample.len() / SLICES) as f32;
        if length < 1.0 {
            return;
        }
        let index = ((slice * SLICES as f32) as usize).min(SLICES - 1);
        self.start = index as f32 * length;
        self.length = length;
        self.reverse = reverse;
        self.travelled = 0.0;
        self.hold = 1.0;
        self.playing = true;
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[slice, speed, reverse, looped] = io.params else {
            return;
        };
        let &[trig, slice_cv, voct] = io.inputs else {
            return;
        };
        let sr = io.sample_rate;
        let edge = self.gate.update(trig);
        if edge == Edge::Rose {
            let pick = (slice + slice_cv / CV_MAX).clamp(0.0, 1.0);
            self.trigger(pick, reverse > 0.5);
        }
        if !self.playing {
            io.outputs[..2].fill(0.0);
            return;
        }

        let fade = FADE_SECONDS * sr;
        let gain = (self.travelled / fade)
            .min((self.length - self.travelled) / fade)
            .min(self.hold)
            .clamp(0.0, 1.0);
        let at = if self.reverse {
            self.start + self.length - 1.0 - self.travelled
        } else {
            self.start + self.travelled
        };
        let y = self.read(at) * gain * AUDIO_VOLTS;
        io.outputs[..2].fill(y.clamp(-AUDIO_VOLTS, AUDIO_VOLTS));

        let looping = looped > 0.5;
        if looping && !self.gate.is_high() {
            self.hold -= 1.0 / (RELEASE_SECONDS * sr);
        }
        self.travelled += ((speed - 0.5) * 4.0 + voct).exp2();
        if self.travelled >= self.length {
            self.travelled -= self.length;
            self.playing = looping && self.hold > 0.0;
        }
        self.playing &= self.hold > 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::{SLICES, State, demo_sample};
    use crate::rig::{SR, peak, render, rms};

    const SLICE: usize = (0.25 * SR) as usize;

    /// One trigger at the start, held for `hold` samples.
    fn play(params: [f32; 4], slice_cv: f32, voct: f32, hold: usize, frames: usize) -> Vec<f32> {
        let mut veneer = State::new(demo_sample(SR));
        let out = render(
            &mut veneer,
            &params,
            &[true, true, true],
            2,
            frames,
            |i, v| {
                v[0] = if i < hold { 10.0 } else { 0.0 };
                v[1] = slice_cv;
                v[2] = voct;
            },
        );
        assert_eq!(out[0], out[1]);
        out.into_iter().next().unwrap()
    }

    /// Index just past the last sound above 1 mV.
    fn length(x: &[f32]) -> usize {
        x.iter().rposition(|v| v.abs() > 0.001).map_or(0, |i| i + 1)
    }

    #[test]
    fn plays_one_slice_then_stops() {
        let out = play([0.0, 0.5, 0.0, 0.0], 0.0, 0.0, 240, SLICE * 3);
        let p = peak(&out);
        assert!(p > 1.0 && p <= 5.0, "peak {p} V");
        assert!(
            length(&out) <= SLICE,
            "slice ran on for {} samples",
            length(&out)
        );
        assert_eq!(peak(&out[SLICE..]), 0.0);
    }

    #[test]
    fn speed_and_pitch_change_the_playing_time() {
        // Slice 0 is a ping. One octave up plays twice as fast, so ends
        // earlier; V/Oct does the same as Speed.
        let normal = length(&play([0.0, 0.5, 0.0, 0.0], 0.0, 0.0, 240, SLICE * 3));
        let fast = length(&play([0.0, 0.75, 0.0, 0.0], 0.0, 0.0, 240, SLICE * 3));
        let by_cv = length(&play([0.0, 0.5, 0.0, 0.0], 0.0, 1.0, 240, SLICE * 3));
        assert_eq!(fast, by_cv);
        let slow = play([0.0, 0.25, 0.0, 0.0], 0.0, 0.0, 240, SLICE * 3);
        assert!(rms(&slow[SLICE..]) > 0.0, "half speed runs past one slice");
        assert!(fast < normal);
    }

    #[test]
    fn slice_knob_and_cv_pick_slices() {
        let first = play([0.0, 0.5, 0.0, 0.0], 0.0, 0.0, 240, SLICE);
        let by_knob = play([1.0 / SLICES as f32, 0.5, 0.0, 0.0], 0.0, 0.0, 240, SLICE);
        let by_cv = play([0.0, 0.5, 0.0, 0.0], 10.0 / SLICES as f32, 0.0, 240, SLICE);
        assert_ne!(first, by_knob);
        assert_eq!(by_knob, by_cv);
    }

    #[test]
    fn reverse_plays_the_slice_backwards() {
        let forward = play([0.0, 0.5, 0.0, 0.0], 0.0, 0.0, 240, SLICE);
        let backward = play([0.0, 0.5, 1.0, 0.0], 0.0, 0.0, 240, SLICE);
        let half = SLICE / 2;
        assert!(
            rms(&forward[..half]) > rms(&forward[half..]) * 3.0,
            "a ping dies away"
        );
        assert!(
            rms(&backward[half..]) > rms(&backward[..half]) * 3.0,
            "reversed it swells"
        );
    }

    #[test]
    fn looping_holds_while_the_gate_is_high_and_ends_on_release() {
        let params = [0.0, 0.5, 0.0, 1.0];
        let held = play(params, 0.0, 0.0, SLICE * 3, SLICE * 5);
        assert!(length(&held[..SLICE * 3]) > SLICE * 2, "still looping");
        assert!(
            length(&held) < SLICE * 3 + 480,
            "stopped within 10 ms of release"
        );
    }
}
