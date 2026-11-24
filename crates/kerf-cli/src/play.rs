//! `kerf play`: live playback. Commands arrive as JSON lines on stdin and go
//! through the session like any other front end's; each change reaches the
//! running engine through `Player::update`.

use std::io::{self, BufRead};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use kerf_audio::Player;
use kerf_core::{Command, Patch};
use kerf_modules::Builtin;
use kerf_session::Session;
use serde_json::json;

use crate::open;

/// Long enough for the player's fade-out to finish before the process exits.
const FADE_OUT: Duration = Duration::from_millis(60);

enum Event {
    Line(String),
    Quit,
}

pub fn run(patch: &Path, seconds: Option<f32>) -> Result<()> {
    if seconds.is_some_and(|s| !(s > 0.0 && s <= 86_400.0)) {
        bail!("--seconds must be above 0 and at most 86400");
    }
    let mut session = open(patch)?;
    let player = Player::new()?;
    player.play(&resolved(&session, patch))?;
    eprintln!(
        "playing {} at {} Hz; JSON commands on stdin, Ctrl-C stops",
        patch.display(),
        player.sample_rate()
    );

    let (tx, rx) = mpsc::channel();
    let quit = tx.clone();
    ctrlc::set_handler(move || {
        let _ = quit.send(Event::Quit);
    })
    .context("installing the Ctrl-C handler")?;
    // Left blocked on stdin at exit; the process ending ends it.
    thread::spawn(move || {
        for line in io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(Event::Line(line)).is_err() {
                break;
            }
        }
    });

    let deadline = seconds.map(|s| Instant::now() + Duration::from_secs_f32(s));
    loop {
        let event = match deadline {
            Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(event) => event,
                Err(_) => break,
            },
            None => match rx.recv() {
                Ok(event) => event,
                Err(_) => break,
            },
        };
        let line = match event {
            Event::Quit => break,
            Event::Line(line) => line,
        };
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let result = serde_json::from_str::<Command>(line)
            .map_err(|e| format!("not a command: {e}"))
            .and_then(|cmd| session.apply(cmd).map_err(|e| e.to_string()))
            .and_then(|()| {
                player
                    .update(&resolved(&session, patch))
                    .map_err(|e| e.to_string())
            });
        if let Err(message) = result {
            println!("{}", json!({ "error": message }));
        }
    }
    player.stop();
    thread::sleep(FADE_OUT);
    Ok(())
}

/// The session's patch with sample paths resolved against the patch file, as
/// `kerf render` does.
fn resolved(session: &Session<Builtin>, file: &Path) -> Patch {
    let mut patch = session.patch().clone();
    kerf_engine::samples::resolve_paths(&mut patch, file.parent().unwrap_or(Path::new("")));
    patch
}
