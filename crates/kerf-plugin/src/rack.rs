//! The audio-thread side: takes built engines from the loader, runs the current one, follows
//! the host's transport and tempo, and cross-fades between engines. No allocation, locks or I/O
//! in `render`; retired engines go back through a queue to be dropped on another thread.

use kerf_core::Patch;
use kerf_engine::{Change, Engine, ParamSlot};
use rtrb::{Consumer, Producer};

/// Frames per control update. Host parameter changes reach the engine this often.
pub const CHUNK: usize = 32;

/// Length of the fade in, out and between engines.
const FADE_SECONDS: f32 = 0.025;

/// Where the host-facing controls live in an engine: the first `Jig`'s macros and the first
/// `Finish`'s volume. `None` when the patch has no such module.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Targets {
    pub macros: [Option<ParamSlot>; 4],
    pub level: Option<ParamSlot>,
}

impl Targets {
    pub fn find(engine: &Engine, patch: &Patch) -> Self {
        let jig = patch.modules.iter().find(|m| m.kind == "Jig");
        let finish = patch.modules.iter().find(|m| m.kind == "Finish");
        let slot = |m: Option<&kerf_core::Module>, key: &str| {
            m.and_then(|m| engine.slot(m.id.as_str(), key).ok())
        };
        Self {
            macros: ["m1", "m2", "m3", "m4"].map(|k| slot(jig, k)),
            level: slot(finish, "volume"),
        }
    }
}

/// An engine built off the audio thread, with where its host-facing controls are.
pub struct Built {
    pub engine: Box<Engine>,
    pub targets: Targets,
    pub sample_rate: f32,
}

/// What the host says each block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Host {
    pub playing: bool,
    pub tempo: Option<f64>,
}

/// The plugin's parameters, plain values for one stretch of frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Controls {
    pub macros: [f32; 4],
    pub level: f32,
    pub follow_tempo: bool,
    pub follow_transport: bool,
}

impl Controls {
    /// The four macros, then the level.
    fn values(&self) -> [f32; 5] {
        let [m1, m2, m3, m4] = self.macros;
        [m1, m2, m3, m4, self.level]
    }
}

/// What `render` found out, for the caller to act on after the block.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Report {
    /// An engine went to the retired queue: ask for it to be dropped.
    pub retired: bool,
    /// The patch should be built afresh (it was stopped, or an engine was out of date).
    pub rearm: bool,
    /// The patch is producing sound (the transport is running).
    pub running: bool,
    /// Largest absolute output sample.
    pub peak: f32,
}

impl Report {
    pub fn merge(&mut self, other: Report) {
        self.retired |= other.retired;
        self.rearm |= other.rearm;
        self.running = other.running;
        self.peak = self.peak.max(other.peak);
    }
}

struct Slot {
    engine: Box<Engine>,
    targets: Targets,
    patch_tempo: f32,
    /// Last value sent per control (4 macros, then level); the host's values when installed.
    sent: [f32; 5],
    sent_tempo: f32,
}

pub struct Rack {
    inbox: Consumer<Built>,
    retired: Producer<Box<Engine>>,
    sample_rate: f32,
    current: Option<Slot>,
    /// The engine fading out while `fade_pos < fade_len`.
    fading: Option<Box<Engine>>,
    fade_len: usize,
    fade_pos: usize,
    running: bool,
    /// Whether the current engine has produced sound since it was built.
    dirty: bool,
    report: Report,
}

impl Rack {
    pub fn new(inbox: Consumer<Built>, retired: Producer<Box<Engine>>) -> Self {
        let mut rack = Self {
            inbox,
            retired,
            sample_rate: 48_000.0,
            current: None,
            fading: None,
            fade_len: 1,
            fade_pos: 1,
            running: false,
            dirty: false,
            report: Report::default(),
        };
        rack.set_sample_rate(48_000.0);
        rack
    }

    /// Forgets the running engine and the engines waiting in the inbox (a loader that ran several
    /// times before audio started would otherwise fill it). Not for the audio thread (called
    /// from `initialize`).
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.fade_len = ((sample_rate * FADE_SECONDS) as usize).max(1);
        self.fade_pos = self.fade_len;
        let old = self.current.take().map(|s| s.engine);
        self.retire(old);
        let fading = self.fading.take();
        self.retire(fading);
        while let Ok(built) = self.inbox.pop() {
            self.retire(Some(built.engine));
            // It may have been the current patch: ask for another.
            self.report.rearm = true;
        }
        self.running = false;
        self.dirty = false;
    }

    /// The tempo the current engine runs at.
    pub fn tempo(&self) -> Option<f32> {
        self.current.as_ref().map(|s| s.engine.tempo())
    }

    /// Asks for a fresh engine if this one has played (the host called `reset`).
    pub fn reset(&mut self) {
        if self.dirty {
            self.report.rearm = true;
        }
    }

    /// Fills `left` and `right` (equal length) with the next audio. No allocation.
    pub fn render(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        host: Host,
        controls: &Controls,
    ) -> Report {
        debug_assert_eq!(left.len(), right.len());
        let want = host.playing || !controls.follow_transport;
        if self.running && !want {
            let old = self.current.take().map(|s| s.engine);
            self.start_fade(old);
            self.report.rearm = true;
            self.dirty = false;
        }
        self.running = want;
        while let Ok(built) = self.inbox.pop() {
            self.install(built, controls);
        }
        if let Some(slot) = self.current.as_mut().filter(|_| self.running) {
            slot.update(host, controls);
        }

        let mut peak = 0.0f32;
        let mut buf = [0.0f32; 2 * CHUNK];
        let mut at = 0;
        while at < left.len() {
            let n = (left.len() - at).min(CHUNK);
            self.fill(&mut buf[..2 * n]);
            for (i, frame) in buf[..2 * n].chunks_exact_mut(2).enumerate() {
                // Never let a runaway patch reach the speakers.
                for x in frame.iter_mut() {
                    *x = if x.is_finite() {
                        x.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                }
                left[at + i] = frame[0];
                right[at + i] = frame[1];
                peak = peak.max(frame[0].abs()).max(frame[1].abs());
            }
            at += n;
        }

        let mut report = std::mem::take(&mut self.report);
        report.running = self.running;
        report.peak = peak;
        report
    }

    fn fill(&mut self, out: &mut [f32]) {
        let mut done = 0;
        while self.fade_pos < self.fade_len && done < out.len() {
            let t = self.fade_pos as f32 / self.fade_len as f32;
            let new = match self.current.as_mut() {
                Some(s) if self.running => {
                    self.dirty = true;
                    s.engine.next_frame()
                }
                _ => [0.0; 2],
            };
            let old = self.fading.as_mut().map_or([0.0; 2], |e| e.next_frame());
            out[done] = new[0] * t + old[0] * (1.0 - t);
            out[done + 1] = new[1] * t + old[1] * (1.0 - t);
            done += 2;
            self.fade_pos += 1;
            if self.fade_pos == self.fade_len {
                let faded = self.fading.take();
                self.retire(faded);
            }
        }
        match self.current.as_mut() {
            Some(s) if self.running => {
                self.dirty = true;
                s.engine.process_block(&mut out[done..]);
            }
            _ => out[done..].fill(0.0),
        }
    }

    /// Makes `built` the current engine. The host's controls as they stand are taken as already
    /// sent, so a new patch sounds as its author left it until the host moves a control.
    fn install(&mut self, built: Built, controls: &Controls) {
        if built.sample_rate != self.sample_rate {
            self.retire(Some(built.engine));
            self.report.rearm = true;
            return;
        }
        let patch_tempo = built.engine.tempo();
        let new = Slot {
            engine: built.engine,
            targets: built.targets,
            patch_tempo,
            sent: controls.values(),
            sent_tempo: f32::NAN,
        };
        let old = self.current.replace(new).map(|s| s.engine);
        self.dirty = false;
        if self.running {
            self.start_fade(old);
        } else {
            self.retire(old);
        }
    }

    /// Fades `old` (if there is one) out while `current` fades in.
    fn start_fade(&mut self, old: Option<Box<Engine>>) {
        let superseded = self.fading.take();
        self.retire(superseded);
        self.fading = old;
        self.fade_pos = 0;
    }

    fn retire(&mut self, engine: Option<Box<Engine>>) {
        if let Some(engine) = engine {
            self.report.retired = true;
            // A full queue (nothing drains it) drops the engine here instead.
            let _ = self.retired.push(engine);
        }
    }
}

impl Slot {
    fn update(&mut self, host: Host, controls: &Controls) {
        let values = controls.values();
        let slots = [
            self.targets.macros[0],
            self.targets.macros[1],
            self.targets.macros[2],
            self.targets.macros[3],
            self.targets.level,
        ];
        for ((sent, slot), value) in self.sent.iter_mut().zip(slots).zip(values) {
            if let Some(slot) = slot
                && *sent != value
            {
                self.engine.apply(Change::Param(slot, value));
                *sent = value;
            }
        }
        let tempo = match host.tempo {
            Some(bpm) if controls.follow_tempo && bpm.is_finite() => {
                (bpm as f32).clamp(*Patch::TEMPO_RANGE.start(), *Patch::TEMPO_RANGE.end())
            }
            _ => self.patch_tempo,
        };
        if self.sent_tempo != tempo {
            self.engine.apply(Change::Tempo(tempo));
            self.sent_tempo = tempo;
        }
    }
}
