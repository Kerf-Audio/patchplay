use std::f32::consts::{PI, TAU};

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{
    AUDIO_VOLTS, CV_MAX, GATE_HIGH, OnePole, Phase, Rng, Svf, flush, lerp, lowpass_coeff, smoothing,
};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Static",
    band: Band::Voices,
    hp: 8,
    summary: "Detuned radio",
    params: &[
        param(
            "tune",
            "Tune",
            0.4,
            "The dial. Whistles sweep as you turn it, and a faint station hides at three spots.",
        ),
        param(
            "width",
            "Width",
            0.4,
            "How wide the tuned band is. Narrow is a thin whistling hiss, wide is open air.",
        ),
        param("hiss", "Hiss", 0.5, "Level of the tuned hiss."),
        param(
            "crackle",
            "Crackle",
            0.3,
            "How often clicks and distant thunder pops land.",
        ),
        param(
            "whistle",
            "Whistle",
            0.4,
            "Level of the whistle you get when the dial is near a station.",
        ),
        param(
            "fade",
            "Fade",
            0.3,
            "Slow fading and drift of the signal. Fainter stations let the hiss in.",
        ),
        param(
            "ghost",
            "Ghost",
            0.3,
            "Level of the muffled station that surfaces when the dial sits right on one.",
        ),
        param("level", "Level", 0.5, "Output volume."),
    ],
    inputs: &[
        jack("tune", "Tune", Cv, "Moves Tune."),
        jack("fade", "Fade", Cv, "Moves Fade."),
    ],
    outputs: &[
        jack("out", "Out", Audio, "The radio."),
        jack(
            "crackle",
            "Crackle",
            Gate,
            "A short pulse for every click and pop.",
        ),
    ],
};

/// The tuned hiss follows the dial from here to there.
const DIAL_LOW_HZ: f32 = 150.0;
const DIAL_HIGH_HZ: f32 = 7_000.0;
/// Band-pass Q at no Width and at full Width.
const NARROW_Q: f32 = 14.0;
const WIDE_Q: f32 = 0.4;
/// Gain that evens out the loudness of noise through the narrow band is
/// capped at this.
const MAX_COMP: f32 = 10.0;
const STATIONS: usize = 3;
/// Whistle pitch per unit of dial distance from the nearest station.
const WHISTLE_HZ_PER_DIAL: f32 = 6_000.0;
/// Whistles are heard this far from a station on the dial.
const WHISTLE_REACH: f32 = 0.3;
/// Whistles below this pitch fade out towards zero beat.
const WHISTLE_FADE_HZ: f32 = 100.0;
const WHISTLE_LEVEL: f32 = 0.4;
/// The ghost is heard within this share of the dial at no Width, plus the
/// second term at full Width.
const GHOST_REACH: f32 = 0.015;
const GHOST_REACH_WIDE: f32 = 0.05;
const GHOST_LEVEL: f32 = 0.3;
const GHOST_PARTIALS: [f32; 4] = [1.0, 2.0, 3.01, 4.97];
const GHOST_LEVELS: [f32; 4] = [1.0, 0.5, 0.33, 0.2];
const GHOST_VIBRATO_HZ: [f32; 4] = [0.31, 0.47, 0.71, 1.1];
const GHOST_TREMOLO_HZ: f32 = 3.1;
const GHOST_MUFFLE_HZ: f32 = 2_500.0;
/// Fading is the sum of three slow sines with these rates.
const FADE_HZ: [f32; 3] = [0.11, 0.037, 0.43];
/// Dial drift at full Fade.
const DRIFT: f32 = 0.006;
/// RMS of the hiss at full Hiss, as a fraction of full scale.
const HISS_LEVEL: f32 = 0.6;
const DIAL_GLIDE_SECONDS: f32 = 0.02;
/// Clicks per second at full Crackle, and how steeply the knob raises it.
const MAX_CLICKS: f32 = 500.0;
const CLICK_CURVE: f32 = 2.5;
/// Distant lightning pops per second at full Crackle.
const MAX_SFERICS: f32 = 4.0;
const SFERIC_CURVE: f32 = 1.5;
const POP_SECONDS: f32 = 0.0004;
const SFERIC_THUMP_HZ: f32 = 900.0;
const TRIGGER_SECONDS: f32 = 0.001;

/// Dial positions and ghost pitches of the three stations, from the seed.
fn stations(seed: u64) -> ([f32; STATIONS], [f32; STATIONS]) {
    let mut rng = Rng::new(seed ^ 0x57A7_1075);
    let mut dial = [0.0; STATIONS];
    let mut pitch = [0.0; STATIONS];
    for (i, (d, p)) in dial.iter_mut().zip(&mut pitch).enumerate() {
        *d = 0.2 + 0.3 * i as f32 + (rng.unit() - 0.5) * 0.12;
        *p = 110.0 * (2.0f32).powf(rng.unit());
    }
    (dial, pitch)
}

/// Gain into the band-pass that makes a unit impulse peak at about 1 coming
/// out of it.
fn impulse_gain(centre: f32, q: f32, sample_rate: f32) -> f32 {
    let g = (PI * (centre / sample_rate).min(0.49)).tan();
    let k = 1.0 / q;
    (1.0 + g * (g + k)) / (k * g)
}

#[derive(Debug, Clone)]
pub struct State {
    noise: Rng,
    events: Rng,
    station_dial: [f32; STATIONS],
    station_pitch: [f32; STATIONS],
    tuned: Svf,
    /// Smoothed dial position; `None` until the first sample.
    dial: Option<f32>,
    pop: f32,
    pop_decay: f32,
    sferic: f32,
    sferic_decay: f32,
    thump: OnePole,
    trigger_left: u32,
    trigger_samples: u32,
    whistle: Phase,
    ghost: [Phase; 4],
    ghost_vibrato: [Phase; 4],
    ghost_tremolo: Phase,
    ghost_muffle: OnePole,
    ghost_muffle_coeff: f32,
    fade: [Phase; 3],
}

impl State {
    pub fn new(seed: u64, sample_rate: f32) -> Self {
        let (station_dial, station_pitch) = stations(seed);
        let mut events = Rng::new(seed ^ 0x00E7_E475);
        let mut phase = |count: usize| {
            let mut all = [Phase::default(); 4];
            for p in all.iter_mut().take(count) {
                p.advance(events.unit());
            }
            all
        };
        let ghost_vibrato = phase(4);
        let fade_phases = phase(3);
        Self {
            noise: Rng::new(seed),
            events,
            station_dial,
            station_pitch,
            tuned: Svf::default(),
            dial: None,
            pop: 0.0,
            pop_decay: (-1.0 / (POP_SECONDS * sample_rate)).exp(),
            sferic: 0.0,
            sferic_decay: 0.0,
            thump: OnePole::default(),
            trigger_left: 0,
            trigger_samples: (TRIGGER_SECONDS * sample_rate).ceil() as u32,
            whistle: Phase::default(),
            ghost: [Phase::default(); 4],
            ghost_vibrato,
            ghost_tremolo: Phase::default(),
            ghost_muffle: OnePole::default(),
            ghost_muffle_coeff: lowpass_coeff(GHOST_MUFFLE_HZ, sample_rate),
            fade: [fade_phases[0], fade_phases[1], fade_phases[2]],
        }
    }

    /// The station nearest `dial` and how far away it is.
    fn nearest(&self, dial: f32) -> (usize, f32) {
        (0..STATIONS)
            .map(|i| (i, (dial - self.station_dial[i]).abs()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((0, 1.0))
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[tune, width, hiss, crackle, whistle, fade, ghost, level] = io.params else {
            return;
        };
        let sr = io.sample_rate;

        let target = (tune + io.inputs[0] / CV_MAX).clamp(0.0, 1.0);
        let dial = self.dial.unwrap_or(target);
        let dial = dial + smoothing(DIAL_GLIDE_SECONDS, sr) * (target - dial);
        self.dial = Some(dial);
        let depth = (fade + io.inputs[1] / CV_MAX).clamp(0.0, 1.0);

        // Signal strength: 1 is a clear signal, 0 a fade-out.
        let [fast, slow, middle] =
            [0, 1, 2].map(|i| (TAU * self.fade[i].advance(FADE_HZ[i] / sr)).sin());
        let strength = 0.5 + 0.5 * (0.5 * fast + 0.3 * slow + 0.2 * middle);
        let signal_gain = lerp(1.0, strength, depth);
        // The receiver turns itself up when the signal drops, letting hiss in.
        let noise_gain = 1.0 + 0.5 * depth * (1.0 - strength);
        let dial = (dial + DRIFT * depth * slow).clamp(0.0, 1.0);

        // Clicks and pops: always draw, so a patch sounds the same wherever
        // Crackle sits.
        let click = self.events.unit() < MAX_CLICKS * crackle.powf(CLICK_CURVE) / sr;
        let sferic = self.events.unit() < MAX_SFERICS * crackle.powf(SFERIC_CURVE) / sr;
        let white = self.noise.unit() * 2.0 - 1.0;
        let rumble = self.noise.unit() * 2.0 - 1.0;

        let centre = DIAL_LOW_HZ * (DIAL_HIGH_HZ / DIAL_LOW_HZ).powf(dial);
        let q = NARROW_Q * (WIDE_Q / NARROW_Q).powf(width);
        let mut sent = 0.0;
        if click {
            let sign = if self.events.next_u64() & 1 == 0 {
                1.0
            } else {
                -1.0
            };
            let u = self.events.unit();
            let size = sign * (0.2 + 0.8 * u * u);
            self.pop = (self.pop + size).clamp(-1.0, 1.0);
            // Each click rings the tuned band for a moment.
            sent = size * impulse_gain(centre, q, sr);
        }
        if sferic {
            self.sferic = 0.4 + 0.6 * self.events.unit();
            let seconds = 0.006 + 0.014 * self.events.unit();
            self.sferic_decay = (-1.0 / (seconds * sr)).exp();
        }
        if click || sferic {
            self.trigger_left = self.trigger_samples;
        }

        // Noise through a narrow band carries less power: bring it back up.
        let comp = (0.5 * sr * q / (1.57 * centre)).sqrt().clamp(1.0, MAX_COMP);
        let band = self
            .tuned
            .process(sent + white * hiss * HISS_LEVEL * comp, centre, q, sr)
            .band
            / q;

        self.pop = flush(self.pop * self.pop_decay);
        let thump = self
            .thump
            .process(rumble, lowpass_coeff(SFERIC_THUMP_HZ, sr))
            * self.sferic
            * 3.0;
        self.sferic = flush(self.sferic * self.sferic_decay);
        let noisy = band + 0.25 * self.pop + thump;

        // Whistle: the beat between the dial and the nearest station.
        let (station, away) = self.nearest(dial);
        let beat = away * WHISTLE_HZ_PER_DIAL;
        let near = (1.0 - away / WHISTLE_REACH).max(0.0).powf(1.5);
        let zero_beat = (beat / WHISTLE_FADE_HZ).min(1.0);
        let whistling = (TAU * self.whistle.advance(beat / sr)).sin()
            * whistle
            * WHISTLE_LEVEL
            * near
            * zero_beat;

        // Ghost: a few drifting partials of a muffled station, tremoring
        // like a voice, strongest right on the station.
        let reach = GHOST_REACH + GHOST_REACH_WIDE * width;
        let on_station = (1.0 - away / reach).max(0.0).powi(2);
        let mut station_voice = 0.0;
        if ghost * on_station > 1e-4 {
            let base = self.station_pitch[station];
            let tremolo =
                0.65 + 0.35 * (TAU * self.ghost_tremolo.advance(GHOST_TREMOLO_HZ / sr)).sin();
            for k in 0..GHOST_PARTIALS.len() {
                let vibrato = 1.0
                    + 0.004 * (TAU * self.ghost_vibrato[k].advance(GHOST_VIBRATO_HZ[k] / sr)).sin();
                let hz = base * GHOST_PARTIALS[k] * vibrato;
                station_voice += GHOST_LEVELS[k] * (TAU * self.ghost[k].advance(hz / sr)).sin();
            }
            station_voice *= tremolo * 0.4;
        }
        let station_voice = self
            .ghost_muffle
            .process(station_voice, self.ghost_muffle_coeff)
            * ghost
            * on_station
            * GHOST_LEVEL;

        let mixed = noisy * noise_gain + (whistling + station_voice) * signal_gain;
        io.outputs[0] = (mixed * level * 2.0).tanh() * AUDIO_VOLTS;
        io.outputs[1] = if self.trigger_left > 0 {
            GATE_HIGH
        } else {
            0.0
        };
        self.trigger_left = self.trigger_left.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{CLICK_CURVE, MAX_CLICKS, MAX_SFERICS, SFERIC_CURVE, State, stations};
    use crate::rig::{SR, peak, render, rms, tone};

    /// Tune, Width, Hiss, Crackle, Whistle, Fade, Ghost, Level.
    type Params = [f32; 8];

    fn run(seed: u64, params: Params, seconds: f32) -> Vec<Vec<f32>> {
        render(
            &mut State::new(seed, SR),
            &params,
            &[false, false],
            2,
            (seconds * SR) as usize,
            |_, _| {},
        )
    }

    fn pulses(trigger: &[f32]) -> usize {
        trigger
            .windows(2)
            .filter(|w| w[0] < 5.0 && w[1] >= 5.0)
            .count()
    }

    /// Nothing but a whistle, at `tune`.
    fn whistle_only(tune: f32) -> Params {
        [tune, 0.4, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5]
    }

    #[test]
    fn crackle_density_sets_the_click_count() {
        let silent = run(1, [0.4, 0.4, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5], 2.0);
        assert_eq!(pulses(&silent[1]), 0, "pulses at zero crackle");
        let seconds = 10.0;
        let mut last = 0.0;
        for crackle in [0.3f32, 0.5, 0.8] {
            let out = run(1, [0.4, 0.4, 0.0, crackle, 0.0, 0.0, 0.0, 0.5], seconds);
            let found = pulses(&out[1]) as f32 / seconds;
            let expected =
                MAX_CLICKS * crackle.powf(CLICK_CURVE) + MAX_SFERICS * crackle.powf(SFERIC_CURVE);
            assert!(
                (0.7..1.2).contains(&(found / expected)),
                "crackle {crackle}: {found} events/s, expected {expected}"
            );
            assert!(found > 2.0 * last || last == 0.0 || crackle > 0.7);
            last = found;
            // The clicks are heard, not only counted.
            assert!(peak(&out[0]) > 1.0, "crackle {crackle} is silent");
        }
        let out = run(1, [0.4, 0.4, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5], 1.0);
        assert!(
            out[1].iter().all(|&v| v == 0.0 || v == 10.0),
            "trigger is not a gate"
        );
    }

    #[test]
    fn same_seed_same_static_and_other_seeds_differ() {
        let params = [0.45, 0.5, 0.6, 0.5, 0.5, 0.5, 0.5, 0.5];
        let a = run(7, params, 0.5);
        assert_eq!(a, run(7, params, 0.5));
        assert_ne!(a, run(8, params, 0.5));
        assert_ne!(stations(7), stations(8));
    }

    #[test]
    fn the_output_stays_inside_full_scale_at_every_extreme() {
        let mut dsp = State::new(3, SR);
        for params in [
            [1.0; 8],
            [0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            [0.5, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0],
        ] {
            let out = render(
                &mut dsp,
                &params,
                &[true, true],
                2,
                (3.0 * SR) as usize,
                |_, inp| {
                    inp.fill(10.0);
                },
            );
            assert!(peak(&out[0]) <= 5.0, "{params:?}");
            assert!(peak(&out[1]) <= 10.0);
        }
        let quiet = run(3, [0.4, 0.4, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5], 1.0);
        assert_eq!(peak(&quiet[0]), 0.0, "nothing switched on, nothing heard");
        let off = run(3, [0.4, 0.4, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0], 1.0);
        assert_eq!(peak(&off[0]), 0.0, "level 0 is silent");
    }

    #[test]
    fn the_whistle_follows_the_dial_away_from_a_station() {
        let (dial, _) = stations(1);
        let station = dial[1];
        let measure = |away: f32| {
            let out = run(1, whistle_only(station + away), 0.7);
            out[0][24_000..28_800].to_vec()
        };
        let expected = |away: f32| away * super::WHISTLE_HZ_PER_DIAL;
        let aways = [0.02, 0.04, 0.08];
        for &away in &aways {
            let w = measure(away);
            let found = tone(&w, expected(away));
            assert!(
                found > 1.0,
                "{away} from the station: {found} V at {} Hz",
                expected(away)
            );
            for &other in aways.iter().filter(|&&o| o != away) {
                assert!(
                    tone(&w, expected(other)) < 0.3,
                    "{away} also sounds like {other}"
                );
            }
        }
        // And on the other side of the station.
        let below = measure(-0.04);
        assert!(tone(&below, expected(0.04)) > 1.0);
        // Dead on the station there is no whistle left to hear.
        let zero_beat = run(1, whistle_only(station), 0.7);
        assert!(rms(&zero_beat[0][24_000..]) < 0.05);
    }

    #[test]
    fn tuning_moves_the_band_and_width_opens_it() {
        let hiss = |tune: f32, width: f32| {
            let out = run(1, [tune, width, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5], 2.0);
            out[0][SR as usize..].to_vec()
        };
        let crossings =
            |x: &[f32]| x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count() as f32;
        let low = crossings(&hiss(0.3, 0.0));
        let high = crossings(&hiss(0.6, 0.0));
        assert!(
            high > 2.0 * low,
            "tuning up moved the band from {low} to {high}"
        );
        let narrow = crossings(&hiss(0.6, 0.0));
        let wide = crossings(&hiss(0.6, 1.0));
        assert!(
            wide > 1.5 * narrow,
            "width made no difference: {narrow} then {wide}"
        );
    }

    #[test]
    fn fade_makes_the_signal_swell_and_sink() {
        let (dial, _) = stations(1);
        let swing = |fade: f32| {
            let mut params = whistle_only(dial[1] + 0.05);
            params[5] = fade;
            let out = run(1, params, 40.0);
            let loudness: Vec<f32> = out[0].chunks(4_800).map(rms).collect();
            let high = loudness.iter().copied().fold(0.0, f32::max);
            let low = loudness.iter().copied().fold(f32::MAX, f32::min);
            high / low
        };
        assert!(swing(0.0) < 1.1, "fade 0 still moves ({})", swing(0.0));
        assert!(swing(1.0) > 3.0, "fade 1 barely moves ({})", swing(1.0));
    }

    #[test]
    fn the_ghost_only_surfaces_on_a_station() {
        let (dial, _) = stations(2);
        let ghost = |tune: f32| {
            let out = run(2, [tune, 0.4, 0.0, 0.0, 0.0, 0.0, 1.0, 0.5], 1.0);
            rms(&out[0][12_000..])
        };
        assert!(ghost(dial[0]) > 0.1, "no ghost on the station");
        assert!(ghost(dial[2]) > 0.1);
        assert_eq!(ghost(dial[0] + 0.12), 0.0, "ghost off the station");
    }

    #[test]
    fn the_tune_jack_moves_the_dial() {
        let (dial, _) = stations(1);
        let params = [dial[1], 0.4, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5];
        let at = |volts: f32| {
            let out = render(
                &mut State::new(1, SR),
                &params,
                &[true, false],
                2,
                28_800,
                |_, inp| {
                    inp[0] = volts;
                },
            );
            tone(&out[0][24_000..], 0.04 * super::WHISTLE_HZ_PER_DIAL)
        };
        assert!(
            at(0.4) > 1.0,
            "0.04 up the dial from the station gives its whistle"
        );
        assert!(at(0.0) < 0.3);
    }
}
