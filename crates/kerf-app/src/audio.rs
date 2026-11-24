//! The live audio output: a `kerf_audio::Player` on a thread of its own.
//!
//! The device's stream is not `Send` on every platform, so the player never
//! leaves the thread that made it; the bridge talks to it over a channel.

use std::sync::mpsc::{self, Sender};
use std::thread;

use kerf_audio::Player;
use kerf_core::Patch;

use crate::transport::{Meter, NoAudio, Scope, Transport};

/// The default output device, or a transport that refuses to play and says
/// why when none can be opened.
pub fn transport() -> Box<dyn Transport> {
    match Threaded::spawn() {
        Ok(t) => Box::new(t),
        Err(reason) => Box::new(NoAudio(reason)),
    }
}

enum Request {
    Play(Patch, Sender<Result<(), String>>),
    Update(Patch),
    Stop,
    IsPlaying(Sender<bool>),
    Meter(Sender<Meter>),
    Scope(Sender<Scope>),
}

struct Threaded {
    requests: Sender<Request>,
}

impl Threaded {
    fn spawn() -> Result<Self, String> {
        let (requests, inbox) = mpsc::channel();
        let (ready, opened) = mpsc::channel();
        thread::Builder::new()
            .name("kerf-audio".into())
            .spawn(move || match Player::new() {
                Ok(player) => {
                    let _ = ready.send(Ok(()));
                    serve(&player, &inbox);
                }
                Err(e) => {
                    let _ = ready.send(Err(e.to_string()));
                }
            })
            .map_err(|e| e.to_string())?;
        opened
            .recv()
            .map_err(|_| "the audio thread stopped before it opened a device".to_owned())??;
        Ok(Self { requests })
    }

    fn ask<R>(&self, make: impl FnOnce(Sender<R>) -> Request) -> Option<R> {
        let (reply, answer) = mpsc::channel();
        self.requests.send(make(reply)).ok()?;
        answer.recv().ok()
    }
}

fn serve(player: &Player, inbox: &mpsc::Receiver<Request>) {
    for request in inbox {
        match request {
            Request::Play(patch, reply) => {
                let _ = reply.send(player.play(&patch).map_err(|e| e.to_string()));
            }
            Request::Update(patch) => {
                // A lost device surfaces on the next `play`.
                let _ = player.update(&patch);
            }
            Request::Stop => player.stop(),
            Request::IsPlaying(reply) => {
                let _ = reply.send(player.is_playing());
            }
            Request::Meter(reply) => {
                let m = player.meter();
                let _ = reply.send(Meter {
                    peak_l: m.peak_l,
                    peak_r: m.peak_r,
                    load: m.load,
                });
            }
            Request::Scope(reply) => {
                let _ = reply.send(Scope {
                    rate: player.sample_rate(),
                    samples: player.scope(),
                });
            }
        }
    }
}

impl Transport for Threaded {
    fn play(&mut self, patch: &Patch) -> Result<(), String> {
        self.ask(|reply| Request::Play(patch.clone(), reply))
            .unwrap_or_else(|| Err("the audio thread has stopped".into()))
    }

    fn update(&mut self, patch: &Patch) {
        let _ = self.requests.send(Request::Update(patch.clone()));
    }

    fn stop(&mut self) {
        let _ = self.requests.send(Request::Stop);
    }

    fn is_playing(&self) -> bool {
        self.ask(Request::IsPlaying).unwrap_or(false)
    }

    fn meter(&self) -> Meter {
        self.ask(Request::Meter).unwrap_or_default()
    }

    fn scope(&self) -> Scope {
        self.ask(Request::Scope).unwrap_or_default()
    }
}
