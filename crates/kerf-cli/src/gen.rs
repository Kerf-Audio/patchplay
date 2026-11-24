//! The generator subcommands: `new --style`, `evolve`, `breed`, `check` and
//! `styles`. All go through `Session` commands like any other front end.

use std::path::Path;

use anyhow::{Context, Result, bail};
use kerf_core::text::print;
use kerf_core::{BreedWith, Command, LockTarget, Patch};
use kerf_modules::Builtin;
use kerf_session::Session;

use crate::{emit, open};

pub fn new(tempo: Option<f32>, style: Option<&str>, seed: u64, out: Option<&Path>) -> Result<()> {
    let mut s = Session::new(Builtin);
    if let Some(style) = style {
        s.apply(Command::Random {
            style: style.to_owned(),
            seed: Some(seed),
        })?;
    }
    if let Some(bpm) = tempo {
        s.apply(Command::SetTempo { bpm })?;
    }
    emit(s.patch(), out, false)
}

/// A part id, a module id, or `module.param`.
fn lock_target(patch: &Patch, name: &str) -> Result<LockTarget> {
    if let Some((module, param)) = name.split_once('.') {
        return Ok(LockTarget::Param {
            module: module.parse()?,
            param: param.parse()?,
        });
    }
    if let Some(part) = patch.parts.iter().find(|p| p.id.as_str() == name) {
        return Ok(LockTarget::Part {
            part: part.id.clone(),
        });
    }
    if let Some(m) = patch.modules.iter().find(|m| m.id.as_str() == name) {
        return Ok(LockTarget::Module {
            module: m.id.clone(),
        });
    }
    bail!("`{name}` is neither a part nor a module in the patch")
}

pub fn evolve(
    path: &Path,
    strength: f32,
    locks: &[String],
    seed: Option<u64>,
    out: Option<&Path>,
) -> Result<()> {
    let mut s = open(path)?;
    for name in locks {
        let target = lock_target(s.patch(), name)?;
        s.apply(Command::Lock {
            target,
            locked: true,
        })?;
    }
    s.apply(Command::Evolve { strength, seed })?;
    emit(s.patch(), out, true)
}

pub fn breed(a: &Path, b: &Path, seed: Option<u64>, out: Option<&Path>) -> Result<()> {
    let mut s = open(a)?;
    let other = open(b)?;
    s.apply(Command::Breed {
        with: BreedWith::Patch(print(other.patch())),
        seed,
    })?;
    emit(s.patch(), out, true)
}

/// Prints the report as JSON; exits 1 when the patch fails.
pub fn check(path: &Path) -> Result<()> {
    let s = open(path)?;
    let report = kerf_gen::safe_check(s.patch());
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("serialising the report")?
    );
    if !report.pass {
        std::process::exit(1);
    }
    Ok(())
}

pub fn styles() {
    for style in kerf_gen::styles() {
        let parts: Vec<String> = style
            .parts
            .iter()
            .map(|p| format!("{}{}", p.id, if p.optional { "?" } else { "" }))
            .collect();
        println!(
            "{:<12} {}-{} bpm  parts: {}\n             {}",
            style.name,
            style.tempo[0],
            style.tempo[1],
            parts.join(", "),
            style.summary
        );
    }
}
