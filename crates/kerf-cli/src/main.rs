//! `kerf`: the command-line front end. Like the GUI and MCP server, it changes
//! patches only through `Command`s applied by a `Session`.

#[path = "gen.rs"]
mod generator;
mod play;

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use kerf_core::text::{parse, print};
use kerf_core::{Band, Catalogue, Command, ModuleDescriptor};
use kerf_engine::write_atomic;
use kerf_modules::Builtin;
use kerf_session::Session;

#[derive(Parser)]
#[command(
    name = "kerf",
    version,
    about = "patchplay: a modular audio playground for the command line, agents and GUI.",
    after_help = "Examples:\n  kerf new --style acid-line --seed 7 -o acid.kerf\n  kerf render acid.kerf -o acid.wav --seconds 8\n  kerf check acid.kerf\n  kerf describe Plane\n\nPatches are `.kerf` text files; see docs/KERF-FORMAT.md. Full reference: docs/CLI.md."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Write a new patch: empty, or generated with `--style`.
    New {
        /// Defaults to 120, or to the style's own choice with `--style`.
        #[arg(long)]
        tempo: Option<f32>,
        /// Generate a patch in this style (see `kerf styles`) instead of an empty one.
        #[arg(long)]
        style: Option<String>,
        /// Seed for `--style`; the same style and seed give the same patch.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Write to this `.kerf` file instead of stdout. Refuses to overwrite.
        #[arg(short, long, value_parser = kerf_file)]
        out: Option<PathBuf>,
    },
    /// List module kinds, or describe one: params, jacks and what they do.
    Describe {
        /// A module kind, e.g. `Plane`. Case does not matter.
        kind: Option<String>,
        /// Print the descriptor(s) as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Apply commands (one JSON object per line) to a patch and print the result.
    ///
    /// Blank lines and lines starting with `#` are skipped. The first bad or failing line stops
    /// the run with its line number, and nothing is written. The result is the whole patch as
    /// `.kerf` text, on stdout or in `--out`.
    #[command(
        after_help = "Examples:\n  echo '{\"op\":\"set_tempo\",\"bpm\":100}' | kerf apply song.kerf\n  kerf apply song.kerf -c edits.jsonl -o song.kerf\n\nExample line: {\"op\":\"add_module\",\"kind\":\"Plane\"}\nEvery op is listed in docs/CLI.md."
    )]
    Apply {
        /// The `.kerf` patch to start from.
        patch: PathBuf,
        /// Command file; `-` reads stdin.
        #[arg(short, long, default_value = "-")]
        commands: PathBuf,
        /// Write to this file instead of stdout. May be the input patch.
        #[arg(short, long, value_parser = kerf_file)]
        out: Option<PathBuf>,
    },
    /// Render a patch offline to a 16-bit stereo WAV.
    ///
    /// Prints the file, peak and RMS in dBFS. Does not touch the audio device.
    Render {
        /// The `.kerf` patch to render. Sample paths in it are relative to this file.
        patch: PathBuf,
        /// The WAV file to write, ending in `.wav`. Overwrites an existing file.
        #[arg(short, long, value_parser = wav_file)]
        out: PathBuf,
        /// Length to render, from 0 to 600.
        #[arg(long, default_value_t = 16.0)]
        seconds: f32,
        /// Sample rate in Hz, from 8000 to 192000.
        #[arg(long, default_value_t = 48_000, value_parser = clap::value_parser!(u32).range(8_000..=192_000))]
        rate: u32,
    },
    /// Mutate a patch, keeping locked modules, params and parts as they are.
    Evolve {
        /// The `.kerf` patch to mutate.
        patch: PathBuf,
        /// 0 changes nothing, 1 changes a lot.
        #[arg(long, default_value_t = 0.3)]
        strength: f32,
        /// A part id, a module id or `module.param` to lock first. Repeatable.
        #[arg(long)]
        lock: Vec<String>,
        /// Defaults to 1, so the same file evolves the same way each time.
        #[arg(long)]
        seed: Option<u64>,
        /// Write to this file instead of stdout. May be the input patch.
        #[arg(short, long, value_parser = kerf_file)]
        out: Option<PathBuf>,
    },
    /// Cross two patches into a child that uses only parameters from the parents.
    Breed {
        /// The first parent. The child starts from this patch.
        a: PathBuf,
        /// The second parent.
        b: PathBuf,
        /// Defaults to 1.
        #[arg(long)]
        seed: Option<u64>,
        /// Write to this file instead of stdout. May be a parent.
        #[arg(short, long, value_parser = kerf_file)]
        out: Option<PathBuf>,
    },
    /// Render a patch and report peak, RMS, silence, DC, clipping and runaway
    /// feedback as JSON. Exits 1 if it fails.
    Check {
        /// The `.kerf` patch to check.
        patch: PathBuf,
    },
    /// List the generator styles: tempo range and parts.
    Styles,
    /// Play a patch live. Reads commands (one JSON object per line) from stdin
    /// and applies them as the patch plays. Ctrl-C stops.
    Play {
        /// The `.kerf` patch to play.
        patch: PathBuf,
        /// Stop after this many seconds instead of waiting for Ctrl-C.
        #[arg(long)]
        seconds: Option<f32>,
    },
    /// Run the MCP server on stdio, so an agent can build, play and render patches.
    Mcp {
        /// A `.kerf` patch to start from; without one the session starts empty.
        patch: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::New {
            tempo,
            style,
            seed,
            out,
        } => generator::new(tempo, style.as_deref(), seed, out.as_deref()),
        Cmd::Describe { kind, json } => describe(kind.as_deref(), json),
        Cmd::Apply {
            patch,
            commands,
            out,
        } => apply(&patch, &commands, out.as_deref()),
        Cmd::Render {
            patch,
            out,
            seconds,
            rate,
        } => render(&patch, &out, seconds, rate),
        Cmd::Evolve {
            patch,
            strength,
            lock,
            seed,
            out,
        } => generator::evolve(&patch, strength, &lock, seed, out.as_deref()),
        Cmd::Breed { a, b, seed, out } => generator::breed(&a, &b, seed, out.as_deref()),
        Cmd::Check { patch } => generator::check(&patch),
        Cmd::Styles => {
            generator::styles();
            Ok(())
        }
        Cmd::Play { patch, seconds } => play::run(&patch, seconds),
        Cmd::Mcp { patch } => {
            let (session, base) = match patch {
                Some(path) => {
                    let base = path.parent().unwrap_or(Path::new("")).to_path_buf();
                    (open(&path)?, Some(base))
                }
                None => (Session::new(Builtin), None),
            };
            Ok(kerf_mcp::serve_stdio(session, base)?)
        }
    }
}

fn open(path: &Path) -> Result<Session<Builtin>> {
    let src = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let patch = parse(&src).with_context(|| path.display().to_string())?;
    Session::open(Builtin, patch).with_context(|| path.display().to_string())
}

fn render(patch: &Path, out: &Path, seconds: f32, rate: u32) -> Result<()> {
    if !(0.0..=600.0).contains(&seconds) {
        bail!("--seconds must be between 0 and 600");
    }
    let s = open(patch)?;
    let mut resolved = s.patch().clone();
    kerf_engine::samples::resolve_paths(&mut resolved, patch.parent().unwrap_or(Path::new("")));
    let mut engine = kerf_engine::Engine::new(&resolved, rate as f32)?;
    let r = engine.render((seconds * rate as f32) as usize);
    r.write_wav(out, rate)
        .with_context(|| format!("writing {}", out.display()))?;
    let db = |x: f32| 20.0 * x.max(1e-9).log10();
    println!(
        "{}: {seconds} s, peak {:.1} dBFS, RMS {:.1} dBFS",
        out.display(),
        db(r.peak()),
        db(r.rms())
    );
    if !engine.silent_kinds().is_empty() {
        println!(
            "silent (DSP not written yet): {}",
            engine.silent_kinds().join(", ")
        );
    }
    for warning in engine.warnings() {
        eprintln!("warning: {warning}");
    }
    Ok(())
}

/// Writes the patch as `.kerf` text to `out`, or to stdout. `new` passes `overwrite: false`:
/// `create_new` refuses a path that exists, even a dangling symlink, which a plain write would
/// follow and fill in. Otherwise the file is replaced whole (the input patch is a fair `--out`).
fn emit(patch: &kerf_core::Patch, out: Option<&Path>, overwrite: bool) -> Result<()> {
    let text = print(patch);
    let Some(path) = out else {
        print!("{text}");
        return Ok(());
    };
    let written = if overwrite {
        write_atomic(path, text.as_bytes())
    } else {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                bail!("{} already exists", path.display())
            }
            opened => opened.and_then(|mut file| file.write_all(text.as_bytes())),
        }
    };
    written.with_context(|| format!("writing {}", path.display()))
}

/// An output path that ends in `.kerf`, so a mistyped `-o` cannot overwrite some other file.
fn kerf_file(s: &str) -> Result<PathBuf, String> {
    with_extension(s, "kerf")
}

fn wav_file(s: &str) -> Result<PathBuf, String> {
    with_extension(s, "wav")
}

fn with_extension(s: &str, ext: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    {
        Ok(path)
    } else {
        Err(format!("{s} must end in .{ext}"))
    }
}

fn apply(patch_path: &Path, commands: &Path, out: Option<&Path>) -> Result<()> {
    let mut s = open(patch_path)?;

    let (name, lines) = if commands == Path::new("-") {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .context("reading stdin")?;
        ("stdin".to_owned(), buf)
    } else {
        let text = fs::read_to_string(commands)
            .with_context(|| format!("reading {}", commands.display()))?;
        (commands.display().to_string(), text)
    };
    for (i, line) in lines.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cmd: Command = serde_json::from_str(line)
            .with_context(|| format!("{name}:{}: not a command", i + 1))?;
        s.apply(cmd).with_context(|| format!("{name}:{}", i + 1))?;
    }
    emit(s.patch(), out, true)
}

fn describe(kind: Option<&str>, json: bool) -> Result<()> {
    let all = Builtin.all();
    let Some(kind) = kind else {
        if json {
            println!("{}", serde_json::to_string_pretty(all)?);
            return Ok(());
        }
        for band in Band::ALL {
            println!("{:<8}{}", band.name(), band.summary());
            for d in all.iter().filter(|d| d.band == band) {
                println!("  {:<12}{}", d.kind, d.summary);
            }
            println!();
        }
        println!("`kerf describe <Kind>` shows params, jacks and tooltips.");
        return Ok(());
    };
    let Some(d) = Builtin.get(kind) else {
        let names: Vec<_> = all.iter().map(|d| d.kind).collect();
        bail!("no module kind `{kind}`; known kinds: {}", names.join(", "));
    };
    if json {
        println!("{}", serde_json::to_string_pretty(d)?);
    } else {
        print_descriptor(d);
    }
    Ok(())
}

fn print_descriptor(d: &ModuleDescriptor) {
    println!(
        "{} · {} · {} band, {} hp",
        d.kind,
        d.summary,
        d.band.name(),
        d.hp
    );
    let w = d
        .params
        .iter()
        .map(|p| p.key.len())
        .chain(d.inputs.iter().chain(d.outputs).map(|j| j.key.len()))
        .max()
        .unwrap_or(0);
    if !d.params.is_empty() {
        println!("\nParams (0–1, default shown)");
        for p in d.params {
            println!("  {:<w$}  {:>4}  {}", p.key, p.default, p.tooltip);
        }
    }
    for (title, jacks) in [("Inputs", d.inputs), ("Outputs", d.outputs)] {
        if !jacks.is_empty() {
            println!("\n{title}");
            for j in jacks {
                println!("  {:<w$}  {:<5}  {}", j.key, j.kind.name(), j.tooltip);
            }
        }
    }
}
