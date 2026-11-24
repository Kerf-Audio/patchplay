use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, Edge, GATE_HIGH, Gate, PULSE_SECONDS, Rng, lerp};

use crate::{Dsp, Io, steps};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Rung",
    band: Band::Time,
    hp: 8,
    summary: "Arpeggiator",
    params: &[
        param(
            "pattern",
            "Pattern",
            0.0,
            "The order the notes are played in: up, down, up-down, down-up, random, as played, converge (outside in) or bounce (the lowest note between each of the others).",
        ),
        param(
            "octaves",
            "Octaves",
            0.0,
            "How many octaves the pattern climbs through, 1–4. The chord is repeated an octave higher each time.",
        ),
        param(
            "rate",
            "Rate",
            0.75,
            "Free-running speed as a note length at the patch tempo, from half notes to thirty-seconds. It is only used while nothing is patched into Clock.",
        ),
        param(
            "length",
            "Length",
            0.5,
            "How long each note is held, from a short click to nearly the whole step. A gap always remains, so every note is a fresh gate.",
        ),
        param(
            "swing",
            "Swing",
            0.0,
            "Delays every second step. 0 is straight; 1 puts it three-quarters of the way to the next beat.",
        ),
        param(
            "ratchet",
            "Ratchet",
            0.0,
            "Probability that a step is played twice, as two quick notes in the time of one.",
        ),
    ],
    inputs: &[
        jack(
            "clock",
            "Clock",
            Clock,
            "Advances one step per pulse. Leave it unpatched and Rung runs by itself at the patch tempo.",
        ),
        jack(
            "gate",
            "Gate",
            Gate,
            "Plays the arpeggio while high and starts it again from the first step on each rise. Leave it unpatched to play all the time.",
        ),
        jack(
            "voice1",
            "Voice 1",
            Pitch,
            "A note of the chord. Patch just this one to repeat a single note, or all four from Mitre.",
        ),
        jack("voice2", "Voice 2", Pitch, "A second note of the chord."),
        jack("voice3", "Voice 3", Pitch, "A third note of the chord."),
        jack("voice4", "Voice 4", Pitch, "A fourth note of the chord."),
    ],
    outputs: &[
        jack("pitch", "Pitch", Pitch, "The note being played."),
        jack("gate", "Gate", Gate, "High while a note sounds."),
        jack(
            "accent",
            "Accent",
            Cv,
            "Full level on the first note of each run through the pattern, lower on the others and lower again on a ratchet repeat.",
        ),
        jack(
            "trig",
            "Trig",
            Gate,
            "A short pulse at the start of every note, repeats included.",
        ),
    ],
};

/// Input and output positions, in descriptor order.
const CLOCK: usize = 0;
const HOLD: usize = 1;
const VOICES: usize = 2;

/// Beats (quarter notes) per step, in knob order: 1/2, 1/4, dotted 1/8, 1/8,
/// 1/8 triplet, dotted 1/16, 1/16, 1/16 triplet, 1/32.
const DIVISIONS: [f32; 9] = [
    2.0,
    1.0,
    0.75,
    0.5,
    1.0 / 3.0,
    0.375,
    0.25,
    1.0 / 6.0,
    0.125,
];

/// A note's gate is this share of its step at the bottom and top of Length.
const SHORTEST_GATE: f32 = 0.05;
const LONGEST_GATE: f32 = 0.95;

const ACCENT_FIRST: f32 = CV_MAX;
const ACCENT_STEP: f32 = 0.6 * CV_MAX;
const ACCENT_REPEAT: f32 = 0.4 * CV_MAX;

/// Most notes in a pool: four voices over four octaves.
const POOL: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pattern {
    Up,
    Down,
    UpDown,
    DownUp,
    Random,
    AsPlayed,
    Converge,
    Bounce,
}

/// In knob order.
const PATTERNS: [Pattern; 8] = [
    Pattern::Up,
    Pattern::Down,
    Pattern::UpDown,
    Pattern::DownUp,
    Pattern::Random,
    Pattern::AsPlayed,
    Pattern::Converge,
    Pattern::Bounce,
];

/// Steps before `pattern` over `m` notes comes round again.
fn period(pattern: Pattern, m: usize) -> u32 {
    match pattern {
        Pattern::UpDown | Pattern::DownUp | Pattern::Bounce if m > 1 => 2 * m as u32 - 2,
        Pattern::UpDown | Pattern::DownUp | Pattern::Bounce => 1,
        _ => m as u32,
    }
}

/// Which of `m` notes (lowest first, or in input order for `AsPlayed`) plays
/// on `step`. `Random` is the caller's job; it counts as `Up` here.
fn position(pattern: Pattern, m: usize, step: u32) -> usize {
    if m < 2 {
        return 0;
    }
    let k = (step % period(pattern, m)) as usize;
    let fold = |k: usize| if k < m { k } else { 2 * m - 2 - k };
    match pattern {
        Pattern::Up | Pattern::Random | Pattern::AsPlayed => k,
        Pattern::Down => m - 1 - k,
        Pattern::UpDown => fold(k),
        Pattern::DownUp => m - 1 - fold(k),
        Pattern::Converge => {
            if k % 2 == 0 {
                k / 2
            } else {
                m - 1 - k / 2
            }
        }
        Pattern::Bounce => {
            if k % 2 == 0 {
                0
            } else {
                k / 2 + 1
            }
        }
    }
}

/// The notes to play, in volts: every patched voice (or voice 1 alone when
/// none is), repeated an octave higher `octaves` times. Sorted low to high
/// unless `sorted` is false. Returns how many there are.
fn build_pool(
    inputs: &[f32],
    connected: &[bool],
    octaves: usize,
    sorted: bool,
    pool: &mut [f32; POOL],
) -> usize {
    let wired = &connected[VOICES..VOICES + 4];
    let any = wired.contains(&true);
    let mut base = [0.0; 4];
    let mut count = 0;
    for (i, &volts) in inputs[VOICES..VOICES + 4].iter().enumerate() {
        if wired[i] || (!any && i == 0) {
            base[count] = volts;
            count += 1;
        }
    }
    let mut n = 0;
    for octave in 0..octaves {
        for &note in &base[..count] {
            pool[n] = note + octave as f32;
            n += 1;
        }
    }
    if sorted {
        pool[..n].sort_unstable_by(f32::total_cmp);
    }
    n
}

/// Knob settings for one step.
struct Knobs {
    pattern: Pattern,
    octaves: usize,
    length: f32,
    ratchet: f32,
    /// Trigger pulse length in samples.
    pulse: u32,
}

pub struct State {
    rng: Rng,
    clock: Gate,
    hold: Gate,
    /// The next free-run tick comes at once rather than after a step.
    restart: bool,
    /// Samples since the free-run tick.
    phase: f32,
    /// Samples since the last clock pulse, and the last gap between two.
    since_tick: f32,
    ticked: bool,
    measured: Option<f32>,
    /// Steps taken since the pattern last restarted.
    step: u32,
    /// The step that is waiting to start or has just started.
    current: u32,
    /// Samples from this step's start to the next one's.
    slot: f32,
    pending: bool,
    start_in: u32,
    pitch: f32,
    accent: f32,
    hit_left: u32,
    hit_len: u32,
    repeat_in: u32,
    /// The gate must drop for this one sample so the next note is a new one.
    gap: bool,
    trig_left: u32,
}

impl State {
    /// `seed` fixes the random pattern and the ratchets: the same patch
    /// plays the same notes every time.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Rng::new(seed),
            clock: Gate::default(),
            hold: Gate::default(),
            restart: true,
            phase: 0.0,
            since_tick: 0.0,
            ticked: false,
            measured: None,
            step: 0,
            current: 0,
            slot: 0.0,
            pending: false,
            start_in: 0,
            pitch: 0.0,
            accent: 0.0,
            hit_left: 0,
            hit_len: 0,
            repeat_in: 0,
            gap: false,
            trig_left: 0,
        }
    }

    /// Stops sounding at once.
    fn silence(&mut self) {
        self.pending = false;
        self.hit_left = 0;
        self.repeat_in = 0;
        self.gap = false;
    }

    /// Starts the note for `self.current`.
    fn begin(&mut self, inputs: &[f32], connected: &[bool], knobs: &Knobs) {
        let mut pool = [0.0; POOL];
        let sorted = knobs.pattern != Pattern::AsPlayed;
        let m = build_pool(inputs, connected, knobs.octaves, sorted, &mut pool);
        let index = if knobs.pattern == Pattern::Random {
            ((self.rng.unit() * m as f32) as usize).min(m - 1)
        } else {
            position(knobs.pattern, m, self.current)
        };
        self.pitch = pool[index];
        self.accent = if self.current % period(knobs.pattern, m) == 0 {
            ACCENT_FIRST
        } else {
            ACCENT_STEP
        };
        let slot = self.slot as u32;
        let ratchet = slot >= 4 && knobs.ratchet > 0.0 && self.rng.unit() < knobs.ratchet;
        let span = if ratchet { slot / 2 } else { slot };
        let share = lerp(SHORTEST_GATE, LONGEST_GATE, knobs.length.clamp(0.0, 1.0));
        // One sample short of the span at most, so the gate always drops.
        let len = ((span as f32 * share).round() as u32).clamp(1, span.saturating_sub(1).max(1));
        self.gap = self.hit_left > 0;
        self.hit_len = len;
        self.hit_left = len;
        self.repeat_in = if ratchet { span } else { 0 };
        self.trig_left = knobs.pulse;
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[pattern, octaves, rate, length, swing, ratchet] = io.params else {
            return;
        };
        let knobs = Knobs {
            pattern: PATTERNS[steps(pattern, PATTERNS.len() - 1)],
            octaves: 1 + steps(octaves, 3),
            length,
            ratchet,
            pulse: (PULSE_SECONDS * io.sample_rate) as u32,
        };
        let free_run = !io.connected[CLOCK];
        let beats = DIVISIONS[steps(rate, DIVISIONS.len() - 1)];
        let free_interval = beats * 60.0 / io.tempo.max(1.0) * io.sample_rate;

        let playing = if io.connected[HOLD] {
            match self.hold.update(io.inputs[HOLD]) {
                Edge::Rose => {
                    self.silence();
                    self.step = 0;
                    self.restart = free_run;
                }
                Edge::Fell => self.silence(),
                Edge::None => {}
            }
            self.hold.is_high()
        } else {
            true
        };

        // A ratchet repeat starts a step's second note.
        if self.repeat_in > 0 {
            self.repeat_in -= 1;
            if self.repeat_in == 0 {
                self.hit_left = self.hit_len;
                self.accent = ACCENT_REPEAT;
                self.trig_left = knobs.pulse;
            }
        }

        self.since_tick += 1.0;
        let mut tick = false;
        if free_run {
            if playing {
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
            }
        } else if self.clock.update(io.inputs[CLOCK]) == Edge::Rose {
            if self.ticked {
                self.measured = Some(self.since_tick);
            }
            self.ticked = true;
            self.since_tick = 0.0;
            tick = playing;
        }

        if tick {
            // A step still waiting on its swing delay starts now, not never.
            if self.pending {
                self.pending = false;
                self.begin(io.inputs, io.connected, &knobs);
            }
            let interval = if free_run {
                free_interval
            } else {
                self.measured.unwrap_or(free_interval)
            };
            let odd = self.step % 2 == 1;
            let shift = swing.clamp(0.0, 1.0) * 0.5 * interval;
            self.slot = if odd {
                interval - shift
            } else {
                interval + shift
            };
            self.start_in = if odd { shift as u32 } else { 0 };
            self.current = self.step;
            self.step = self.step.wrapping_add(1);
            self.pending = true;
        } else if self.pending && self.start_in > 0 {
            self.start_in -= 1;
        }
        if self.pending && self.start_in == 0 {
            self.pending = false;
            self.begin(io.inputs, io.connected, &knobs);
        }

        io.outputs[0] = self.pitch;
        io.outputs[1] = if self.gap {
            self.gap = false;
            0.0
        } else if self.hit_left > 0 {
            self.hit_left -= 1;
            GATE_HIGH
        } else {
            0.0
        };
        io.outputs[2] = self.accent;
        io.outputs[3] = if self.trig_left > 0 {
            self.trig_left -= 1;
            GATE_HIGH
        } else {
            0.0
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{DESCRIPTOR, State};
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;
    /// Samples between external clock pulses in most tests.
    const EVERY: usize = 1000;

    /// `pattern`, `octaves`, `rate`, `length`, `swing`, `ratchet` as knob positions.
    #[derive(Clone, Copy)]
    struct Knobs {
        pattern: usize,
        octaves: usize,
        rate: f32,
        length: f32,
        swing: f32,
        ratchet: f32,
    }

    impl Default for Knobs {
        fn default() -> Self {
            Self {
                pattern: 0,
                octaves: 1,
                rate: 0.75,
                length: 0.5,
                swing: 0.0,
                ratchet: 0.0,
            }
        }
    }

    impl Knobs {
        fn params(self) -> [f32; 6] {
            [
                self.pattern as f32 / 7.0,
                (self.octaves - 1) as f32 / 3.0,
                self.rate,
                self.length,
                self.swing,
                self.ratchet,
            ]
        }
    }

    /// What `feed` returns for one sample: inputs and which are patched.
    type Frame = ([f32; 6], [bool; 6]);

    fn render(
        knobs: Knobs,
        seed: u64,
        tempo: f32,
        samples: usize,
        mut feed: impl FnMut(usize) -> Frame,
    ) -> Vec<[f32; 4]> {
        let mut rung = State::new(seed);
        let params = knobs.params();
        (0..samples)
            .map(|i| {
                let (inputs, connected) = feed(i);
                let mut outputs = [0.0; 4];
                rung.process(&mut Io {
                    sample_rate: SR,
                    tempo,
                    params: &params,
                    inputs: &inputs,
                    connected: &connected,
                    outputs: &mut outputs,
                });
                outputs
            })
            .collect()
    }

    /// An external clock every `EVERY` samples; `chord` in semitones fills
    /// the voice inputs in order and the rest are unpatched.
    fn clocked(knobs: Knobs, seed: u64, chord: &[i32], samples: usize) -> Vec<[f32; 4]> {
        render(knobs, seed, 120.0, samples, |i| frame(Some(i), None, chord))
    }

    fn frame(clock: Option<usize>, gate: Option<bool>, chord: &[i32]) -> Frame {
        let mut inputs = [0.0; 6];
        let mut connected = [false; 6];
        if let Some(i) = clock {
            connected[0] = true;
            inputs[0] = if i % EVERY < 100 { 10.0 } else { 0.0 };
        }
        if let Some(held) = gate {
            connected[1] = true;
            inputs[1] = if held { 10.0 } else { 0.0 };
        }
        for (k, &semitones) in chord.iter().enumerate() {
            inputs[2 + k] = semitones as f32 / 12.0;
            connected[2 + k] = true;
        }
        (inputs, connected)
    }

    /// Sample indices where `out[channel]` goes from low to high.
    fn rises(out: &[[f32; 4]], channel: usize) -> Vec<usize> {
        (0..out.len())
            .filter(|&i| out[i][channel] > 5.0 && (i == 0 || out[i - 1][channel] <= 5.0))
            .collect()
    }

    /// `(start, length)` of every high stretch of `out[channel]`.
    fn runs(out: &[[f32; 4]], channel: usize) -> Vec<(usize, usize)> {
        rises(out, channel)
            .into_iter()
            .map(|s| {
                let len = out[s..].iter().take_while(|f| f[channel] > 5.0).count();
                (s, len)
            })
            .collect()
    }

    /// The pitches (in semitones) at the start of each note.
    fn notes(out: &[[f32; 4]]) -> Vec<i32> {
        rises(out, 3)
            .into_iter()
            .map(|i| (out[i][0] * 12.0).round() as i32)
            .collect()
    }

    fn first_steps(pattern: usize, chord: &[i32], steps: usize) -> Vec<i32> {
        let knobs = Knobs {
            pattern,
            ..Knobs::default()
        };
        let mut n = notes(&clocked(knobs, 1, chord, EVERY * steps + 500));
        n.truncate(steps);
        n
    }

    #[test]
    fn each_pattern_plays_a_dominant_seventh_in_its_order() {
        let chord = [0, 4, 7, 10];
        let cases: [(usize, &str, [i32; 8]); 6] = [
            (0, "up", [0, 4, 7, 10, 0, 4, 7, 10]),
            (1, "down", [10, 7, 4, 0, 10, 7, 4, 0]),
            (2, "up-down", [0, 4, 7, 10, 7, 4, 0, 4]),
            (3, "down-up", [10, 7, 4, 0, 4, 7, 10, 7]),
            (6, "converge", [0, 10, 4, 7, 0, 10, 4, 7]),
            (7, "bounce", [0, 4, 0, 7, 0, 10, 0, 4]),
        ];
        for (pattern, name, want) in cases {
            assert_eq!(first_steps(pattern, &chord, 8), want, "{name}");
        }
    }

    #[test]
    fn as_played_keeps_the_input_order_and_the_others_sort() {
        let scrambled = [7, 0, 10, 4];
        assert_eq!(
            first_steps(5, &scrambled, 8),
            [7, 0, 10, 4, 7, 0, 10, 4],
            "as played"
        );
        assert_eq!(first_steps(0, &scrambled, 4), [0, 4, 7, 10], "up sorts");
        assert_eq!(first_steps(1, &scrambled, 4), [10, 7, 4, 0], "down sorts");
    }

    #[test]
    fn fewer_voices_make_shorter_patterns() {
        assert_eq!(first_steps(2, &[0, 7], 6), [0, 7, 0, 7, 0, 7]);
        assert_eq!(first_steps(2, &[0, 4, 7], 8), [0, 4, 7, 4, 0, 4, 7, 4]);
        assert_eq!(first_steps(7, &[0, 4, 7], 6), [0, 4, 0, 7, 0, 4]);
        // One note, every pattern: the same note over and over.
        for pattern in 0..8 {
            assert_eq!(
                first_steps(pattern, &[5], 3),
                [5, 5, 5],
                "pattern {pattern}"
            );
        }
    }

    #[test]
    fn nothing_patched_plays_c4() {
        let out = render(Knobs::default(), 1, 120.0, 3000, |_| frame(None, None, &[]));
        assert_eq!(notes(&out), [0]);
    }

    #[test]
    fn octaves_repeat_the_chord_higher() {
        let knobs = |octaves| Knobs {
            octaves,
            ..Knobs::default()
        };
        let steps = |octaves, chord: &[i32], n: usize| {
            let mut v = notes(&clocked(knobs(octaves), 1, chord, EVERY * n + 500));
            v.truncate(n);
            v
        };
        assert_eq!(steps(1, &[0, 4, 7], 6), [0, 4, 7, 0, 4, 7]);
        assert_eq!(steps(2, &[0, 4, 7], 7), [0, 4, 7, 12, 16, 19, 0]);
        assert_eq!(steps(4, &[0], 5), [0, 12, 24, 36, 0]);
        // A chord wider than an octave still climbs in order.
        assert_eq!(steps(2, &[0, 16, 19], 7), [0, 12, 16, 19, 28, 31, 0]);
        let down = Knobs {
            pattern: 1,
            octaves: 4,
            ..Knobs::default()
        };
        let mut n = notes(&clocked(down, 1, &[0], EVERY * 4 + 500));
        n.truncate(4);
        assert_eq!(n, [36, 24, 12, 0]);
    }

    #[test]
    fn gate_length_is_a_share_of_the_step() {
        for (length, want) in [(0.0, 50), (0.5, 500), (1.0, 950)] {
            let knobs = Knobs {
                length,
                ..Knobs::default()
            };
            let out = clocked(knobs, 1, &[0, 4], EVERY * 6);
            // The first step runs before the clock has been measured.
            for (start, len) in runs(&out, 1).into_iter().skip(1) {
                assert_eq!(len, want, "length {length} at {start}");
            }
        }
    }

    #[test]
    fn swing_delays_every_second_step() {
        let knobs = Knobs {
            swing: 1.0,
            ..Knobs::default()
        };
        let out = clocked(knobs, 1, &[0, 4], EVERY * 5);
        assert_eq!(
            rises(&out, 3)[..5],
            [0, 1500, 2000, 3500, 4000],
            "odd steps sit half a step late"
        );
        // Half swing is a quarter of a step.
        let knobs = Knobs {
            swing: 0.5,
            ..Knobs::default()
        };
        let out = clocked(knobs, 1, &[0, 4], EVERY * 5);
        assert_eq!(rises(&out, 3)[..5], [0, 1250, 2000, 3250, 4000]);
    }

    #[test]
    fn ratchets_play_a_step_twice() {
        let always = Knobs {
            ratchet: 1.0,
            ..Knobs::default()
        };
        let out = clocked(always, 1, &[0], EVERY * 5);
        // Step 0 is cut short by step 1; after that every step is two notes.
        assert_eq!(
            rises(&out, 3)[1..],
            [1000, 1500, 2000, 2500, 3000, 3500, 4000, 4500]
        );
        assert_eq!(rises(&out, 1)[2..], rises(&out, 3)[2..]);
        let never = clocked(Knobs::default(), 1, &[0], EVERY * 5);
        assert_eq!(rises(&never, 3), [0, 1000, 2000, 3000, 4000]);
        // Half the time: some, but not all, steps double up.
        let half = Knobs {
            ratchet: 0.5,
            ..Knobs::default()
        };
        let steps = 200;
        let out = clocked(half, 3, &[0], EVERY * steps);
        let count = rises(&out, 3).len();
        assert!(count > steps + 50 && count < steps + 150, "{count}");
        assert_eq!(out, clocked(half, 3, &[0], EVERY * steps), "same seed");
    }

    #[test]
    fn accent_marks_the_start_of_each_run_and_ratchet_repeats_are_softer() {
        let out = clocked(Knobs::default(), 1, &[0, 4, 7, 10], EVERY * 8);
        let accents: Vec<f32> = rises(&out, 3).iter().map(|&i| out[i][2]).collect();
        assert_eq!(accents, [10.0, 6.0, 6.0, 6.0, 10.0, 6.0, 6.0, 6.0]);
        let ratchet = Knobs {
            ratchet: 1.0,
            ..Knobs::default()
        };
        let out = clocked(ratchet, 1, &[0, 4, 7, 10], EVERY * 5);
        let accents: Vec<f32> = rises(&out, 3).iter().map(|&i| out[i][2]).collect();
        assert_eq!(accents[1..5], [6.0, 4.0, 6.0, 4.0]);
    }

    #[test]
    fn random_is_seeded_and_stays_inside_the_chord() {
        let knobs = Knobs {
            pattern: 4,
            ..Knobs::default()
        };
        let chord = [0, 4, 7, 10];
        let a = clocked(knobs, 9, &chord, EVERY * 200);
        assert_eq!(a, clocked(knobs, 9, &chord, EVERY * 200), "same seed");
        let b = clocked(knobs, 10, &chord, EVERY * 200);
        assert_ne!(notes(&a), notes(&b), "other seed");
        let played = notes(&a);
        assert!(played.iter().all(|n| chord.contains(n)));
        for note in chord {
            assert!(played.contains(&note), "never played {note}");
        }
        // Not just one fixed order repeated.
        assert!(played.windows(2).any(|w| w[0] == w[1]) || played[..4] != played[4..8]);
    }

    #[test]
    fn free_run_follows_tempo_and_division() {
        let free = |knobs, tempo, samples| {
            render(knobs, 1, tempo, samples, |_| frame(None, None, &[0, 7]))
        };
        // A sixteenth at 120 BPM is 0.125 s.
        assert_eq!(
            rises(&free(Knobs::default(), 120.0, 30_000), 3),
            [0, 6000, 12_000, 18_000, 24_000]
        );
        assert_eq!(
            rises(&free(Knobs::default(), 60.0, 30_000), 3),
            [0, 12_000, 24_000]
        );
        // A quarter note at 120 BPM is 0.5 s.
        let quarter = Knobs {
            rate: 1.0 / 8.0,
            ..Knobs::default()
        };
        assert_eq!(rises(&free(quarter, 120.0, 50_000), 3), [0, 24_000, 48_000]);
        assert_eq!(notes(&free(quarter, 120.0, 50_000)), [0, 7, 0]);
        // A clock, once patched, takes over from the free-running one.
        let out = clocked(Knobs::default(), 1, &[0, 7], 5000);
        assert_eq!(rises(&out, 3), [0, 1000, 2000, 3000, 4000]);
    }

    #[test]
    fn the_gate_input_holds_and_restarts_the_pattern() {
        let chord = [0, 4, 7, 10];
        let out = render(Knobs::default(), 1, 120.0, 40_000, |i| {
            let held = (3000..20_000).contains(&i) || i >= 30_000;
            frame(None, Some(held), &chord)
        });
        let starts = rises(&out, 3);
        assert_eq!(starts, [3000, 9000, 15_000, 30_000, 36_000]);
        // Each start is the first note of the pattern.
        assert_eq!(notes(&out), [0, 4, 7, 0, 4]);
        // Silent before the first rise; the gate drops the moment it falls.
        assert!(out[..3000].iter().all(|f| f[1] == 0.0 && f[3] == 0.0));
        assert!(
            out[20_000..30_000]
                .iter()
                .all(|f| f[1] == 0.0 && f[3] == 0.0)
        );
    }

    #[test]
    fn gates_never_run_into_the_next_note() {
        let chord = [0, 4, 7, 10];
        for pattern in [0, 2, 4, 7] {
            for swing in [0.0, 0.5, 1.0] {
                for ratchet in [0.0, 0.5, 1.0] {
                    for length in [0.0, 0.5, 1.0] {
                        let knobs = Knobs {
                            pattern,
                            swing,
                            ratchet,
                            length,
                            ..Knobs::default()
                        };
                        let out = clocked(knobs, 5, &chord, EVERY * 40);
                        let tag = format!(
                            "pattern {pattern} swing {swing} ratchet {ratchet} length {length}"
                        );
                        for f in &out {
                            assert!(f[1] == 0.0 || f[1] == 10.0, "{tag}");
                            assert!(f[3] == 0.0 || f[3] == 10.0, "{tag}");
                        }
                        // One separate gate per note, starting with its trigger.
                        let trigs = rises(&out, 3);
                        let gates = runs(&out, 1);
                        assert_eq!(trigs.len(), gates.len(), "{tag}");
                        for (&t, &(g, len)) in trigs.iter().zip(&gates) {
                            assert!(g == t || g == t + 1, "{tag}: trig {t} gate {g}");
                            assert!(len >= 1, "{tag}");
                        }
                        // A gate ends before the next note begins.
                        for pair in gates.windows(2) {
                            assert!(pair[0].0 + pair[0].1 < pair[1].0, "{tag}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn an_early_clock_cuts_a_gate_instead_of_merging_it() {
        // Pulses 1000 apart, then suddenly 200 apart, with the longest gates.
        let knobs = Knobs {
            length: 1.0,
            ..Knobs::default()
        };
        let out = render(knobs, 1, 120.0, 3000, |i| {
            let pulse = [0, 1000, 1500, 1800, 2100, 2400]
                .iter()
                .any(|&start| (start..start + 30).contains(&i));
            let (mut inputs, connected) = frame(Some(0), None, &[0]);
            inputs[0] = if pulse { 10.0 } else { 0.0 };
            (inputs, connected)
        });
        assert_eq!(rises(&out, 1).len(), rises(&out, 3).len());
    }

    #[test]
    fn same_settings_give_the_same_samples() {
        let knobs = Knobs {
            pattern: 4,
            swing: 0.3,
            ratchet: 0.4,
            ..Knobs::default()
        };
        let a = render(knobs, 77, 133.0, 60_000, |_| frame(None, None, &[0, 3, 7]));
        let b = render(knobs, 77, 133.0, 60_000, |_| frame(None, None, &[0, 3, 7]));
        assert_eq!(a, b);
    }

    #[test]
    fn extreme_settings_stay_finite_and_tidy() {
        let wild = [10.0, -10.0, 1e30, -1e30];
        for pattern in 0..8 {
            for octaves in [1, 4] {
                for rate in [0.0, 1.0] {
                    for (swing, ratchet, length) in [(0.0, 0.0, 0.0), (1.0, 1.0, 1.0)] {
                        for tempo in [0.0, 1.0, 120.0, 1000.0, 1e9] {
                            for external in [false, true] {
                                let knobs = Knobs {
                                    pattern,
                                    octaves,
                                    rate,
                                    length,
                                    swing,
                                    ratchet,
                                };
                                let out = render(knobs, 2, tempo, 3000, |i| {
                                    let (mut inputs, connected) =
                                        frame(external.then_some(i), Some(true), &[0; 4]);
                                    inputs[1] = 10.0;
                                    for (k, w) in wild.iter().enumerate() {
                                        inputs[2 + k] = if i % 7 == 0 { *w } else { 0.5 };
                                    }
                                    (inputs, connected)
                                });
                                for f in out {
                                    assert!(f.iter().all(|v| v.is_finite()), "{f:?}");
                                    assert!(f[1] == 0.0 || f[1] == 10.0);
                                    assert!(f[3] == 0.0 || f[3] == 10.0);
                                    assert!((0.0..=10.0).contains(&f[2]));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn descriptor_voices_follow_the_input_order_the_code_reads() {
        let keys: Vec<_> = DESCRIPTOR.inputs.iter().map(|j| j.key).collect();
        assert_eq!(
            keys,
            ["clock", "gate", "voice1", "voice2", "voice3", "voice4"]
        );
        let keys: Vec<_> = DESCRIPTOR.outputs.iter().map(|j| j.key).collect();
        assert_eq!(keys, ["pitch", "gate", "accent", "trig"]);
    }
}
