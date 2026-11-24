//! Offline render and the Veneer waveform.

use std::path::Path;

use kerf_core::{Catalogue, ModuleDescriptor};
use kerf_engine::{Engine, samples};
use kerf_modules::{Builtin, VENEER_SLICES, demo_sample};

use crate::{Bridge, BridgeError, RenderSummary, SamplePeaks};

pub const RENDER_RATE: u32 = 48_000;
pub const MAX_RENDER_SECONDS: f32 = 600.0;
/// Min/max pairs in a waveform.
const PEAK_BINS: usize = 256;

impl Bridge {
    /// Renders the current patch offline to a 16-bit stereo WAV file, in memory. `path` in the
    /// summary is left empty for the host to fill in once it has saved the bytes.
    pub fn render_wav_bytes(&self, seconds: f32) -> Result<(Vec<u8>, RenderSummary), BridgeError> {
        if !(0.0..=MAX_RENDER_SECONDS).contains(&seconds) {
            return Err(BridgeError::Invalid(format!(
                "length must be between 0 and {MAX_RENDER_SECONDS} seconds"
            )));
        }
        // Rendering takes a while; do it on a copy so the UI stays live.
        let patch = self.lock().engine_patch();
        let mut engine = Engine::new(&patch, RENDER_RATE as f32)?;
        let r = engine.render((seconds * RENDER_RATE as f32) as usize);
        let bytes = r
            .wav_bytes(RENDER_RATE)
            .map_err(|e| BridgeError::Invalid(e.to_string()))?;
        let db = |x: f32| 20.0 * x.max(1e-9).log10();
        let summary = RenderSummary {
            path: String::new(),
            seconds,
            peak_db: db(r.peak()),
            rms_db: db(r.rms()),
            silent_kinds: engine.silent_kinds().to_vec(),
            warnings: engine.warnings().to_vec(),
        };
        Ok((bytes, summary))
    }

    /// The waveform of the sample a Veneer or Splinter plays: the decoded file, or the
    /// demo sample when it has none (or the file cannot be used).
    pub fn sample_peaks(&self, module: &str) -> Result<SamplePeaks, BridgeError> {
        let patch = self.lock().engine_patch();
        let m = patch
            .modules
            .iter()
            .find(|m| m.id.as_str() == module)
            .ok_or_else(|| BridgeError::Invalid(format!("no module `{module}`")))?;
        if !Builtin
            .get(&m.kind)
            .is_some_and(ModuleDescriptor::takes_sample)
        {
            return Err(BridgeError::Invalid(format!(
                "`{module}` is a {}; only a Veneer or Splinter plays a sample",
                m.kind
            )));
        }
        let (mut file, mut warning, mut loaded) = (None, None, None);
        if let Some(path) = &m.sample {
            match samples::load(Path::new(path), RENDER_RATE as f32) {
                Ok(buf) => {
                    file = Path::new(path).file_name().map(|n| {
                        let name = n.to_string_lossy();
                        name.strip_prefix("web:").unwrap_or(&name).to_owned()
                    });
                    loaded = Some(buf);
                }
                Err(e) => warning = Some(format!("{path}: {e}; playing the built-in demo sample")),
            }
        }
        let buf = loaded.unwrap_or_else(|| demo_sample(RENDER_RATE as f32));
        Ok(peaks_of(&buf, file, warning))
    }

    /// The waveform of the built-in demo sample, for showing a sampler that is not in a patch.
    pub fn demo_sample_peaks() -> SamplePeaks {
        peaks_of(&demo_sample(RENDER_RATE as f32), None, None)
    }
}

fn peaks_of(buf: &[f32], file: Option<String>, warning: Option<String>) -> SamplePeaks {
    SamplePeaks {
        peaks: samples::peaks(buf, PEAK_BINS),
        seconds: buf.len() as f32 / RENDER_RATE as f32,
        slices: VENEER_SLICES,
        file,
        warning,
    }
}
