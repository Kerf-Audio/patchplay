//! patchplay as a CLAP and VST3 instrument.
//!
//! The real engine (`kerf-engine`) runs on the host's audio thread. Patches are parsed, checked
//! and compiled on other threads (the editor, or nih-plug's background task) and swapped in
//! through a lock-free queue, as the desktop app does. The patch is saved in the project as
//! `.kerf` text. See `docs/PLUGIN.md`.

mod doc;
mod editor;
mod params;
mod rack;
mod shared;

#[cfg(test)]
mod tests;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use nih_plug::prelude::*;

pub use doc::{PatchCell, PatchDoc};
pub use params::PatchplayParams;
pub use rack::{CHUNK, Controls, Host, Rack, Report};
pub use shared::Shared;

/// Work for nih-plug's background thread; none of it runs on the audio thread.
pub enum Task {
    /// The host restored the project state: open the saved patch.
    Reload,
    /// Build a fresh engine of the current patch (after a stop or a reset).
    Rearm,
    /// Drop engines the audio thread has finished with.
    Drain,
}

pub struct Patchplay {
    params: Arc<PatchplayParams>,
    shared: Arc<Shared>,
    rack: Rack,
}

impl Default for Patchplay {
    fn default() -> Self {
        let cell = Arc::new(PatchCell::default());
        let (shared, rack) = Shared::new(Arc::clone(&cell));
        Self {
            params: Arc::new(PatchplayParams::new(cell)),
            shared,
            rack,
        }
    }
}

impl Plugin for Patchplay {
    const NAME: &'static str = "patchplay";
    const VENDOR: &'static str = "Kerf Audio";
    const URL: &'static str = "https://github.com/Kerf-Audio/patchplay";
    const EMAIL: &'static str = "";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // A generator: no audio input. The engine has no audio-input module to feed one to.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    // The engine has no note or pitch input yet, so MIDI is not requested.
    const MIDI_INPUT: MidiConfig = MidiConfig::None;

    type SysExMessage = ();
    type BackgroundTask = Task;

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let shared = Arc::clone(&self.shared);
        Box::new(move |task| match task {
            Task::Reload => shared.reload(),
            Task::Rearm => {
                if let Err(message) = shared.publish() {
                    shared.report(message);
                }
            }
            Task::Drain => shared.drain_retired(),
        })
    }

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(Arc::clone(&self.params), Arc::clone(&self.shared))
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let sample_rate = buffer_config.sample_rate;
        self.shared.set_sample_rate(sample_rate);
        self.rack.set_sample_rate(sample_rate);
        // The state is in place by now; build the engine here, off the audio thread, at the
        // host's rate.
        self.params.patch.take_restored();
        self.shared.reload();
        true
    }

    fn reset(&mut self) {
        self.rack.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        if self.params.patch.take_restored() {
            context.execute_background(Task::Reload);
        }
        let transport = context.transport();
        let host = Host {
            playing: transport.playing,
            tempo: transport.tempo,
        };
        let frames = buffer.samples();
        let mut report = Report::default();
        if let [left, right, ..] = buffer.as_slice() {
            let mut at = 0;
            while at < frames {
                let end = (at + CHUNK).min(frames);
                let controls = self.params.controls(end - at);
                report.merge(self.rack.render(
                    &mut left[at..end],
                    &mut right[at..end],
                    host,
                    &controls,
                ));
                at = end;
            }
        }
        if report.rearm {
            context.execute_background(Task::Rearm);
        }
        if report.retired {
            context.execute_background(Task::Drain);
        }
        let shared = &self.shared;
        shared.running.store(report.running, Ordering::Relaxed);
        shared.host_tempo.store(
            (host.tempo.unwrap_or(0.0) as f32).to_bits(),
            Ordering::Relaxed,
        );
        if self.params.editor_state.is_open() {
            shared
                .peak
                .fetch_max(report.peak.to_bits(), Ordering::Relaxed);
        }
        // A generative patch can be quiet for a while: never let the host put it to sleep.
        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for Patchplay {
    const CLAP_ID: &'static str = "com.kerfaudio.patchplay";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A modular synth and sequencer: patches play in time with the host.");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for Patchplay {
    const VST3_CLASS_ID: [u8; 16] = *b"KerfPatchplay001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

// The export macros generate the C entry points the hosts load, which is unsafe code.
#[allow(unsafe_code, semicolon_in_expressions_from_non_local_macros)]
mod export {
    use nih_plug::prelude::*;

    use super::Patchplay;

    nih_export_clap!(Patchplay);
    nih_export_vst3!(Patchplay);
}
