use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{GATE_HIGH, PULSE_SECONDS};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Fret",
    band: Band::Time,
    hp: 6,
    summary: "Quantiser",
    params: &[
        param("root", "Root", 0.75, "Key centre, C to B."),
        param(
            "scale",
            "Scale",
            0.3,
            "Which notes are allowed: major, minor, dorian, pentatonic and more.",
        ),
        param(
            "octave",
            "Octave",
            0.5,
            "Shifts the output up or down in whole octaves.",
        ),
    ],
    inputs: &[jack(
        "pitch",
        "Pitch",
        Pitch,
        "Any pitch. It is moved to the nearest note in the scale.",
    )],
    outputs: &[
        jack(
            "pitch",
            "Pitch",
            Pitch,
            "The incoming pitch, snapped to the scale.",
        ),
        jack("changed", "Changed", Gate, "Pulses when the note changes."),
    ],
};

/// Semitones above the root that each scale allows, as a bit mask.
const fn mask(degrees: &[u8]) -> u16 {
    let mut bits = 0;
    let mut i = 0;
    while i < degrees.len() {
        bits |= 1 << degrees[i];
        i += 1;
    }
    bits
}

/// In knob order, from the sparse pentatonics to the seven-note modes. Mitre's
/// scale lock reads the same table.
pub(crate) const SCALES: [u16; 8] = [
    mask(&[0, 2, 4, 7, 9]),
    mask(&[0, 3, 5, 7, 10]),
    mask(&[0, 2, 4, 5, 7, 9, 11]),
    mask(&[0, 2, 3, 5, 7, 8, 10]),
    mask(&[0, 2, 3, 5, 7, 9, 10]),
    mask(&[0, 2, 4, 5, 7, 9, 10]),
    mask(&[0, 1, 3, 5, 7, 8, 10]),
    mask(&[0, 2, 3, 5, 7, 8, 11]),
];

fn allowed(scale: u16, root: i32, note: i32) -> bool {
    scale >> (note - root).rem_euclid(12) & 1 == 1
}

/// The note (in semitones from C4) in the scale nearest to `semitones`.
pub(crate) fn nearest(scale: u16, root: i32, semitones: f32) -> i32 {
    let centre = semitones.round() as i32;
    let toward = if semitones >= centre as f32 { 1 } else { -1 };
    // Per step the nearer side comes first, so the first hit is the closest.
    (0..=6)
        .flat_map(|k| [centre + toward * k, centre - toward * k])
        .find(|&note| allowed(scale, root, note))
        .unwrap_or(centre)
}

#[derive(Default)]
pub struct State {
    /// The note last sent out, in semitones from C4, octave shift included.
    note: Option<i32>,
    pulse_left: u32,
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[root, scale, octave] = io.params else {
            return;
        };
        let root = (root * 12.0).round() as i32;
        let pick = (scale * (SCALES.len() - 1) as f32).round() as usize;
        let scale = SCALES[pick.min(SCALES.len() - 1)];
        let shift = 12 * ((octave - 0.5) * 8.0).round() as i32;
        let note = nearest(scale, root, io.inputs[0] * 12.0) + shift;
        if self.note.is_some_and(|last| last != note) {
            self.pulse_left = (PULSE_SECONDS * io.sample_rate) as u32;
        }
        self.note = Some(note);
        io.outputs[0] = note as f32 / 12.0;
        io.outputs[1] = if self.pulse_left > 0 {
            self.pulse_left -= 1;
            GATE_HIGH
        } else {
            0.0
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{SCALES, State, allowed};
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;

    /// Feeds `pitch` volts one sample at a time; returns `[pitch, changed]`.
    fn run(params: [f32; 3], pitch: impl Iterator<Item = f32>) -> Vec<[f32; 2]> {
        let mut fret = State::default();
        pitch
            .map(|v| {
                let mut out = [0.0; 2];
                fret.process(&mut Io {
                    sample_rate: SR,
                    tempo: 120.0,
                    params: &params,
                    inputs: &[v],
                    connected: &[true],
                    outputs: &mut out,
                });
                out
            })
            .collect()
    }

    fn sweep(from: f32, to: f32, samples: usize) -> impl Iterator<Item = f32> {
        (0..samples).map(move |i| from + (to - from) * i as f32 / samples as f32)
    }

    #[test]
    fn every_scale_and_root_lands_on_the_nearest_allowed_note() {
        for root in 0..12 {
            for (i, &scale) in SCALES.iter().enumerate() {
                let params = [root as f32 / 12.0, i as f32 / 7.0, 0.5];
                let input: Vec<f32> = sweep(-1.0, 1.0, 960).collect();
                for (v, out) in input.iter().zip(run(params, input.iter().copied())) {
                    let note = (out[0] * 12.0).round() as i32;
                    assert!((out[0] * 12.0 - note as f32).abs() < 1e-4);
                    assert!(allowed(scale, root, note), "root {root} scale {i}: {note}");
                    let miss = (v * 12.0 - note as f32).abs();
                    for other in -30..30 {
                        if allowed(scale, root, other) {
                            assert!(miss <= (v * 12.0 - other as f32).abs() + 1e-4);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn octave_shifts_the_output_in_whole_octaves() {
        let out = run([0.0, 0.0, 0.75], std::iter::repeat_n(0.0, 10));
        assert!((out[9][0] - 2.0).abs() < 1e-6);
        let out = run([0.0, 0.0, 0.0], std::iter::repeat_n(0.0, 10));
        assert!((out[9][0] + 4.0).abs() < 1e-6);
    }

    #[test]
    fn changed_pulses_once_per_new_note() {
        // C major from C4 to C5 takes seven steps; the first note is not a change.
        let out = run([0.0, 2.0 / 7.0, 0.5], sweep(0.0, 1.0, SR as usize));
        let pulses = out
            .windows(2)
            .filter(|w| w[0][1] < 1.0 && w[1][1] > 1.0)
            .count();
        assert_eq!(pulses, 7);
        assert!(out[0][1] < 1.0);
    }
}
