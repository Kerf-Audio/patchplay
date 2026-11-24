//! What each tool does. Every patch change goes through `Session::apply`.

use std::path::PathBuf;

use kerf_audio::Player;
use kerf_core::text::print;
use kerf_core::{Catalogue, Command, Patch};
use kerf_engine::Engine;
use kerf_modules::Builtin;
use kerf_session::Session;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::{INVALID_PARAMS, RpcError};

const DEFAULT_SECONDS: f32 = 8.0;
const MAX_SECONDS: f32 = 120.0;
const DEFAULT_RATE: u32 = 48_000;
const RATES: std::ops::RangeInclusive<u32> = 8_000..=192_000;
const DEFAULT_STRENGTH: f32 = 0.3;

/// Text blocks for the client, or the message of a tool failure.
type Reply = Result<Vec<String>, String>;

pub struct Server {
    session: Session<Builtin>,
    /// Opened by the first `play`, so a machine without audio can still use
    /// every other tool.
    player: Option<Player>,
    base: Option<PathBuf>,
    renders: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    commands: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Random {
    style: String,
    seed: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evolve {
    strength: Option<f32>,
    seed: Option<u64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum With {
    Node(u32),
    Patch(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Breed {
    with: With,
    seed: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Render {
    seconds: Option<f32>,
    rate: Option<u32>,
    path: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Describe {
    kind: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Docs {
    section: Option<String>,
}

fn parse<T: DeserializeOwned>(args: &Value) -> Result<T, String> {
    let args = if args.is_null() { &json!({}) } else { args };
    serde_json::from_value(args.clone()).map_err(|e| format!("bad arguments: {e}"))
}

fn text(value: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(value).expect("tool results are plain JSON")
}

fn db(gain: f32) -> f32 {
    20.0 * gain.max(1e-9).log10()
}

impl Server {
    /// `base` is the folder of the `.kerf` file the session was opened from,
    /// if any; relative sample paths are taken from it.
    pub fn new(session: Session<Builtin>, base: Option<PathBuf>) -> Self {
        Self {
            session,
            base,
            player: None,
            renders: 0,
        }
    }

    /// Runs `tools/call`. Unknown tools are protocol errors; failures of a
    /// known tool are results flagged `isError`, so the agent can read them.
    pub fn call(&mut self, params: &Value) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "tools/call needs a tool name".to_string()))?;
        let args = params.get("arguments").unwrap_or(&Value::Null);
        let reply = match name {
            "patch_get" => Ok(vec![print(self.session.patch())]),
            "patch_apply" => self.patch_apply(args),
            "random" => self.random(args),
            "evolve" => self.evolve(args),
            "breed" => self.breed(args),
            "lineage" => Ok(vec![self.lineage()]),
            "render" => self.render(args),
            "check" => Ok(vec![text(&kerf_gen::safe_check(self.session.patch()))]),
            "describe_module" => Self::describe_module(args),
            "suggest_cables" => self.suggest_cables(args),
            "language_docs" => Self::language_docs(args),
            "play" => self.play(),
            "stop" => {
                if let Some(player) = &self.player {
                    player.stop();
                }
                Ok(vec!["stopped".to_string()])
            }
            _ => return Err((INVALID_PARAMS, format!("unknown tool: {name}"))),
        };
        let (blocks, is_error) = match reply {
            Ok(blocks) => (blocks, false),
            Err(message) => (vec![message], true),
        };
        let content: Vec<Value> = blocks
            .into_iter()
            .map(|t| json!({ "type": "text", "text": t }))
            .collect();
        Ok(json!({ "content": content, "isError": is_error }))
    }

    /// The patch with relative sample paths made absolute against the folder
    /// of the file the server started from, as `kerf render` does. With no
    /// file, paths stay as written and are taken from the working directory.
    fn resolved(&self) -> Patch {
        let mut patch = self.session.patch().clone();
        if let Some(dir) = &self.base {
            kerf_engine::samples::resolve_paths(&mut patch, dir);
        }
        patch
    }

    /// Hands the current patch to the live engine, if one is playing.
    fn refresh_player(&self) -> Result<(), String> {
        match &self.player {
            Some(player) => player
                .update(&self.resolved())
                .map_err(|e| format!("applied, but the live engine refused the patch: {e}")),
            None => Ok(()),
        }
    }

    fn patch_apply(&mut self, args: &Value) -> Reply {
        let Apply { commands } = parse(args)?;
        let mut trial = self.session.clone();
        let mut connected = Vec::new();
        for (i, raw) in commands.into_iter().enumerate() {
            let command: Command = serde_json::from_value(raw)
                .map_err(|e| format!("command {i} is not a command: {e}; nothing was applied"))?;
            trial
                .apply(command)
                .map_err(|e| format!("command {i}: {e}; nothing was applied"))?;
            connected.extend_from_slice(trial.last_plan());
        }
        self.session = trial;
        self.refresh_player()?;
        let mut reply = vec![print(self.session.patch())];
        if !connected.is_empty() {
            reply.push(text(&json!({ "connected": connected })));
        }
        Ok(reply)
    }

    /// Applies a generator command and reports the lineage node it made and
    /// the seed it ran with (the caller's, or the session's default).
    fn generate(&mut self, command: Command) -> Reply {
        self.session.apply(command).map_err(|e| e.to_string())?;
        self.refresh_player()?;
        let node = self.session.lineage().current_node();
        Ok(vec![
            json!({ "node": node.id, "seed": node.origin.seed() }).to_string(),
            print(self.session.patch()),
        ])
    }

    fn random(&mut self, args: &Value) -> Reply {
        let Random { style, seed } = parse(args)?;
        self.generate(Command::Random { style, seed })
    }

    fn evolve(&mut self, args: &Value) -> Reply {
        let Evolve { strength, seed } = parse(args)?;
        let strength = strength.unwrap_or(DEFAULT_STRENGTH);
        self.generate(Command::Evolve { strength, seed })
    }

    fn breed(&mut self, args: &Value) -> Reply {
        let Breed { with, seed } = parse(args)?;
        let with = match with {
            With::Node(n) => kerf_core::BreedWith::Node(n),
            With::Patch(text) => kerf_core::BreedWith::Patch(text),
        };
        self.generate(Command::Breed { with, seed })
    }

    fn lineage(&self) -> String {
        let lineage = self.session.lineage();
        let nodes: Vec<Value> = lineage
            .nodes
            .iter()
            .map(|n| {
                json!({
                    "id": n.id,
                    "parents": n.parents,
                    "origin": n.origin,
                    "current": n.id == lineage.current,
                })
            })
            .collect();
        text(&json!({ "current": lineage.current, "nodes": nodes }))
    }

    fn render(&mut self, args: &Value) -> Reply {
        let Render {
            seconds,
            rate,
            path,
        } = parse(args)?;
        let seconds = seconds.unwrap_or(DEFAULT_SECONDS);
        if !(seconds > 0.0 && seconds <= MAX_SECONDS) {
            return Err(format!("seconds must be above 0 and at most {MAX_SECONDS}"));
        }
        let rate = rate.unwrap_or(DEFAULT_RATE);
        if !RATES.contains(&rate) {
            return Err(format!(
                "rate must be between {} and {}",
                RATES.start(),
                RATES.end()
            ));
        }
        let path = path.unwrap_or_else(|| {
            self.renders += 1;
            std::env::temp_dir().join(format!(
                "kerf-render-{}-{}.wav",
                std::process::id(),
                self.renders
            ))
        });
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        {
            return Err(format!("{} must end in .wav", path.display()));
        }
        let mut engine = Engine::new(&self.resolved(), rate as f32).map_err(|e| e.to_string())?;
        let render = engine.render((seconds * rate as f32) as usize);
        render
            .write_wav(&path, rate)
            .map_err(|e| format!("writing {}: {e}", path.display()))?;
        Ok(vec![text(&json!({
            "path": path.display().to_string(),
            "seconds": seconds,
            "rate": rate,
            "peak_db": db(render.peak()),
            "rms_db": db(render.rms()),
            "silent_kinds": engine.silent_kinds(),
            "warnings": engine.warnings(),
        }))])
    }

    fn describe_module(args: &Value) -> Reply {
        let Describe { kind } = parse(args)?;
        match kind {
            Some(kind) => match Builtin.get(&kind) {
                Some(descriptor) => Ok(vec![text(descriptor)]),
                None => Err(format!(
                    "no module kind `{kind}`; call describe_module without `kind` to list them"
                )),
            },
            None => {
                let all: Vec<Value> = Builtin
                    .all()
                    .iter()
                    .map(|d| json!({ "kind": d.kind, "band": d.band, "summary": d.summary }))
                    .collect();
                Ok(vec![text(&all)])
            }
        }
    }

    /// Describes the cables auto-patching would add.
    fn suggest_cables(&self, args: &Value) -> Reply {
        let q: kerf_session::Suggest = parse(args)?;
        let cables = self.session.suggest_cables(&q).map_err(|e| e.to_string())?;
        Ok(vec![text(&json!({ "cables": cables }))])
    }

    /// The language guide: section ids and titles, or one section's text.
    fn language_docs(args: &Value) -> Reply {
        let Docs { section } = parse(args)?;
        let docs = kerf_session::language_docs(&Builtin);
        match section {
            None => {
                Ok(vec![
                    docs.sections.iter().fold(
                        "Call language_docs again with `section` set to one of these ids:\n"
                            .to_string(),
                        |mut s, d| {
                            s.push_str(&format!("{}: {}\n", d.id, d.title));
                            s
                        },
                    ),
                ])
            }
            Some(id) => docs
                .sections
                .into_iter()
                .find(|d| d.id == id)
                .map(|d| vec![format!("# {}\n\n{}", d.title, d.markdown)])
                .ok_or_else(|| {
                    format!("no section `{id}`; call language_docs without `section` to list them")
                }),
        }
    }

    fn play(&mut self) -> Reply {
        let player = match self.player.take() {
            Some(player) => player,
            None => Player::new().map_err(|e| format!("no audio output: {e}"))?,
        };
        let started = player.play(&self.resolved());
        let rate = player.sample_rate();
        self.player = Some(player);
        started.map_err(|e| format!("cannot play: {e}"))?;
        Ok(vec![format!(
            "playing at {rate} Hz; later changes are heard live"
        )])
    }
}
