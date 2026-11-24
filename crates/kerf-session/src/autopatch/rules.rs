//! What each kind of input wants, and how well each output suits it.
//!
//! Scores are comparable only within one input. The numbers encode taste:
//! a source in the same part beats one elsewhere, a source nobody uses yet
//! beats a shared one, and a rhythm lane beats the bare clock for a drum.

use kerf_core::{Band, Catalogue, JackDescriptor, JackKind, Module, ModuleDescriptor, ModuleId};

use super::Planner;

/// What an input is for. Inputs with no `Need` are never wired automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Need {
    Clock,
    Pitch,
    /// A trigger for a struck or sampled sound: a rhythm lane suits it.
    DrumGate,
    /// A gate that plays notes: the sequencer's gate suits it.
    MelodicGate,
    Audio,
    Env,
    Motion,
}

/// Jacks that are deliberate choices, not defaults: starting and resetting
/// clocks, transposing, hard sync, freezing, FM, side-chains, 1V/oct
/// tracking on a filter, and the gate and extra chord voices of a chord or
/// arpeggio module.
const SKIP: [(&str, &str); 17] = [
    ("Tally", "run"),
    ("Tally", "reset"),
    ("Dovetail", "reset"),
    ("Dovetail", "transpose"),
    ("Inlay", "reset"),
    ("Inlay", "transpose"),
    ("Spokeshave", "sync"),
    ("Shavings", "freeze"),
    ("Burl", "trig"),
    ("Chisel", "fm"),
    ("Vise", "side"),
    ("Mitre", "gate"),
    ("Rung", "gate"),
    ("Rung", "voice2"),
    ("Rung", "voice3"),
    ("Rung", "voice4"),
    ("Plane", "voct"),
];

pub(super) fn need(d: &ModuleDescriptor, j: &JackDescriptor) -> Option<Need> {
    if SKIP.contains(&(d.kind, j.key)) {
        return None;
    }
    match (d.kind, j.key) {
        ("Clamp", "cv") => Some(Need::Env),
        ("Plane", "cutoff") | ("Lathe", "turn") | ("Chisel", "pw") => Some(Need::Motion),
        ("Mortise", "trig") | ("Veneer", "trig") | ("Splinter", "trig") => Some(Need::DrumGate),
        _ => match j.kind {
            JackKind::Clock => Some(Need::Clock),
            JackKind::Pitch => Some(Need::Pitch),
            JackKind::Gate => Some(Need::MelodicGate),
            JackKind::Audio => Some(Need::Audio),
            JackKind::Cv => None,
        },
    }
}

impl<C: Catalogue> Planner<'_, C> {
    /// How well output `out` (number `idx`) of module `s` suits input `jack`
    /// of module `t`; `None` when it does not suit at all.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn score(
        &self,
        s: &Module,
        sd: &ModuleDescriptor,
        idx: usize,
        out: &JackDescriptor,
        t: &Module,
        td: &ModuleDescriptor,
        need: Need,
    ) -> Option<(f32, String)> {
        let same = s.part.is_some() && s.part == t.part;
        let bonus = if same { 2.0 } else { 0.0 };
        let used = self.used(&s.id, out.key);
        let unused = if used { 0.0 } else { 1.0 };
        let lane = |base: f32| {
            // A rhythm lane that is switched off (no hits) plays nothing.
            let live = s
                .params
                .iter()
                .find(|p| p.key.as_str() == format!("hits_{}", out.key))
                .is_some_and(|p| p.value > 0.0);
            (sd.kind == "Ruler" && out.kind == JackKind::Gate && live && !used)
                .then_some(base + bonus - 0.1 * idx as f32)
        };
        let reason = |text: &str| text.to_owned();
        match need {
            Need::Clock => (out.kind == JackKind::Clock).then(|| {
                let score = if out.key == "clock" { 10.0 } else { 4.0 };
                (score, reason("the clock keeps it in time"))
            }),
            Need::Pitch => {
                let ok = !used || same;
                match (sd.kind, out.key) {
                    ("Fret", "pitch") if td.kind != "Fret" && ok && self.fed(&s.id, "pitch") => {
                        Some((9.0 + bonus + unused, reason("the quantiser keeps it in key")))
                    }
                    ("Dovetail" | "Inlay", "pitch") if ok => {
                        Some((8.0 + bonus + unused, reason("the sequencer plays the notes")))
                    }
                    _ => None,
                }
            }
            Need::DrumGate => lane(9.0)
                .map(|s| (s, reason("a rhythm lane triggers the drum")))
                .or_else(|| self.clock_gate(sd, out, 3.0, "the clock triggers it every step")),
            Need::MelodicGate => match (sd.kind, out.key) {
                ("Dovetail" | "Inlay", "gate") if !used || same => {
                    Some((9.0 + bonus + unused, reason("the sequencer's gate plays the note")))
                }
                _ => lane(6.0)
                    .map(|s| (s, reason("a rhythm lane plays the note")))
                    .or_else(|| self.clock_gate(sd, out, 3.0, "the clock plays it every step")),
            },
            Need::Audio => self.audio_score(s, sd, idx, out, t, td, same),
            Need::Env => (sd.kind == "Bevel" && out.key == "env" && (!used || same))
                .then(|| (8.0 + bonus, reason("the envelope sets the level"))),
            Need::Motion => {
                let lfo = sd.kind == "Spokeshave" && out.key.starts_with("lfo");
                let drift = sd.kind == "Burl" && out.key == "drift";
                if used || !(lfo || drift) {
                    return None;
                }
                Some(if lfo {
                    (7.0 - 0.1 * idx as f32, reason("a slow LFO moves it"))
                } else {
                    (6.0, reason("slow random drift moves it"))
                })
            }
        }
    }

    /// The bare clock as a gate: fires every step, so only a fallback.
    fn clock_gate(
        &self,
        sd: &ModuleDescriptor,
        out: &JackDescriptor,
        score: f32,
        reason: &str,
    ) -> Option<(f32, String)> {
        (sd.kind == "Tally" && out.key == "clock").then(|| (score, reason.to_owned()))
    }

    #[expect(clippy::too_many_arguments, reason = "one candidate pair, scored in place")]
    fn audio_score(
        &self,
        s: &Module,
        sd: &ModuleDescriptor,
        idx: usize,
        out: &JackDescriptor,
        t: &Module,
        td: &ModuleDescriptor,
        same: bool,
    ) -> Option<(f32, String)> {
        if out.kind != JackKind::Audio || matches!(sd.kind, "Finish") {
            return None;
        }
        if sd.kind == "Router" {
            // The send bus is for the Space band; the mixer outs are not.
            let ok = out.key == "send" && td.band == Band::Space && !self.used(&s.id, "send");
            let score = if td.kind == "Kiln" { 12.0 } else { 4.0 };
            return ok.then(|| (score, "the send bus feeds the effect".to_owned()));
        }
        if self.feeds_processor(&s.id, sd) {
            return None;
        }
        let alone = !self.any_used(&s.id, sd.outputs.iter());
        let score = 5.0 + if same { 3.0 } else { 0.0 } + if alone { 1.0 } else { 0.0 } - 0.01 * idx as f32;
        Some((score, format!("the {} goes into the {}", s.kind, t.kind)))
    }

    /// True when one of the module's outputs already goes into an effect or
    /// filter's audio input: it is spoken for, and the next module should
    /// take the end of the chain instead.
    fn feeds_processor(&self, id: &ModuleId, sd: &ModuleDescriptor) -> bool {
        self.patch.cables.iter().any(|c| {
            &c.from.module == id
                && sd.output(c.from.jack.as_str()).is_some_and(|o| o.kind == JackKind::Audio)
                && self.desc(&c.to.module).is_some_and(|d| {
                    !matches!(d.kind, "Router" | "Finish")
                        && d.input(c.to.jack.as_str()).is_some_and(|j| j.kind == JackKind::Audio)
                })
        })
    }
}
