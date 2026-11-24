//! The desktop's bridge: `kerf_bridge::Bridge` plus what needs the file system. Everything else
//! (every other command, the factory presets) is the shared bridge, reached through `Deref`.
//! The Tauri commands and the `--serve` HTTP endpoints both call this, so the web UI behaves
//! the same in the app and in a browser.

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use kerf_bridge::presets::PREFIX;
pub use kerf_bridge::{
    BridgeError, Checked, Description, PresetInfo, RenderSummary, SamplePeaks, State, Suggestions,
    file_error,
};
use kerf_engine::write_atomic;

use crate::transport::Transport;

pub struct Bridge {
    core: kerf_bridge::Bridge,
}

impl Deref for Bridge {
    type Target = kerf_bridge::Bridge;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl Bridge {
    /// An empty patch.
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Self {
            core: kerf_bridge::Bridge::new(transport),
        }
    }

    /// Opens a `.kerf` file, or a factory preset by its `preset:<name>` path (an unsaved
    /// session, so Save asks for a new file instead of overwriting anything).
    pub fn open_file(&self, path: &Path) -> Result<State, BridgeError> {
        if let Some(name) = path.to_str().and_then(|p| p.strip_prefix(PREFIX)) {
            return self.core.open_preset(name);
        }
        let src = fs::read_to_string(path).map_err(|e| file_error(path, e))?;
        let shown = path.display().to_string();
        self.core.open_text(&shown, &src, Some(shown.clone()))
    }

    /// Saves to `path`, or to the file the patch came from when `None`. The file is a `.kerf`
    /// file: a name without an extension gets one, any other extension is refused.
    pub fn save_file(&self, path: Option<&Path>) -> Result<State, BridgeError> {
        let target = match path {
            Some(p) => p.to_path_buf(),
            None => self
                .core
                .path()
                .map(PathBuf::from)
                .ok_or_else(|| BridgeError::Invalid("choose a file to save to".into()))?,
        };
        let target = with_extension(&target, "kerf")?;
        write_atomic(&target, self.core.text().as_bytes()).map_err(|e| file_error(&target, e))?;
        Ok(self.core.set_path(Some(target.display().to_string())))
    }

    /// Renders the current patch offline to a 16-bit stereo WAV at `path`, which gets `.wav`
    /// when it has no extension and is refused with any other.
    pub fn render_wav(&self, path: &Path, seconds: f32) -> Result<RenderSummary, BridgeError> {
        let path = with_extension(path, "wav")?;
        let (bytes, mut summary) = self.core.render_wav_bytes(seconds)?;
        write_atomic(&path, &bytes).map_err(|e| file_error(&path, e))?;
        summary.path = path.display().to_string();
        Ok(summary)
    }
}

/// `path` ending in `.ext`: as it is when it does, with the extension added when it has none (a
/// save dialog on Linux does not add one), and an error for any other, so a command cannot be
/// pointed at, say, a shell profile.
fn with_extension(path: &Path, ext: &str) -> Result<PathBuf, BridgeError> {
    match path.extension() {
        None => {
            let mut name = path.as_os_str().to_owned();
            name.push(".");
            name.push(ext);
            Ok(name.into())
        }
        Some(have) if have.eq_ignore_ascii_case(ext) => Ok(path.to_path_buf()),
        Some(_) => Err(BridgeError::Invalid(format!(
            "{} must end in .{ext}",
            path.display()
        ))),
    }
}
