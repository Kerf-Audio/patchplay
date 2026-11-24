//! The built-in module catalogue. One file per module; each holds the
//! module's descriptor (params, jacks, tooltips) and, from phase 2, its DSP.

use std::sync::Arc;

use kerf_core::{Catalogue, ModuleDescriptor};

pub use veneer::{SLICES as VENEER_SLICES, demo_sample};

mod bandsaw;
mod bevel;
mod burl;
mod chatter;
mod chisel;
mod clamp;
mod dado;
mod delay_line;
#[cfg(test)]
mod depth_golden;
mod dovetail;
mod finish;
mod fret;
mod inlay;
mod jig;
mod kiln;
mod lathe;
mod level;
mod mallet;
mod mitre;
mod mortise;
mod plane;
mod ply;
mod rasp;
#[cfg(test)]
mod rig;
mod rocker;
mod router;
mod ruler;
mod rung;
mod sawdust;
mod shavings;
mod siren;
mod splice;
mod splinter;
mod spokeshave;
mod static_radio;
mod tally;
mod tread;
mod veneer;
mod vise;
mod warp;

/// Every built-in module, in catalogue order (band, then panel order).
static ALL: &[&ModuleDescriptor] = &[
    &tally::DESCRIPTOR,
    &dovetail::DESCRIPTOR,
    &inlay::DESCRIPTOR,
    &ruler::DESCRIPTOR,
    &fret::DESCRIPTOR,
    &mitre::DESCRIPTOR,
    &rung::DESCRIPTOR,
    &spokeshave::DESCRIPTOR,
    &burl::DESCRIPTOR,
    &bevel::DESCRIPTOR,
    &level::DESCRIPTOR,
    &jig::DESCRIPTOR,
    &rocker::DESCRIPTOR,
    &tread::DESCRIPTOR,
    &warp::DESCRIPTOR,
    &chatter::DESCRIPTOR,
    &chisel::DESCRIPTOR,
    &siren::DESCRIPTOR,
    &lathe::DESCRIPTOR,
    &ply::DESCRIPTOR,
    &mallet::DESCRIPTOR,
    &mortise::DESCRIPTOR,
    &veneer::DESCRIPTOR,
    &splinter::DESCRIPTOR,
    &sawdust::DESCRIPTOR,
    &static_radio::DESCRIPTOR,
    &plane::DESCRIPTOR,
    &clamp::DESCRIPTOR,
    &rasp::DESCRIPTOR,
    &vise::DESCRIPTOR,
    &splice::DESCRIPTOR,
    &bandsaw::DESCRIPTOR,
    &dado::DESCRIPTOR,
    &shavings::DESCRIPTOR,
    &kiln::DESCRIPTOR,
    &router::DESCRIPTOR,
    &finish::DESCRIPTOR,
];

/// The built-in catalogue.
#[derive(Debug, Clone, Copy, Default)]
pub struct Builtin;

impl Catalogue for Builtin {
    fn get(&self, kind: &str) -> Option<&'static ModuleDescriptor> {
        ALL.iter()
            .copied()
            .find(|d| d.kind.eq_ignore_ascii_case(kind))
    }

    fn all(&self) -> &'static [&'static ModuleDescriptor] {
        ALL
    }
}

/// Signals for one module for one sample. Slices follow descriptor order.
pub struct Io<'a> {
    pub sample_rate: f32,
    /// Patch tempo in BPM.
    pub tempo: f32,
    pub params: &'a [f32],
    /// Each input is the sum of the cables into it, 0 V when unpatched.
    pub inputs: &'a [f32],
    pub connected: &'a [bool],
    pub outputs: &'a mut [f32],
}

/// A running module instance. `process` runs once per sample on the audio
/// thread: no allocation, locks, I/O or logging.
pub trait Dsp: Send {
    fn process(&mut self, io: &mut Io<'_>);

    /// The stereo frame this module sends to the speakers, in `-1..=1`.
    /// Only output modules (Finish) return one.
    fn master(&self) -> Option<[f32; 2]> {
        None
    }
}

/// The knob positions that mean something: `0..=count` from `0..=1`.
pub(crate) fn steps(knob: f32, count: usize) -> usize {
    ((knob.clamp(0.0, 1.0) * count as f32).round() as usize).min(count)
}

/// Creates the DSP for a module kind. `None` for kinds whose DSP is not
/// written yet; the engine keeps them silent and reports them.
pub fn instantiate(kind: &str, seed: u64, sample_rate: f32) -> Option<Box<dyn Dsp>> {
    instantiate_with(kind, seed, sample_rate, None)
}

/// Like [`instantiate`], with the audio a sample module plays: mono, at
/// `sample_rate`. Loading files is the caller's job, so this crate does no
/// I/O. Other kinds ignore it; without it a Veneer or Splinter plays [`demo_sample`].
pub fn instantiate_with(
    kind: &str,
    seed: u64,
    sample_rate: f32,
    sample: Option<Arc<[f32]>>,
) -> Option<Box<dyn Dsp>> {
    let kind = Builtin.get(kind)?.kind;
    Some(match kind {
        "Tally" => Box::new(tally::State::default()),
        "Dovetail" => Box::new(dovetail::State::new(seed, sample_rate)),
        "Inlay" => Box::new(inlay::State::default()),
        "Ruler" => Box::new(ruler::State::new(seed)),
        "Mortise" => Box::new(mortise::State::new(seed)),
        "Chisel" => Box::new(chisel::State::default()),
        "Siren" => Box::new(siren::State::new(seed, sample_rate)),
        "Plane" => Box::new(plane::State::default()),
        "Clamp" => Box::new(clamp::State::default()),
        "Bevel" => Box::new(bevel::State::default()),
        "Router" => Box::new(router::State),
        "Finish" => Box::new(finish::State::default()),
        "Fret" => Box::new(fret::State::default()),
        "Mitre" => Box::new(mitre::State::default()),
        "Rung" => Box::new(rung::State::new(seed)),
        "Spokeshave" => Box::new(spokeshave::State::default()),
        "Burl" => Box::new(burl::State::new(seed)),
        "Level" => Box::new(level::State::default()),
        "Jig" => Box::new(jig::State::new(sample_rate)),
        "Rocker" => Box::new(rocker::State::new(seed, sample_rate)),
        "Tread" => Box::new(tread::State::new(seed)),
        "Warp" => Box::new(warp::State::new(seed)),
        "Chatter" => Box::new(chatter::State::new(seed)),
        "Rasp" => Box::new(rasp::State::default()),
        "Vise" => Box::new(vise::State::default()),
        "Bandsaw" => Box::new(bandsaw::State::new(seed, sample_rate)),
        "Shavings" => Box::new(shavings::State::new(seed, sample_rate)),
        "Kiln" => Box::new(kiln::State::new(sample_rate)),
        "Lathe" => Box::new(lathe::State::new()),
        "Ply" => Box::new(ply::State::new(seed, sample_rate)),
        "Mallet" => Box::new(mallet::State::default()),
        "Sawdust" => Box::new(sawdust::State::new(seed, sample_rate)),
        "Static" => Box::new(static_radio::State::new(seed, sample_rate)),
        "Splice" => Box::new(splice::State::new(sample_rate)),
        "Dado" => Box::new(dado::State::new(seed, sample_rate)),
        "Veneer" => Box::new(veneer::State::new(
            sample.unwrap_or_else(|| veneer::demo_sample(sample_rate)),
        )),
        "Splinter" => Box::new(splinter::State::new(
            seed,
            sample.unwrap_or_else(|| veneer::demo_sample(sample_rate)),
        )),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use kerf_core::text::RESERVED_MODULE_KEYS;
    use kerf_core::{Band, Key};

    use super::*;

    #[test]
    fn descriptors_are_usable_by_the_text_format_and_the_gui() {
        let mut kinds = HashSet::new();
        for d in Builtin.all() {
            assert!(kinds.insert(d.kind), "duplicate kind {}", d.kind);
            assert!(!d.summary.is_empty() && d.hp > 0, "{}", d.kind);
            let mut params = HashSet::new();
            for p in d.params {
                assert!(
                    Key::new(p.key).is_ok(),
                    "{}.{} is not a valid key",
                    d.kind,
                    p.key
                );
                assert!(
                    !RESERVED_MODULE_KEYS.contains(&p.key),
                    "{}.{} is a reserved word in the text format",
                    d.kind,
                    p.key
                );
                assert!(params.insert(p.key), "{}.{} twice", d.kind, p.key);
                assert!(
                    (0.0..=1.0).contains(&p.default),
                    "{}.{} default",
                    d.kind,
                    p.key
                );
                assert!(
                    !p.tooltip.is_empty() && !p.label.is_empty(),
                    "{}.{}",
                    d.kind,
                    p.key
                );
            }
            for (side, jacks) in [("in", d.inputs), ("out", d.outputs)] {
                let mut keys = HashSet::new();
                for j in jacks {
                    assert!(
                        Key::new(j.key).is_ok(),
                        "{}.{} is not a valid key",
                        d.kind,
                        j.key
                    );
                    assert!(
                        !RESERVED_MODULE_KEYS.contains(&j.key),
                        "{}.{} is a reserved word in the text format",
                        d.kind,
                        j.key
                    );
                    assert!(keys.insert(j.key), "{} {side} {} twice", d.kind, j.key);
                    assert!(
                        !j.tooltip.is_empty() && !j.label.is_empty(),
                        "{}.{}",
                        d.kind,
                        j.key
                    );
                }
            }
        }
        for band in Band::ALL {
            assert!(
                Builtin.all().iter().any(|d| d.band == band),
                "{band:?} is empty"
            );
        }
    }

    #[test]
    fn lookup_ignores_case() {
        assert_eq!(Builtin.get("plane").map(|d| d.kind), Some("Plane"));
        assert!(Builtin.get("Planer").is_none());
    }

    /// Everything `d` sends out over one second of busy inputs, master frame included. Each
    /// input gets its own pulse train and sine; `grounded` is held at 0 V instead.
    fn heard(d: &ModuleDescriptor, grounded: Option<usize>) -> Vec<f32> {
        const RATE: f32 = 48_000.0;
        let mut dsp = instantiate(d.kind, 1, RATE).unwrap();
        // Mid-way, so an amount knob that rests at 0 does not hide a live jack.
        let params = vec![0.5; d.params.len()];
        let connected = vec![true; d.inputs.len()];
        let mut inputs = vec![0.0; d.inputs.len()];
        let mut outputs = vec![0.0; d.outputs.len()];
        let mut heard = Vec::new();
        for i in 0..RATE as usize {
            let t = i as f32 / RATE;
            for (k, v) in inputs.iter_mut().enumerate() {
                let pulse = if (5.0 * t + k as f32 * 0.13).fract() < 0.3 {
                    8.0
                } else {
                    0.0
                };
                let hum = 2.0 * (std::f32::consts::TAU * (110.0 + 77.0 * k as f32) * t).sin();
                *v = if grounded == Some(k) {
                    0.0
                } else {
                    pulse + hum
                };
            }
            dsp.process(&mut Io {
                sample_rate: RATE,
                tempo: 120.0,
                params: &params,
                inputs: &inputs,
                connected: &connected,
                outputs: &mut outputs,
            });
            heard.extend_from_slice(&outputs);
            heard.extend(dsp.master().unwrap_or_default());
        }
        heard
    }

    #[test]
    fn every_input_jack_changes_what_its_module_does() {
        for d in Builtin.all() {
            let all = heard(d, None);
            for (k, jack) in d.inputs.iter().enumerate() {
                assert!(
                    heard(d, Some(k)) != all,
                    "{}.{} does nothing",
                    d.kind,
                    jack.key
                );
            }
        }
    }
}
