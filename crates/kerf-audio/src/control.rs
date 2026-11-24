//! The editing side of the queue to the audio thread. Compiles engines here,
//! never on the audio thread, and decides how an edit travels: as param
//! changes, or as a rebuilt engine that inherits the running one's state.

use kerf_core::Patch;
use kerf_engine::{Engine, diff};
use rtrb::Producer;

use crate::AudioError;
use crate::voice::Msg;

pub(crate) struct Control {
    messages: Producer<Msg>,
    sample_rate: f32,
    /// The patch the audio thread is running; `None` when stopped.
    playing: Option<Patch>,
}

impl Control {
    pub(crate) fn new(messages: Producer<Msg>, sample_rate: u32) -> Self {
        Self {
            messages,
            sample_rate: sample_rate as f32,
            playing: None,
        }
    }

    /// Starts `patch`, fading out whatever is playing.
    pub(crate) fn play(&mut self, patch: &Patch) -> Result<(), AudioError> {
        self.swap(patch, false)?;
        self.playing = Some(patch.clone());
        Ok(())
    }

    /// Applies an edit to the playing patch. Param and tempo edits go through
    /// the queue; anything else swaps in a rebuilt engine that keeps the
    /// module state of modules with the same id and kind. Does nothing when
    /// stopped.
    pub(crate) fn update(&mut self, patch: &Patch) -> Result<(), AudioError> {
        let Some(current) = &self.playing else {
            return Ok(());
        };
        match diff(current, patch) {
            Some(changes) if changes.len() <= self.messages.slots() => {
                for change in changes {
                    self.messages
                        .push(Msg::Change(change))
                        .map_err(|_| AudioError::Busy)?;
                }
            }
            _ => self.swap(patch, true)?,
        }
        self.playing = Some(patch.clone());
        Ok(())
    }

    pub(crate) fn stop(&mut self) -> Result<(), AudioError> {
        if self.playing.is_some() {
            self.messages
                .push(Msg::Stop)
                .map_err(|_| AudioError::Busy)?;
            self.playing = None;
        }
        Ok(())
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    fn swap(&mut self, patch: &Patch, carry: bool) -> Result<(), AudioError> {
        let engine = Box::new(Engine::new(patch, self.sample_rate)?);
        self.messages
            .push(Msg::Swap { engine, carry })
            .map_err(|_| AudioError::Busy)
    }
}
