use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, Edge, GATE_HIGH, Gate, OnePole, lerp, smoothing};

use crate::{Dsp, Io, steps};

const STEP_TIP: &str = "One step of the drawn pattern. Draw it on the roll rather than by hand: 0 is a rest, 0.04 to 0.48 are the twelve notes C to B, and 0.52 to 0.96 are the same notes accented.";

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Inlay",
    band: Band::Time,
    hp: 18,
    summary: "Drawn note pattern",
    params: &[
        param(
            "length",
            "Length",
            1.0,
            "How many steps of the pattern play before it starts again, 1–16.",
        ),
        param(
            "transpose",
            "Transpose",
            0.5,
            "Moves every note up or down by whole semitones, from -12 to +12. Centre is 0.",
        ),
        param(
            "octave",
            "Octave",
            0.5,
            "Shifts every note up or down by whole octaves, from -2 to +2. Centre is 0.",
        ),
        param(
            "rate",
            "Rate",
            0.6,
            "Free-running speed as a note length at the patch tempo: quarter, eighth, eighth triplet, sixteenth, sixteenth triplet or thirty-second. It is only used while nothing is patched into Clock.",
        ),
        param(
            "gate",
            "Gate",
            0.5,
            "How long each note is held, from a short click to nearly the whole step. Notes tied across neighbouring steps are held through.",
        ),
        param(
            "glide",
            "Glide",
            0.0,
            "Slides pitch from one note to the next.",
        ),
        param("step1", "Step 1", 0.0, STEP_TIP),
        param("step2", "Step 2", 0.0, STEP_TIP),
        param("step3", "Step 3", 0.0, STEP_TIP),
        param("step4", "Step 4", 0.0, STEP_TIP),
        param("step5", "Step 5", 0.0, STEP_TIP),
        param("step6", "Step 6", 0.0, STEP_TIP),
        param("step7", "Step 7", 0.0, STEP_TIP),
        param("step8", "Step 8", 0.0, STEP_TIP),
        param("step9", "Step 9", 0.0, STEP_TIP),
        param("step10", "Step 10", 0.0, STEP_TIP),
        param("step11", "Step 11", 0.0, STEP_TIP),
        param("step12", "Step 12", 0.0, STEP_TIP),
        param("step13", "Step 13", 0.0, STEP_TIP),
        param("step14", "Step 14", 0.0, STEP_TIP),
        param("step15", "Step 15", 0.0, STEP_TIP),
        param("step16", "Step 16", 0.0, STEP_TIP),
    ],
    inputs: &[
        jack(
            "clock",
            "Clock",
            Clock,
            "Advances one step per pulse. Leave it unpatched and Inlay runs by itself at the patch tempo.",
        ),
        jack(
            "reset",
            "Reset",
            Gate,
            "Back to step 1, and the note that is sounding stops.",
        ),
        jack(
            "transpose",
            "Transpose",
            Pitch,
            "Added to every note. 1 V = 1 octave.",
        ),
    ],
    outputs: &[
        jack("pitch", "Pitch", Pitch, "Pitch of the current note."),
        jack(
            "gate",
            "Gate",
            Gate,
            "High while a note sounds. Neighbouring steps on the same row are tied: one gate, no new attack.",
        ),
        jack(
            "accent",
            "Accent",
            Cv,
            "Full level on an accented step, lower on a plain one.",
        ),
    ],
};

/// Input positions, in descriptor order.
const CLOCK: usize = 0;
const RESET: usize = 1;
const TRANSPOSE: usize = 2;

/// Params before the steps: length, transpose, octave, rate, gate, glide.
const KNOBS: usize = 6;
const STEPS: usize = 16;
/// Notes on the roll, C to B.
const ROWS: u8 = 12;
/// A step param holds one of 25 levels: 0 a rest, 1..=12 a note, 13..=24 an
/// accented note. The level is `round(value * 25)`, so the values print as
/// short decimals (0.04 apiece).
const LEVELS: f32 = 25.0;

/// Beats (quarter notes) per step, in knob order.
const DIVISIONS: [f32; 6] = [1.0, 0.5, 1.0 / 3.0, 0.25, 1.0 / 6.0, 0.125];

/// A note's gate is this share of its step at the bottom and top of Gate.
const SHORTEST_GATE: f32 = 0.05;
const LONGEST_GATE: f32 = 0.95;

const ACCENT_PLAIN: f32 = 0.6 * CV_MAX;
const ACCENT_FULL: f32 = CV_MAX;

/// One drawn step: which of the twelve rows, counted up from C, and whether
/// it is accented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Note {
    row: u8,
    accent: bool,
}

/// The note a step param draws; `None` for a rest.
fn note(value: f32) -> Option<Note> {
    let level = ((value.clamp(0.0, 1.0) * LEVELS).round() as u8).min(2 * ROWS);
    (level > 0).then(|| Note {
        row: (level - 1) % ROWS,
        accent: level > ROWS,
    })
}

pub struct State {
    clock: Gate,
    reset: Gate,
    /// The next free-run tick comes at once rather than after a step.
    restart: bool,
    /// Samples since the free-run tick.
    phase: f32,
    /// Samples since the last clock pulse, and the last gap between two.
    since_tick: f32,
    ticked: bool,
    measured: Option<f32>,
    /// The step that is playing; `usize::MAX` until the first tick.
    index: usize,
    /// The row of the previous step, `None` after a rest.
    last: Option<u8>,
    /// The next step is the same note: keep the gate up through this one.
    hold: bool,
    gate_left: u32,
    /// The gate must drop for this one sample so the next note is a new one.
    gap: bool,
    high: bool,
    /// The row being played, in volts.
    target: f32,
    glide: OnePole,
    accent: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            clock: Gate::default(),
            reset: Gate::default(),
            restart: true,
            phase: 0.0,
            since_tick: 0.0,
            ticked: false,
            measured: None,
            index: usize::MAX,
            last: None,
            hold: false,
            gate_left: 0,
            gap: false,
            high: false,
            target: 0.0,
            glide: OnePole::default(),
            accent: 0.0,
        }
    }
}

impl State {
    /// Moves to the next step, which lasts about `interval` samples.
    fn advance(&mut self, steps: &[f32; STEPS], length: usize, interval: f32, share: f32) {
        self.index = self.index.wrapping_add(1) % length;
        let next = (self.index + 1) % length;
        let tied_in = self.hold;
        self.hold = false;
        let Some(here) = note(steps[self.index]) else {
            self.gate_left = 0;
            self.last = None;
            return;
        };
        let tied_in = tied_in && self.last == Some(here.row);
        self.last = Some(here.row);
        self.hold = next != self.index && note(steps[next]).is_some_and(|n| n.row == here.row);
        if !tied_in {
            self.target = f32::from(here.row) / 12.0;
            self.accent = if here.accent {
                ACCENT_FULL
            } else {
                ACCENT_PLAIN
            };
            self.gap = self.high;
        }
        // One sample short of the step at most, so the gate always drops.
        let whole = interval as u32;
        self.gate_left = ((interval * share) as u32).clamp(1, whole.saturating_sub(1).max(1));
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let [length, transpose, octave, rate, gate, glide, ..] = io.params else {
            return;
        };
        let Some(roll) = io.params[KNOBS..].first_chunk::<STEPS>() else {
            return;
        };
        let free_run = !io.connected[CLOCK];
        let beats = DIVISIONS[steps(*rate, DIVISIONS.len() - 1)];
        let free_interval = beats * 60.0 / io.tempo.max(1.0) * io.sample_rate;

        if self.reset.update(io.inputs[RESET]) == Edge::Rose {
            self.index = usize::MAX;
            self.last = None;
            self.hold = false;
            self.gate_left = 0;
            self.restart = free_run;
        }

        self.since_tick += 1.0;
        let mut tick = false;
        if free_run {
            if self.restart {
                self.restart = false;
                self.phase = 0.0;
                tick = true;
            } else {
                self.phase += 1.0;
                if self.phase >= free_interval {
                    // Capped, so an absurd tempo cannot bank ticks for later.
                    self.phase = (self.phase - free_interval).min(free_interval);
                    tick = true;
                }
            }
        } else if self.clock.update(io.inputs[CLOCK]) == Edge::Rose {
            if self.ticked {
                self.measured = Some(self.since_tick);
            }
            self.ticked = true;
            self.since_tick = 0.0;
            tick = true;
        }
        if tick {
            let interval = if free_run {
                free_interval
            } else {
                self.measured.unwrap_or(free_interval)
            };
            let share = lerp(SHORTEST_GATE, LONGEST_GATE, gate.clamp(0.0, 1.0));
            self.advance(roll, 1 + steps(*length, STEPS - 1), interval, share);
        }

        let coeff = smoothing(glide * glide * 0.25, io.sample_rate);
        let shift = (steps(*transpose, 24) as f32 - 12.0) / 12.0 + steps(*octave, 4) as f32 - 2.0;
        io.outputs[0] = self.glide.process(self.target, coeff) + shift + io.inputs[TRANSPOSE];
        self.high = if self.gap {
            self.gap = false;
            false
        } else if self.hold {
            true
        } else if self.gate_left > 0 {
            self.gate_left -= 1;
            true
        } else {
            false
        };
        io.outputs[1] = if self.high { GATE_HIGH } else { 0.0 };
        io.outputs[2] = self.accent;
    }
}

#[cfg(test)]
mod tests {
    use super::{DESCRIPTOR, KNOBS, LEVELS, Note, STEPS, State, note};
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;
    /// Samples between external clock pulses in most tests.
    const EVERY: usize = 1000;

    /// The param value that draws `row` (0 = C), accented or not.
    fn drawn(row: u8, accent: bool) -> f32 {
        let level = u32::from(row) + 1 + if accent { 12 } else { 0 };
        level as f32 * 4.0 / 100.0
    }

    /// Params with `pattern` drawn from step 1: `None` is a rest.
    fn params(length: usize, pattern: &[Option<(u8, bool)>]) -> Vec<f32> {
        let mut p = vec![0.0; KNOBS + STEPS];
        p[0] = (length - 1) as f32 / 15.0;
        p[1] = 0.5;
        p[2] = 0.5;
        p[3] = 0.6;
        p[4] = 0.5;
        p[5] = 0.0;
        for (i, step) in pattern.iter().enumerate() {
            p[KNOBS + i] = step.map_or(0.0, |(row, accent)| drawn(row, accent));
        }
        p
    }

    /// Plain notes, one per entry.
    fn rows(rows: &[Option<u8>]) -> Vec<Option<(u8, bool)>> {
        rows.iter().map(|r| r.map(|row| (row, false))).collect()
    }

    /// What `feed` supplies per sample: clock volts (`None` = unpatched),
    /// reset volts, transpose volts.
    type Frame = (Option<f32>, f32, f32);

    fn render(
        params: &[f32],
        tempo: f32,
        samples: usize,
        mut feed: impl FnMut(usize) -> Frame,
    ) -> Vec<[f32; 3]> {
        let mut inlay = State::default();
        (0..samples)
            .map(|i| {
                let (clock, reset, transpose) = feed(i);
                let inputs = [clock.unwrap_or(0.0), reset, transpose];
                let connected = [clock.is_some(), false, false];
                let mut outputs = [0.0; 3];
                inlay.process(&mut Io {
                    sample_rate: SR,
                    tempo,
                    params,
                    inputs: &inputs,
                    connected: &connected,
                    outputs: &mut outputs,
                });
                outputs
            })
            .collect()
    }

    fn pulse(i: usize) -> f32 {
        if i % EVERY < 100 { 10.0 } else { 0.0 }
    }

    fn clocked(params: &[f32], steps: usize) -> Vec<[f32; 3]> {
        render(params, 120.0, EVERY * steps, |i| (Some(pulse(i)), 0.0, 0.0))
    }

    /// `(start, length)` of every high stretch of the gate.
    fn runs(out: &[[f32; 3]]) -> Vec<(usize, usize)> {
        (0..out.len())
            .filter(|&i| out[i][1] > 5.0 && (i == 0 || out[i - 1][1] <= 5.0))
            .map(|s| (s, out[s..].iter().take_while(|f| f[1] > 5.0).count()))
            .collect()
    }

    fn semitones(volts: f32) -> f32 {
        volts * 12.0
    }

    #[test]
    fn the_roll_holds_every_step_as_a_param() {
        assert_eq!(DESCRIPTOR.params.len(), KNOBS + STEPS);
        for (i, p) in DESCRIPTOR.params[KNOBS..].iter().enumerate() {
            assert_eq!(p.key, format!("step{}", i + 1));
            assert_eq!(p.default, 0.0);
        }
    }

    #[test]
    fn a_step_value_names_a_row_and_an_accent() {
        assert_eq!(note(0.0), None);
        assert_eq!(note(0.019), None);
        assert_eq!(
            note(0.04),
            Some(Note {
                row: 0,
                accent: false
            })
        );
        assert_eq!(
            note(0.48),
            Some(Note {
                row: 11,
                accent: false
            })
        );
        assert_eq!(
            note(0.52),
            Some(Note {
                row: 0,
                accent: true
            })
        );
        assert_eq!(
            note(0.96),
            Some(Note {
                row: 11,
                accent: true
            })
        );
        assert_eq!(
            note(1.0),
            Some(Note {
                row: 11,
                accent: true
            })
        );
        assert_eq!(note(f32::NAN), None);
        for row in 0..12 {
            for accent in [false, true] {
                assert_eq!(note(drawn(row, accent)), Some(Note { row, accent }));
            }
        }
        assert_eq!(LEVELS, 25.0);
    }

    #[test]
    fn plays_each_drawn_note_at_its_pitch() {
        let p = params(4, &rows(&[Some(0), Some(4), Some(7), None]));
        let out = clocked(&p, 8);
        let mid = |step: usize| out[step * EVERY + 300];
        for (step, want) in [(0, Some(0.0)), (1, Some(4.0)), (2, Some(7.0)), (3, None)] {
            match want {
                Some(semis) => {
                    assert!(mid(step)[1] > 5.0, "step {step}");
                    assert!(
                        (semitones(mid(step)[0]) - semis).abs() < 1e-3,
                        "step {step}"
                    );
                }
                None => assert_eq!(mid(step)[1], 0.0, "rest"),
            }
        }
        // The pattern comes round again.
        assert!((semitones(mid(5)[0]) - 4.0).abs() < 1e-3);
    }

    #[test]
    fn every_note_gets_its_own_gate_and_rests_get_none() {
        let p = params(4, &rows(&[Some(0), Some(4), None, Some(7)]));
        let out = clocked(&p, 8);
        // The first pass has no measured step length yet, so judge the second.
        let starts: Vec<usize> = runs(&out)
            .iter()
            .map(|r| r.0)
            .filter(|&s| s >= 4 * EVERY)
            .collect();
        assert_eq!(starts, [4 * EVERY, 5 * EVERY, 7 * EVERY]);
        assert!(out[2 * EVERY + 10][1] == 0.0);
    }

    #[test]
    fn neighbours_on_one_row_are_tied_into_one_gate() {
        let p = params(4, &rows(&[Some(2), Some(2), Some(5), None]));
        let out = clocked(&p, 4);
        let gates = runs(&out);
        assert_eq!(gates.len(), 2, "{gates:?}");
        // The tied pair is one unbroken gate across the step boundary.
        assert_eq!(gates[0].0, 0);
        assert!(gates[0].1 > EVERY + 400, "{gates:?}");
        assert!(out[EVERY - 1][1] > 5.0 && out[EVERY][1] > 5.0);
        assert_eq!(gates[1].0, 2 * EVERY);
        // No new attack: the pitch never moved.
        assert!((0..2 * EVERY).all(|i| (semitones(out[i][0]) - 2.0).abs() < 1e-3));
    }

    #[test]
    fn a_tie_wraps_round_the_end_of_the_pattern() {
        let p = params(3, &rows(&[Some(1), Some(6), Some(1)]));
        let out = clocked(&p, 6);
        let gates = runs(&out);
        // Step 3 ties into step 1 on the next pass: one gate across the seam.
        assert!(
            gates
                .iter()
                .any(|&(s, l)| s == 2 * EVERY && l > EVERY + 400),
            "{gates:?}"
        );
    }

    #[test]
    fn a_single_step_does_not_tie_to_itself() {
        let p = params(1, &rows(&[Some(3)]));
        let out = clocked(&p, 4);
        assert_eq!(runs(&out).len(), 4);
    }

    #[test]
    fn different_rows_next_to_each_other_retrigger() {
        let p = params(2, &rows(&[Some(3), Some(4)]));
        let out = clocked(&p, 4);
        assert_eq!(runs(&out).len(), 4);
    }

    #[test]
    fn length_cuts_the_pattern_short() {
        let p = params(3, &rows(&[Some(0), Some(2), Some(4), Some(5), Some(7)]));
        let out = clocked(&p, 7);
        let heard: Vec<i32> = (0..7)
            .map(|s| semitones(out[s * EVERY + 600][0]).round() as i32)
            .collect();
        assert_eq!(heard, [0, 2, 4, 0, 2, 4, 0]);
    }

    #[test]
    fn transpose_octave_and_cv_all_move_the_pitch() {
        let mut p = params(1, &rows(&[Some(2)]));
        p[1] = 19.0 / 24.0; // +7 semitones
        p[2] = 0.75; // +1 octave
        let out = render(&p, 120.0, 2 * EVERY, |i| (Some(pulse(i)), 0.0, 0.25));
        let want = 2.0 / 12.0 + 7.0 / 12.0 + 1.0 + 0.25;
        assert!((out[600][0] - want).abs() < 1e-4, "{}", out[600][0]);
        p[1] = 0.0; // -12
        p[2] = 0.0; // -2
        let out = clocked(&p, 2);
        assert!((out[600][0] - (2.0 / 12.0 - 3.0)).abs() < 1e-4);
    }

    #[test]
    fn accented_steps_are_louder() {
        let p = params(3, &[Some((0, false)), Some((2, true)), Some((4, false))]);
        let out = clocked(&p, 3);
        let (plain, loud) = (out[600][2], out[EVERY + 600][2]);
        assert!(
            loud > plain && loud <= 10.0 && plain > 0.0,
            "{plain} {loud}"
        );
        assert_eq!(out[2 * EVERY + 600][2], plain);
    }

    #[test]
    fn gate_sets_how_long_a_note_is_held() {
        let mut p = params(2, &rows(&[Some(0), Some(3)]));
        let mut length = |gate: f32| {
            p[4] = gate;
            runs(&clocked(&p, 6))[2].1
        };
        let short = length(0.0);
        let mid = length(0.5);
        let long = length(1.0);
        assert!(short < mid && mid < long, "{short} {mid} {long}");
        assert!(long < EVERY, "the gate always drops before the next step");
        assert!(short >= 1);
    }

    #[test]
    fn reset_goes_back_to_step_one_and_stops_the_note() {
        let p = params(4, &rows(&[Some(0), Some(2), Some(4), Some(5)]));
        // Reset lands during step 3 (samples 2000..3000), 500 samples in.
        let out = render(&p, 120.0, 6 * EVERY, |i| {
            let reset = if (2500..2600).contains(&i) { 10.0 } else { 0.0 };
            (Some(pulse(i)), reset, 0.0)
        });
        assert!((semitones(out[2 * EVERY + 100][0]) - 4.0).abs() < 1e-3);
        assert_eq!(out[2700][1], 0.0, "the gate is cut");
        // The next clock plays step 1 again.
        assert!((semitones(out[3 * EVERY + 600][0])).abs() < 1e-3);
        assert!((semitones(out[4 * EVERY + 600][0]) - 2.0).abs() < 1e-3);
    }

    #[test]
    fn runs_by_itself_at_the_patch_tempo_when_no_clock_is_patched() {
        let p = params(4, &rows(&[Some(0), Some(2), Some(4), Some(5)]));
        // 120 BPM, sixteenth notes: 0.125 s = 6000 samples.
        let out = render(&p, 120.0, 6000 * 5, |_| (None, 0.0, 0.0));
        let starts: Vec<usize> = runs(&out).iter().map(|r| r.0).collect();
        assert_eq!(starts, [0, 6000, 12_000, 18_000, 24_000]);
        // Twice the tempo, twice as fast.
        let out = render(&p, 240.0, 6000 * 2, |_| (None, 0.0, 0.0));
        assert_eq!(runs(&out).len(), 4);
    }

    #[test]
    fn rate_picks_the_free_running_note_length() {
        let p = |rate: f32| {
            let mut p = params(2, &rows(&[Some(0), Some(2)]));
            p[3] = rate;
            p
        };
        // Quarter notes at 120 BPM: 24 000 samples. Thirty-seconds: 3000.
        let slow = render(&p(0.0), 120.0, 48_000, |_| (None, 0.0, 0.0));
        let fast = render(&p(1.0), 120.0, 48_000, |_| (None, 0.0, 0.0));
        assert_eq!(runs(&slow).len(), 2);
        assert_eq!(runs(&fast).len(), 16);
    }

    #[test]
    fn a_patched_clock_overrides_the_rate() {
        let mut p = params(2, &rows(&[Some(0), Some(2)]));
        p[3] = 0.0;
        let out = clocked(&p, 4);
        assert_eq!(runs(&out).len(), 4);
    }

    #[test]
    fn glide_slides_between_notes() {
        let mut p = params(2, &rows(&[Some(0), Some(7)]));
        p[5] = 1.0;
        let out = clocked(&p, 4);
        // Step 2 is the higher note; it is still on its way up 50 samples in.
        let early = semitones(out[EVERY + 50][0]);
        let late = semitones(out[2 * EVERY - 1][0]);
        assert!(0.0 < early && early < late && late < 7.0, "{early} {late}");
        p[5] = 0.0;
        let out = clocked(&p, 4);
        assert!((semitones(out[EVERY + 50][0]) - 7.0).abs() < 1e-3);
    }

    #[test]
    fn an_empty_pattern_stays_silent() {
        let p = params(16, &[]);
        let out = clocked(&p, 20);
        assert!(runs(&out).is_empty());
    }
}
