use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::CV_MAX;

use crate::fret::{SCALES, nearest};
use crate::{Dsp, Io, steps};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Mitre",
    band: Band::Time,
    hp: 8,
    summary: "Chord maker",
    params: &[
        param(
            "chord",
            "Chord",
            0.0,
            "Which chord to build: major, minor, sus2, sus4, diminished, augmented, maj7, min7, dom7, min9, then open major, open minor, stacked fifths and stacked fourths. Triads get their root again an octave up as voice 4.",
        ),
        param(
            "inversion",
            "Inversion",
            0.0,
            "Turns the chord over: each step lifts the lowest voice up an octave, from root position to the third inversion.",
        ),
        param(
            "spread",
            "Spread",
            0.0,
            "Opens the chord out. 0 keeps the voices close; higher steps lift voices 2, 3 and 4 by more octaves, up to 3 octaves for voice 4.",
        ),
        param(
            "root",
            "Root",
            0.75,
            "Key centre for Snap to scale, C to B. It does nothing while Snap is off.",
        ),
        param(
            "snap",
            "Snap to scale",
            0.0,
            "Moves any voice that falls outside a scale to the nearest note inside it. Off, or the same eight scales as Fret: major and minor pentatonic, major, minor, dorian, mixolydian, phrygian, harmonic minor.",
        ),
    ],
    inputs: &[
        jack(
            "pitch",
            "Pitch",
            Pitch,
            "The root of the chord. 1 V = 1 octave.",
        ),
        jack("gate", "Gate", Gate, "Passed straight to Gate out."),
        jack(
            "chord",
            "Chord",
            Cv,
            "Moves Chord from a voltage, so a sequence can walk through chords.",
        ),
    ],
    outputs: &[
        jack("voice1", "Voice 1", Pitch, "The lowest note of the chord."),
        jack("voice2", "Voice 2", Pitch, "The second note."),
        jack("voice3", "Voice 3", Pitch, "The third note."),
        jack("voice4", "Voice 4", Pitch, "The highest note."),
        jack(
            "gate",
            "Gate",
            Gate,
            "The incoming gate, so the chord and its notes share one gate.",
        ),
    ],
};

/// Semitones above the root for each voice, lowest first, in knob order.
const CHORDS: [[i32; 4]; 14] = [
    [0, 4, 7, 12],  // major
    [0, 3, 7, 12],  // minor
    [0, 2, 7, 12],  // sus2
    [0, 5, 7, 12],  // sus4
    [0, 3, 6, 12],  // diminished
    [0, 4, 8, 12],  // augmented
    [0, 4, 7, 11],  // maj7
    [0, 3, 7, 10],  // min7
    [0, 4, 7, 10],  // dom7
    [0, 3, 10, 14], // min9 (no fifth)
    [0, 7, 16, 19], // open major: root, fifth, tenth, twelfth
    [0, 7, 15, 19], // open minor
    [0, 7, 12, 19], // stacked fifths
    [0, 5, 10, 15], // stacked fourths
];

/// Semitone offsets of the four voices, lowest first.
fn voicing(chord: usize, inversion: usize, spread: usize) -> [i32; 4] {
    let mut notes = CHORDS[chord.min(CHORDS.len() - 1)];
    for note in notes.iter_mut().take(inversion) {
        *note += 12;
    }
    notes.sort_unstable();
    for (i, note) in notes.iter_mut().enumerate() {
        *note += 12 * (i * spread / 3) as i32;
    }
    notes
}

/// The settings a set of voices was worked out for.
#[derive(Clone, Copy, PartialEq)]
struct Setting {
    pitch: u32,
    chord: usize,
    inversion: usize,
    spread: usize,
    root: i32,
    snap: usize,
}

#[derive(Default)]
pub struct State {
    last: Option<(Setting, [f32; 4])>,
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[chord, inversion, spread, root, snap] = io.params else {
            return;
        };
        let chord = (chord + io.inputs[2] / CV_MAX).clamp(0.0, 1.0);
        let setting = Setting {
            pitch: io.inputs[0].to_bits(),
            chord: steps(chord, CHORDS.len() - 1),
            inversion: steps(inversion, 3),
            spread: steps(spread, 3),
            root: (root.clamp(0.0, 1.0) * 12.0).round() as i32,
            snap: steps(snap, SCALES.len()),
        };
        let voices = match self.last {
            Some((last, voices)) if last == setting => voices,
            _ => {
                let voices = work_out(&setting, io.inputs[0]);
                self.last = Some((setting, voices));
                voices
            }
        };
        io.outputs[..4].copy_from_slice(&voices);
        io.outputs[4] = io.inputs[1];
    }
}

fn work_out(setting: &Setting, pitch: f32) -> [f32; 4] {
    let notes = voicing(setting.chord, setting.inversion, setting.spread);
    notes.map(|offset| {
        if setting.snap == 0 {
            pitch + offset as f32 / 12.0
        } else {
            // Beyond 10 V either way is nonsense, and keeps the search's integers small.
            let semitones = pitch.clamp(-10.0, 10.0) * 12.0 + offset as f32;
            nearest(SCALES[setting.snap - 1], setting.root, semitones) as f32 / 12.0
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{CHORDS, State, voicing};
    use crate::fret::SCALES;
    use crate::{Dsp, Io};

    /// Runs one sample and returns `[voice1..voice4, gate]`.
    fn run(state: &mut State, params: [f32; 5], pitch: f32, gate: f32, chord_cv: f32) -> [f32; 5] {
        let mut out = [0.0; 5];
        state.process(&mut Io {
            sample_rate: 48_000.0,
            tempo: 120.0,
            params: &params,
            inputs: &[pitch, gate, chord_cv],
            connected: &[true, true, true],
            outputs: &mut out,
        });
        out
    }

    fn semitones(out: [f32; 5]) -> [f32; 4] {
        [0, 1, 2, 3].map(|i| out[i] * 12.0)
    }

    fn close(got: [f32; 4], want: [i32; 4]) {
        for (g, w) in got.iter().zip(want) {
            assert!((g - w as f32).abs() < 1e-4, "{got:?} vs {want:?}");
        }
    }

    /// The knob position that picks entry `i` of `count + 1` steps.
    fn knob(i: usize, count: usize) -> f32 {
        i as f32 / count as f32
    }

    #[test]
    fn every_chord_has_its_exact_intervals() {
        for (i, want) in CHORDS.iter().enumerate() {
            let out = run(
                &mut State::default(),
                [knob(i, 13), 0.0, 0.0, 0.0, 0.0],
                0.0,
                0.0,
                0.0,
            );
            close(semitones(out), *want);
        }
        // Spot checks by ear: minor third, dominant seventh, minor ninth.
        assert_eq!(CHORDS[1], [0, 3, 7, 12]);
        assert_eq!(CHORDS[8], [0, 4, 7, 10]);
        assert_eq!(CHORDS[9], [0, 3, 10, 14]);
    }

    #[test]
    fn the_chord_follows_the_root_in_volts() {
        // 0.25 V is three semitones: a C major chord becomes E-flat major.
        let out = run(&mut State::default(), [0.0; 5], 0.25, 0.0, 0.0);
        close(semitones(out), [3, 7, 10, 15]);
        let out = run(&mut State::default(), [0.0; 5], -1.0, 0.0, 0.0);
        close(semitones(out), [-12, -8, -5, 0]);
    }

    #[test]
    fn inversions_lift_the_lowest_voices_by_octaves() {
        assert_eq!(voicing(0, 0, 0), [0, 4, 7, 12]);
        assert_eq!(voicing(0, 1, 0), [4, 7, 12, 12]);
        assert_eq!(voicing(0, 2, 0), [7, 12, 12, 16]);
        assert_eq!(voicing(6, 1, 0), [4, 7, 11, 12]);
        assert_eq!(voicing(6, 2, 0), [7, 11, 12, 16]);
        assert_eq!(voicing(6, 3, 0), [11, 12, 16, 19]);
        // Through the knob: 1/3 and 1 are the first and third inversions.
        let out = run(
            &mut State::default(),
            [knob(6, 13), 1.0 / 3.0, 0.0, 0.0, 0.0],
            0.0,
            0.0,
            0.0,
        );
        close(semitones(out), [4, 7, 11, 12]);
    }

    #[test]
    fn spread_lifts_upper_voices_by_whole_octaves() {
        // Min7 is 0, 3, 7, 10.
        assert_eq!(voicing(7, 0, 0), [0, 3, 7, 10]);
        assert_eq!(voicing(7, 0, 1), [0, 3, 7, 22]);
        assert_eq!(voicing(7, 0, 2), [0, 3, 19, 34]);
        assert_eq!(voicing(7, 0, 3), [0, 15, 31, 46]);
        let out = run(
            &mut State::default(),
            [knob(7, 13), 0.0, 1.0, 0.0, 0.0],
            0.0,
            0.0,
            0.0,
        );
        close(semitones(out), [0, 15, 31, 46]);
    }

    #[test]
    fn voices_stay_in_order_for_every_setting() {
        for chord in 0..14 {
            for inversion in 0..4 {
                for spread in 0..4 {
                    let v = voicing(chord, inversion, spread);
                    assert!(v.windows(2).all(|w| w[0] <= w[1]), "{v:?}");
                }
            }
        }
    }

    #[test]
    fn chord_input_moves_the_chord_knob() {
        // 10 V is the whole knob: the last chord, stacked fourths.
        let out = run(&mut State::default(), [0.0; 5], 0.0, 0.0, 10.0);
        close(semitones(out), [0, 5, 10, 15]);
        // 10/13 of the way is entry 10 (open major).
        let out = run(
            &mut State::default(),
            [0.0; 5],
            0.0,
            0.0,
            10.0 * 10.0 / 13.0,
        );
        close(semitones(out), [0, 7, 16, 19]);
    }

    #[test]
    fn gate_passes_straight_through() {
        let mut state = State::default();
        for gate in [0.0, 10.0, 3.3, 0.0] {
            assert_eq!(run(&mut state, [0.5; 5], 0.0, gate, 0.0)[4], gate);
        }
    }

    #[test]
    fn snap_moves_stray_voices_into_the_scale_and_leaves_the_rest() {
        // A natural minor (root A = 0.75, scale 3 of 0..7 = snap step 4).
        let snap = knob(4, 8);
        let allowed = |note: i32| SCALES[3] >> (note - 9).rem_euclid(12) & 1 == 1;
        // C major from C: C E G C are all in A minor, so nothing moves.
        let out = run(
            &mut State::default(),
            [0.0, 0.0, 0.0, 0.75, snap],
            0.0,
            0.0,
            0.0,
        );
        close(semitones(out), [0, 4, 7, 12]);
        // Rooted on D: D F# A D. F# is not in A minor; the others stay put.
        let two = 2.0 / 12.0;
        let out = semitones(run(
            &mut State::default(),
            [0.0, 0.0, 0.0, 0.75, snap],
            two,
            0.0,
            0.0,
        ));
        assert!((out[0] - 2.0).abs() < 1e-4 && (out[2] - 9.0).abs() < 1e-4);
        assert!((out[3] - 14.0).abs() < 1e-4);
        assert!(
            (out[1] - 5.0).abs() < 1e-4 || (out[1] - 7.0).abs() < 1e-4,
            "{out:?}"
        );
        // Every voice of every chord on every root lands in the scale.
        for root in 0..12 {
            for chord in 0..14 {
                for step in 0..12 {
                    let pitch = step as f32 / 12.0;
                    let params = [knob(chord, 13), 0.0, 0.0, root as f32 / 12.0, knob(4, 8)];
                    for v in semitones(run(&mut State::default(), params, pitch, 0.0, 0.0)) {
                        let note = v.round() as i32;
                        assert!((v - note as f32).abs() < 1e-4);
                        let scale = SCALES[3];
                        assert!(scale >> (note - root).rem_euclid(12) & 1 == 1, "{note}");
                    }
                }
            }
        }
        // And snap really is off at 0.
        let out = semitones(run(
            &mut State::default(),
            [0.0, 0.0, 0.0, 0.75, 0.0],
            two,
            0.0,
            0.0,
        ));
        close(out, [2, 6, 9, 14]);
        assert!(!allowed(6));
    }

    #[test]
    fn a_changed_setting_is_never_served_from_the_cache() {
        let mut state = State::default();
        let first = run(&mut state, [0.0; 5], 0.0, 0.0, 0.0);
        let same = run(&mut state, [0.0; 5], 0.0, 0.0, 0.0);
        assert_eq!(first, same);
        let moved = run(&mut state, [0.0; 5], 0.5, 0.0, 0.0);
        close(semitones(moved), [6, 10, 13, 18]);
        let minor = run(&mut state, [knob(1, 13), 0.0, 0.0, 0.0, 0.0], 0.5, 0.0, 0.0);
        close(semitones(minor), [6, 9, 13, 18]);
        let back = run(&mut state, [0.0; 5], 0.0, 0.0, 0.0);
        assert_eq!(first, back);
    }

    #[test]
    fn extreme_settings_stay_finite_and_bounded() {
        for chord in [0.0, 0.5, 1.0] {
            for knobs in [0.0, 0.5, 1.0] {
                for pitch in [-10.0, 0.0, 10.0, 1e30, -1e30] {
                    for snap in [0.0, 0.5, 1.0] {
                        let params = [chord, knobs, knobs, knobs, snap];
                        let out = run(&mut State::default(), params, pitch, 10.0, 10.0);
                        assert!(out.iter().all(|v| v.is_finite() || pitch.abs() > 100.0));
                        if pitch.abs() <= 10.0 || snap > 0.0 {
                            assert!(out[..4].iter().all(|v| v.abs() < 20.0), "{out:?}");
                        }
                    }
                }
            }
        }
        // A NaN pitch must not panic with snap on.
        run(
            &mut State::default(),
            [0.0, 0.0, 0.0, 0.5, 0.5],
            f32::NAN,
            0.0,
            0.0,
        );
    }
}
