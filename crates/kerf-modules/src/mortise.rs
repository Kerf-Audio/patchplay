use std::f32::consts::TAU;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, CV_MAX, Edge, Gate, OnePole, Phase, Rng, Svf, lowpass_coeff};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Mortise",
    band: Band::Voices,
    hp: 10,
    summary: "Drum voice",
    params: &[
        param("engine", "Engine", 0.0, "Kick, snare, hat, clap or tom."),
        param("pitch", "Pitch", 0.4, "Tuning of the drum."),
        param("punch", "Punch", 0.6, "Snap at the start of each hit."),
        param("decay", "Decay", 0.4, "Length of each hit."),
        param("tone", "Tone", 0.5, "Darker to brighter."),
    ],
    inputs: &[
        jack("trig", "Trig", Gate, "Plays a hit."),
        jack("accent", "Accent", Cv, "Makes a hit louder and brighter."),
    ],
    outputs: &[jack("out", "Out", Audio, "The drum.")],
};

/// Square-wave ratios for the hat's metallic cluster (after the TR-808).
const METAL: [f32; 6] = [1.0, 1.483, 1.800, 2.546, 2.630, 3.897];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Engine {
    #[default]
    Kick,
    Snare,
    Hat,
    Clap,
    Tom,
}

impl Engine {
    fn from_param(p: f32) -> Self {
        match (p * 4.0).round() as u8 {
            0 => Self::Kick,
            1 => Self::Snare,
            2 => Self::Hat,
            3 => Self::Clap,
            _ => Self::Tom,
        }
    }
}

/// Exponential decay that falls by 60 dB over the time given to `hit`.
#[derive(Debug, Clone, Copy, Default)]
struct Decay {
    value: f32,
    mul: f32,
}

impl Decay {
    fn hit(&mut self, seconds: f32, sample_rate: f32) {
        self.value = 1.0;
        self.mul = (-6.91 / (seconds * sample_rate)).exp();
    }

    fn next(&mut self) -> f32 {
        let v = self.value;
        self.value = if v < 1e-5 { 0.0 } else { v * self.mul };
        v
    }
}

pub struct State {
    trig: Gate,
    rng: Rng,
    engine: Engine,
    /// Level and brightness of the current hit.
    level: f32,
    tone: f32,
    /// Samples since the last hit.
    age: u32,
    amp: Decay,
    noise_amp: Decay,
    pitch_env: Decay,
    click: Decay,
    phases: [Phase; 6],
    filters: [Svf; 2],
    click_lp: OnePole,
}

impl State {
    pub fn new(seed: u64) -> Self {
        Self {
            trig: Gate::default(),
            rng: Rng::new(seed),
            engine: Engine::default(),
            level: 0.0,
            tone: 0.0,
            age: u32::MAX,
            amp: Decay::default(),
            noise_amp: Decay::default(),
            pitch_env: Decay::default(),
            click: Decay::default(),
            phases: [Phase::default(); 6],
            filters: [Svf::default(); 2],
            click_lp: OnePole::default(),
        }
    }

    fn noise(&mut self) -> f32 {
        self.rng.unit() * 2.0 - 1.0
    }

    fn hit(&mut self, engine: Engine, punch: f32, decay: f32, sr: f32) {
        self.engine = engine;
        self.age = 0;
        self.phases = [Phase::default(); 6];
        let (amp, noise, pitch) = match engine {
            Engine::Kick => (0.15 + decay * 1.2, 0.0, 0.02 + (1.0 - punch) * 0.04),
            Engine::Snare => (0.06 + decay * 0.25, 0.08 + decay * 0.5, 0.02),
            Engine::Hat => (0.03 + decay * decay * 1.2, 0.0, 0.005),
            Engine::Clap => (0.12 + decay * 0.6, 0.0, 0.0),
            Engine::Tom => (0.15 + decay * 0.9, 0.0, 0.08),
        };
        self.amp.hit(amp, sr);
        if noise > 0.0 {
            self.noise_amp.hit(noise, sr);
        }
        if pitch > 0.0 {
            self.pitch_env.hit(pitch, sr);
        }
        self.click.hit(0.004, sr);
    }

    fn kick(&mut self, pitch: f32, punch: f32, sr: f32) -> f32 {
        let base = 35.0 * (pitch * 1.5).exp2();
        let hz = base * (punch * 3.5 * self.pitch_env.next()).exp2();
        let body = (TAU * self.phases[0].advance(hz / sr)).sin() * self.amp.next();
        let n = self.noise();
        let click_hz = 1000.0 * (self.tone * 3.0).exp2();
        let click = self.click_lp.process(n, lowpass_coeff(click_hz, sr)) * self.click.next();
        let drive = 1.0 + self.tone * 2.0;
        (drive * (body + click * punch * 1.5)).tanh()
    }

    fn snare(&mut self, pitch: f32, punch: f32, sr: f32) -> f32 {
        let base = 150.0 * ((pitch - 0.5) * 1.5).exp2();
        let bend = 1.0 + punch * 0.6 * self.pitch_env.next();
        let a = (TAU * self.phases[0].advance(base * bend / sr)).sin();
        let b = (TAU * self.phases[1].advance(base * 1.6 * bend / sr)).sin();
        let body = (a + 0.6 * b) * 0.6 * self.amp.next();
        let n = self.noise();
        let cutoff = 800.0 * (self.tone * 2.5).exp2();
        let rattle = self.filters[0].process(n, cutoff, 0.7, sr).high * self.noise_amp.next();
        (1.5 * (body + rattle * (0.6 + punch * 0.5))).tanh()
    }

    fn hat(&mut self, pitch: f32, punch: f32, sr: f32) -> f32 {
        let scale = 300.0 * ((pitch - 0.5) * 2.0).exp2();
        let mut metal = 0.0;
        for (phase, ratio) in self.phases.iter_mut().zip(METAL) {
            metal += if phase.advance(scale * ratio / sr) < 0.5 {
                1.0
            } else {
                -1.0
            };
        }
        let mix = metal / 6.0 * 0.7 + self.noise() * 0.3;
        let band = self.filters[0].process(mix, 9000.0, 1.2, sr).band;
        let cutoff = 4000.0 * (self.tone * 1.5).exp2();
        let high = self.filters[1].process(band, cutoff, 0.7, sr).high;
        let env = self.amp.next() * (1.0 + punch * self.pitch_env.next());
        (3.0 * high * env).tanh()
    }

    fn clap(&mut self, punch: f32, sr: f32) -> f32 {
        let t = self.age as f32 / sr;
        let tail = self.amp.next();
        let env = if t < 0.033 {
            (-(t % 0.011) / 0.004).exp() * (0.6 + punch * 0.4)
        } else {
            tail * 0.7
        };
        let n = self.noise();
        let centre = 1000.0 * (self.tone * 1.5).exp2();
        let band = self.filters[0].process(n, centre, 1.5, sr).band;
        (3.0 * band * env).tanh()
    }

    fn tom(&mut self, pitch: f32, punch: f32, sr: f32) -> f32 {
        let base = 90.0 * (pitch * 2.0).exp2();
        let hz = base * (punch * 0.8 * self.pitch_env.next()).exp2();
        let body = (TAU * self.phases[0].advance(hz / sr)).sin() * self.amp.next();
        let click = self.noise() * self.click.next() * punch * 0.3;
        (1.5 * (body + click)).tanh()
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[engine, pitch, punch, decay, tone] = io.params else {
            return;
        };
        let sr = io.sample_rate;
        if self.trig.update(io.inputs[0]) == Edge::Rose {
            let accent = if io.connected[1] {
                (io.inputs[1] / CV_MAX).clamp(0.0, 1.0)
            } else {
                1.0
            };
            self.level = 0.5 + 0.5 * accent;
            self.tone = (tone + 0.3 * (accent - 1.0) + 0.15).clamp(0.0, 1.0);
            self.hit(Engine::from_param(engine), punch, decay, sr);
        }
        if self.age == u32::MAX {
            io.outputs[0] = 0.0;
            return;
        }
        let y = match self.engine {
            Engine::Kick => self.kick(pitch, punch, sr),
            Engine::Snare => self.snare(pitch, punch, sr),
            Engine::Hat => self.hat(pitch, punch, sr),
            Engine::Clap => self.clap(punch, sr),
            Engine::Tom => self.tom(pitch, punch, sr),
        };
        self.age = self.age.saturating_add(1);
        io.outputs[0] = y * self.level * AUDIO_VOLTS;
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;

    /// One hit; returns peak volts in the first 50 ms and in the last 50 ms
    /// of a 2 s render.
    fn hit(engine: f32) -> (f32, f32) {
        let params = [engine, 0.4, 0.6, 0.4, 0.5];
        let mut drum = State::new(1);
        let mut start = 0f32;
        let mut end = 0f32;
        let n = (2.0 * SR) as usize;
        for i in 0..n {
            let trig = if i < 240 { 10.0 } else { 0.0 };
            let mut out = [0.0];
            drum.process(&mut Io {
                sample_rate: SR,
                tempo: 120.0,
                params: &params,
                inputs: &[trig, 0.0],
                connected: &[true, false],
                outputs: &mut out,
            });
            assert!(out[0].is_finite());
            if i < 2400 {
                start = start.max(out[0].abs());
            } else if i >= n - 2400 {
                end = end.max(out[0].abs());
            }
        }
        (start, end)
    }

    #[test]
    fn every_engine_hits_hard_and_dies_away() {
        for (i, name) in ["kick", "snare", "hat", "clap", "tom"].iter().enumerate() {
            let (start, end) = hit(i as f32 / 4.0);
            assert!(start > 2.0 && start <= 5.0, "{name} peak {start} V");
            assert!(end < 0.01, "{name} still ringing at {end} V");
        }
    }
}
