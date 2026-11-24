//! Kerf Modular desktop app. `kerf_bridge::Bridge` is the single implementation of what the UI
//! can do (shared with the browser build); `bridge` adds the file system, `shell` exposes it as
//! Tauri commands and `serve` as HTTP.

pub mod audio;
pub mod bridge;
pub mod serve;
pub mod shell;
pub mod transport;

#[cfg(test)]
mod tests;
