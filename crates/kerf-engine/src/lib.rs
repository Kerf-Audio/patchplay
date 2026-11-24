//! The engine: compiles a `Patch` into a flat processing graph and runs it
//! one sample at a time, or a block at a time for live audio.
//!
//! Modules run in dependency order, so a signal reaches the end of a chain in
//! the same sample. Cables that close a loop read the previous sample.
//!
//! A running engine takes param and tempo edits without a rebuild (`diff`,
//! `Engine::apply`); for anything else, build a new engine and hand the old
//! one's module state over with `Engine::take_state`.

use std::ops::Range;
use std::path::Path;

use kerf_core::{
    Catalogue, JackKind, JackRef, Module, ModuleDescriptor, ModuleId, ParamDescriptor, Patch,
};
use kerf_modules::{Builtin, Dsp, Io};
use part_gain::{PartGain, Source};

mod atomic;
#[cfg(feature = "files")]
mod decode;
mod live;
mod part_gain;
pub mod samples;
mod wav;

pub use atomic::write_atomic;
pub use live::{Change, ParamSlot, diff};
pub use samples::SampleError;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EngineError {
    #[error("unknown module kind `{0}`")]
    UnknownKind(String),
    #[error("no jack `{0}`")]
    UnknownJack(JackRef),
    #[error("no module `{0}` in the engine")]
    UnknownModule(String),
    #[error("{kind} has no param `{param}`")]
    UnknownParam { kind: &'static str, param: String },
}

struct Node {
    id: ModuleId,
    desc: &'static ModuleDescriptor,
    dsp: Option<Box<dyn Dsp>>,
    params: Vec<f32>,
    /// For each input, the outputs feeding it.
    sources: Vec<Vec<Source>>,
    connected: Vec<bool>,
    inputs: Vec<f32>,
    outputs: Range<usize>,
    muted: bool,
    /// The sample path this module was built with, as the patch gave it.
    sample: Option<String>,
}

pub struct Engine {
    nodes: Vec<Node>,
    order: Vec<usize>,
    /// Latest value of every output jack, in volts.
    values: Vec<f32>,
    sample_rate: f32,
    tempo: f32,
    silent: Vec<String>,
    warnings: Vec<String>,
    /// One per part, in patch order.
    part_gains: Vec<PartGain>,
    gain_coef: f32,
}

impl Engine {
    /// Builds the graph. Expects a patch already checked by `kerf-session`;
    /// errors only cover kinds or jacks the catalogue does not know.
    pub fn new(patch: &Patch, sample_rate: f32) -> Result<Self, EngineError> {
        let style_seed = patch.style.as_ref().map_or(0, |s| s.seed);
        let mut nodes = Vec::with_capacity(patch.modules.len());
        let mut silent = Vec::new();
        let mut warnings = Vec::new();
        let mut next_output = 0;
        for m in &patch.modules {
            let desc = Builtin
                .get(&m.kind)
                .ok_or_else(|| EngineError::UnknownKind(m.kind.clone()))?;
            let seed = kerf_dsp::hash(m.id.as_str()) ^ style_seed;
            let loaded = match m.sample.as_deref().filter(|_| desc.takes_sample()) {
                Some(path) => samples::load(Path::new(path), sample_rate)
                    .map_err(|e| {
                        warnings.push(format!(
                            "{}: sample \"{path}\": {e}; playing the built-in demo sample",
                            m.id
                        ));
                    })
                    .ok(),
                None => None,
            };
            let dsp = kerf_modules::instantiate_with(desc.kind, seed, sample_rate, loaded);
            if dsp.is_none() && !silent.iter().any(|k| k == desc.kind) {
                silent.push(desc.kind.to_owned());
            }
            let params = desc.params.iter().map(|p| param_value(m, p)).collect();
            let n_in = desc.inputs.len();
            let n_out = desc.outputs.len();
            nodes.push(Node {
                id: m.id.clone(),
                desc,
                dsp,
                params,
                sources: vec![Vec::new(); n_in],
                connected: vec![false; n_in],
                inputs: vec![0.0; n_in],
                outputs: next_output..next_output + n_out,
                muted: is_muted(patch, m),
                sample: m.sample.clone(),
            });
            next_output += n_out;
        }

        let index_of = |jack: &JackRef| patch.modules.iter().position(|m| m.id == jack.module);
        let mut edges = Vec::with_capacity(patch.cables.len());
        for cable in &patch.cables {
            let unknown = |j: &JackRef| EngineError::UnknownJack(j.clone());
            let from = index_of(&cable.from).ok_or_else(|| unknown(&cable.from))?;
            let to = index_of(&cable.to).ok_or_else(|| unknown(&cable.to))?;
            let out = Builtin
                .get(&patch.modules[from].kind)
                .and_then(|d| {
                    d.outputs
                        .iter()
                        .position(|j| j.key == cable.from.jack.as_str())
                })
                .ok_or_else(|| unknown(&cable.from))?;
            let input = Builtin
                .get(&patch.modules[to].kind)
                .and_then(|d| {
                    d.inputs
                        .iter()
                        .position(|j| j.key == cable.to.jack.as_str())
                })
                .ok_or_else(|| unknown(&cable.to))?;
            let leaves_part = patch.modules[from].part != patch.modules[to].part;
            let source = Source {
                value: nodes[from].outputs.start + out,
                // Audio leaving a part for the rest of the patch is scaled by
                // the part's level.
                part: (leaves_part && nodes[from].desc.outputs[out].kind == JackKind::Audio)
                    .then(|| {
                        let part = patch.modules[from].part.as_ref();
                        patch.parts.iter().position(|p| part == Some(&p.id))
                    })
                    .flatten(),
            };
            nodes[to].sources[input].push(source);
            nodes[to].connected[input] = true;
            edges.push((from, to));
        }

        Ok(Self {
            order: run_order(nodes.len(), &edges),
            nodes,
            values: vec![0.0; next_output],
            sample_rate,
            tempo: patch.tempo,
            silent,
            warnings,
            part_gains: patch.parts.iter().map(|p| PartGain::new(p.level)).collect(),
            gain_coef: PartGain::coefficient(sample_rate),
        })
    }

    /// Module kinds in the patch whose DSP is not written yet. They output
    /// silence.
    pub fn silent_kinds(&self) -> &[String] {
        &self.silent
    }

    /// Problems building the graph that did not stop it, e.g. a sample file
    /// that is missing or cannot be decoded (the sampler plays its demo sample).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The tempo the engine is running at, in beats per minute.
    pub fn tempo(&self) -> f32 {
        self.tempo
    }

    /// Runs every module for one sample and returns the speaker frame.
    pub fn next_frame(&mut self) -> [f32; 2] {
        let mut master = [0.0; 2];
        for gain in &mut self.part_gains {
            gain.tick(self.gain_coef);
        }
        for &i in &self.order {
            let node = &mut self.nodes[i];
            let Some(dsp) = node.dsp.as_mut() else {
                continue;
            };
            for (input, sources) in node.inputs.iter_mut().zip(&node.sources) {
                let sum: f32 = sources
                    .iter()
                    .map(|s| {
                        self.values[s.value] * s.part.map_or(1.0, |p| self.part_gains[p].current)
                    })
                    .sum();
                // A feedback loop that has run away must not hand infinity or NaN to a
                // limiter or filter, which would never recover from it.
                *input = if sum.is_finite() { sum } else { 0.0 };
            }
            let outputs = &mut self.values[node.outputs.clone()];
            dsp.process(&mut Io {
                sample_rate: self.sample_rate,
                tempo: self.tempo,
                params: &node.params,
                inputs: &node.inputs,
                connected: &node.connected,
                outputs,
            });
            if node.muted {
                self.values[node.outputs.clone()].fill(0.0);
            } else if let Some([l, r]) = dsp.master() {
                master[0] += l;
                master[1] += r;
            }
        }
        master
    }

    /// Fills `out` with interleaved stereo frames (left, right, left, …),
    /// sample for sample what `next_frame` returns. No allocation. `out.len()`
    /// must be even.
    pub fn process_block(&mut self, out: &mut [f32]) {
        debug_assert!(
            out.len() % 2 == 0,
            "interleaved stereo needs an even length"
        );
        for frame in out.chunks_exact_mut(2) {
            let [l, r] = self.next_frame();
            frame[0] = l;
            frame[1] = r;
        }
    }

    /// Renders `frames` samples offline.
    pub fn render(&mut self, frames: usize) -> Render {
        let mut left = Vec::with_capacity(frames);
        let mut right = Vec::with_capacity(frames);
        for _ in 0..frames {
            let [l, r] = self.next_frame();
            left.push(l);
            right.push(r);
        }
        Render { left, right }
    }
}

/// Offline render output, `-1..=1` per channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Render {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl Render {
    pub fn peak(&self) -> f32 {
        self.left
            .iter()
            .chain(&self.right)
            .fold(0.0, |p, x| p.max(x.abs()))
    }

    pub fn rms(&self) -> f32 {
        let n = (self.left.len() + self.right.len()).max(1);
        let sum: f64 = self
            .left
            .iter()
            .chain(&self.right)
            .map(|&x| f64::from(x) * f64::from(x))
            .sum();
        (sum / n as f64).sqrt() as f32
    }
}

/// The patch value of `desc` on `module`, or the default when unset.
fn param_value(module: &Module, desc: &ParamDescriptor) -> f32 {
    module
        .params
        .iter()
        .find(|v| v.key.as_str() == desc.key)
        .map_or(desc.default, |v| v.value)
}

fn is_muted(patch: &Patch, module: &Module) -> bool {
    module
        .part
        .as_ref()
        .and_then(|p| patch.part(p))
        .is_some_and(|p| p.muted)
}

/// Topological order (Kahn). Modules left in loops run last, in patch order.
fn run_order(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut indegree = vec![0usize; n];
    for &(from, to) in edges {
        if from != to {
            indegree[to] += 1;
        }
    }
    let mut ready: Vec<usize> = (0..n).rev().filter(|&i| indegree[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    let mut placed = vec![false; n];
    while let Some(i) = ready.pop() {
        order.push(i);
        placed[i] = true;
        for &(from, to) in edges {
            if from == i && to != i {
                indegree[to] -= 1;
                if indegree[to] == 0 {
                    ready.push(to);
                }
            }
        }
    }
    order.extend((0..n).filter(|&i| !placed[i]));
    order
}

#[cfg(test)]
mod part_tests;
#[cfg(test)]
mod provided_tests;
#[cfg(all(test, feature = "files"))]
mod samples_tests;
#[cfg(test)]
mod tests;
