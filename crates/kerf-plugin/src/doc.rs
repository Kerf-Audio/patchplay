//! The patch as plugin state: `.kerf` text and a name, saved in the host project.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use nih_plug::params::persist::PersistentField;
use serde::{Deserialize, Serialize};

/// What the project stores. An empty `text` means "use the default patch".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PatchDoc {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub text: String,
}

/// The persisted field. When the host restores state after the plugin is running, `set` raises
/// `restored`; the audio thread sees that and asks a background task to reload the patch, so no
/// engine is ever built on the audio thread.
#[derive(Default)]
pub struct PatchCell {
    doc: Mutex<PatchDoc>,
    restored: AtomicBool,
    /// Counts changes to the document, so a reader can tell cheaply whether to look again.
    version: AtomicU32,
}

impl PatchCell {
    pub fn get(&self) -> PatchDoc {
        self.doc.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Stores a patch the plugin itself loaded. Does not raise `restored`.
    pub fn store(&self, doc: PatchDoc) {
        *self.doc.lock().unwrap_or_else(|e| e.into_inner()) = doc;
        self.version.fetch_add(1, Ordering::Release);
    }

    /// The patch's name only, without copying the text.
    pub fn name(&self) -> String {
        self.doc
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .name
            .clone()
    }

    /// Changes whenever the name or the text does.
    pub fn version(&self) -> u32 {
        self.version.load(Ordering::Acquire)
    }

    pub fn set_name(&self, name: &str) {
        self.doc.lock().unwrap_or_else(|e| e.into_inner()).name = name.to_owned();
        self.version.fetch_add(1, Ordering::Release);
    }

    /// True once after the host restored state. Lock-free.
    pub fn take_restored(&self) -> bool {
        self.restored.swap(false, Ordering::Relaxed)
    }
}

impl PersistentField<'_, PatchDoc> for std::sync::Arc<PatchCell> {
    fn set(&self, new_value: PatchDoc) {
        self.store(new_value);
        self.restored.store(true, Ordering::Relaxed);
    }

    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&PatchDoc) -> R,
    {
        f(&self.doc.lock().unwrap_or_else(|e| e.into_inner()))
    }
}
