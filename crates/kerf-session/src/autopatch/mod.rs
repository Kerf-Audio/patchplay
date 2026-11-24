//! Auto-patching: working out which cables a module wants.
//!
//! One planner serves two callers. `AutoPatch` (and `AddModule` with
//! `auto_patch` or `near`) runs it on the real patch and keeps the cables;
//! `Session::suggest_cables` runs it on a copy and only reports them, so what
//! the GUI previews is exactly what the command then does.
//!
//! For a module M the planner takes three steps, each only filling jacks that
//! have no cable yet and never closing a feedback loop:
//!
//! 1. M's own inputs pull the best source in the patch (`rules::score`).
//! 2. Other modules' empty inputs claim M when M is their best source.
//! 3. M's audio goes to the first free mixer channel, or to the master.
//!
//! With `near`, all three run first restricted to that one module (so a
//! drum dropped next to a rhythm lane takes that lane), then, unless the
//! caller only wants the near cables, once more over the whole patch.

mod rules;

use kerf_core::{Cable, Catalogue, JackDescriptor, JackKind, JackRef, Key, ModuleDescriptor, ModuleId, Patch};
use serde::Serialize;

use crate::SessionError as E;
use rules::Need;

/// One cable the planner wants, and why, in plain words.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedCable {
    pub from: JackRef,
    pub to: JackRef,
    pub reason: String,
    /// How good the match is; only the order matters.
    pub score: f32,
}

/// Which modules may take part in a step.
#[derive(Clone, Copy)]
enum Only<'a> {
    Anyone,
    Module(&'a ModuleId),
}

impl Only<'_> {
    fn allows(self, id: &ModuleId) -> bool {
        match self {
            Only::Anyone => true,
            Only::Module(m) => m == id,
        }
    }
}

struct Pick {
    from: JackRef,
    score: f32,
    reason: String,
}

/// Plans (and adds to `patch`) the cables for `module`. With `near_only`,
/// only the cables between `module` and `near` are planned.
pub(crate) fn plan(
    cat: &impl Catalogue,
    patch: &mut Patch,
    module: &ModuleId,
    near: Option<&ModuleId>,
    near_only: bool,
) -> Result<Vec<PlannedCable>, E> {
    for id in std::iter::once(module).chain(near) {
        if patch.module(id).is_none() {
            return Err(E::UnknownModule(id.clone()));
        }
    }
    let mut p = Planner {
        cat,
        patch,
        plan: Vec::new(),
    };
    let near = near.filter(|n| *n != module);
    if let Some(n) = near {
        p.wire(module, Only::Module(n));
    }
    if !(near.is_some() && near_only) {
        p.wire(module, Only::Anyone);
    }
    Ok(p.plan)
}

struct Planner<'a, C> {
    cat: &'a C,
    patch: &'a mut Patch,
    plan: Vec<PlannedCable>,
}

impl<C: Catalogue> Planner<'_, C> {
    fn wire(&mut self, m: &ModuleId, only: Only) {
        self.pull_inputs(m, only);
        self.pull_orphans(m, only);
        self.sink(m, only);
        if matches!(only, Only::Anyone) {
            self.ensure_master();
        }
    }

    fn desc(&self, id: &ModuleId) -> Option<&'static ModuleDescriptor> {
        self.patch.module(id).and_then(|m| self.cat.get(&m.kind))
    }

    fn ids(&self) -> Vec<ModuleId> {
        self.patch.modules.iter().map(|m| m.id.clone()).collect()
    }

    fn fed(&self, m: &ModuleId, jack: &str) -> bool {
        self.patch
            .cables
            .iter()
            .any(|c| &c.to.module == m && c.to.jack.as_str() == jack)
    }

    fn used(&self, m: &ModuleId, jack: &str) -> bool {
        self.patch
            .cables
            .iter()
            .any(|c| &c.from.module == m && c.from.jack.as_str() == jack)
    }

    /// Whether anything leaves `m` through one of the `jacks`.
    fn any_used<'j>(&self, m: &ModuleId, mut jacks: impl Iterator<Item = &'j JackDescriptor>) -> bool {
        jacks.any(|j| self.used(m, j.key))
    }

    /// True when a cable path leads from `from` to `to` (or they are the same
    /// module), so a cable from `to` back into `from` would close a loop.
    fn reaches(&self, from: &ModuleId, to: &ModuleId) -> bool {
        let mut seen = vec![from];
        let mut i = 0;
        while let Some(&at) = seen.get(i) {
            if at == to {
                return true;
            }
            for c in &self.patch.cables {
                if &c.from.module == at && !seen.contains(&&c.to.module) {
                    seen.push(&c.to.module);
                }
            }
            i += 1;
        }
        false
    }

    fn jack(module: &ModuleId, key: &str) -> JackRef {
        JackRef::new(module.clone(), Key::new(key).expect("catalogue keys are valid"))
    }

    fn connect(&mut self, from: JackRef, to: JackRef, reason: impl Into<String>, score: f32) -> bool {
        let cable = Cable {
            from: from.clone(),
            to: to.clone(),
        };
        if self.patch.cables.contains(&cable) || self.reaches(&to.module, &from.module) {
            return false;
        }
        self.patch.cables.push(cable);
        self.plan.push(PlannedCable {
            from,
            to,
            reason: reason.into(),
            score,
        });
        true
    }

    /// Step 1: fill each empty input of `m` from the best source.
    fn pull_inputs(&mut self, m: &ModuleId, only: Only) {
        let Some(d) = self.desc(m) else { return };
        match d.kind {
            "Router" => return self.gather_router(m, only),
            "Finish" => return self.gather_finish(m, only),
            _ => {}
        }
        for j in d.inputs {
            let Some(need) = rules::need(d, j) else { continue };
            if self.fed(m, j.key) {
                continue;
            }
            if let Some(pick) = self.best(m, d, j, need, only) {
                self.connect(pick.from, Self::jack(m, j.key), pick.reason, pick.score);
            }
        }
    }

    /// Step 2: other modules' empty inputs that `m` serves best.
    fn pull_orphans(&mut self, m: &ModuleId, only: Only) {
        for o in self.ids() {
            if o == *m || !only.allows(&o) {
                continue;
            }
            let Some(d) = self.desc(&o) else { continue };
            if matches!(d.kind, "Router" | "Finish") {
                continue;
            }
            // Next to a chosen module, any plausible cable from `m` will do;
            // over the whole patch `m` must be the best source there is.
            let sources = match only {
                Only::Module(_) => Only::Module(m),
                Only::Anyone => Only::Anyone,
            };
            for j in d.inputs {
                let Some(need) = rules::need(d, j) else { continue };
                if self.fed(&o, j.key) {
                    continue;
                }
                if let Some(pick) = self.best(&o, d, j, need, sources) {
                    if pick.from.module == *m {
                        self.connect(pick.from, Self::jack(&o, j.key), pick.reason, pick.score);
                    }
                }
            }
        }
    }

    /// The best source for one input, or `None` when nothing fits.
    fn best(
        &self,
        t: &ModuleId,
        td: &'static ModuleDescriptor,
        jack: &JackDescriptor,
        need: Need,
        sources: Only,
    ) -> Option<Pick> {
        if let Some(pick) = self.stereo_partner(t, td, jack) {
            return sources.allows(&pick.from.module).then_some(pick);
        }
        let tm = self.patch.module(t)?;
        let mut best: Option<Pick> = None;
        for s in &self.patch.modules {
            if s.id == *t || !sources.allows(&s.id) || self.reaches(t, &s.id) {
                continue;
            }
            let Some(sd) = self.cat.get(&s.kind) else { continue };
            for (idx, out) in sd.outputs.iter().enumerate() {
                let Some((score, reason)) = self.score(s, sd, idx, out, tm, td, need) else {
                    continue;
                };
                if best.as_ref().is_none_or(|b| score > b.score) {
                    best = Some(Pick {
                        from: Self::jack(&s.id, out.key),
                        score,
                        reason,
                    });
                }
            }
        }
        best
    }

    /// A stereo `in_r` follows whatever feeds `in_l`: its `r` output if it has
    /// one, otherwise the same output again (a mono send feeds both sides).
    fn stereo_partner(&self, t: &ModuleId, td: &ModuleDescriptor, jack: &JackDescriptor) -> Option<Pick> {
        if jack.key != "in_r" || td.input("in_l").is_none() {
            return None;
        }
        let left = self
            .patch
            .cables
            .iter()
            .find(|c| &c.to.module == t && c.to.jack.as_str() == "in_l")?;
        let has_r = self.desc(&left.from.module)?.output("r").is_some();
        let from = if has_r {
            Self::jack(&left.from.module, "r")
        } else {
            left.from.clone()
        };
        Some(Pick {
            from,
            score: 20.0,
            reason: "the right side follows the left".into(),
        })
    }

    /// The audio output(s) that stand for the whole module when it is wired
    /// into a mixer: `l` and `r` for a stereo module, else the first audio
    /// output. Empty when the module makes no audio.
    fn ends(d: &ModuleDescriptor) -> Vec<&'static str> {
        let audio: Vec<_> = d
            .outputs
            .iter()
            .filter(|o| o.kind == JackKind::Audio)
            .map(|o| o.key)
            .collect();
        if audio.contains(&"l") && audio.contains(&"r") {
            vec!["l", "r"]
        } else {
            audio.into_iter().take(1).collect()
        }
    }

    /// Step 3: send the module's audio to a mixer channel or the master.
    fn sink(&mut self, m: &ModuleId, only: Only) {
        let Some(d) = self.desc(m) else { return };
        let ends = Self::ends(d);
        let audio = d.outputs.iter().filter(|o| o.kind == JackKind::Audio);
        if matches!(d.kind, "Router" | "Finish") || ends.is_empty() || self.any_used(m, audio) {
            return;
        }
        // A reverb returns to the master; its input is what the mixer feeds.
        if d.kind != "Kiln" {
            for r in self.ids().into_iter().filter(|r| only.allows(r)) {
                let Some(rd) = self.desc(&r).filter(|rd| rd.kind == "Router") else { continue };
                let Some(channel) = rd
                    .inputs
                    .iter()
                    .find(|j| j.kind == JackKind::Audio && !self.fed(&r, j.key))
                else {
                    continue;
                };
                let mut any = false;
                for end in &ends {
                    any |= self.connect(
                        Self::jack(m, end),
                        Self::jack(&r, channel.key),
                        "the sound goes to the mixer",
                        5.0,
                    );
                }
                if any {
                    return;
                }
            }
        }
        for f in self.ids().into_iter().filter(|f| only.allows(f)) {
            if self.desc(&f).is_some_and(|fd| fd.kind == "Finish") {
                self.wire_to_finish(m, &ends, &f);
            }
        }
    }

    /// Wires `ends` into the master's `l` and `r` (a mono end feeds both).
    fn wire_to_finish(&mut self, m: &ModuleId, ends: &[&str], finish: &ModuleId) {
        for (i, side) in ["l", "r"].into_iter().enumerate() {
            let end = ends.get(i).or(ends.first()).copied().unwrap_or("out");
            self.connect(
                Self::jack(m, end),
                Self::jack(finish, side),
                "the sound goes to the master",
                4.0,
            );
        }
    }

    /// Audio ends nothing listens to yet, in patch order: what a new mixer or
    /// master should collect.
    fn loose_ends(&self, except: &ModuleId, only: Only) -> Vec<(ModuleId, Vec<&'static str>)> {
        self.ids()
            .into_iter()
            .filter(|id| id != except && only.allows(id))
            .filter_map(|id| {
                let d = self.desc(&id)?;
                let ends = Self::ends(d);
                let audio = d.outputs.iter().filter(|o| o.kind == JackKind::Audio);
                let skip = matches!(d.kind, "Router" | "Finish" | "Kiln");
                (!skip && !ends.is_empty() && !self.any_used(&id, audio)).then_some((id, ends))
            })
            .collect()
    }

    fn gather_router(&mut self, router: &ModuleId, only: Only) {
        let Some(d) = self.desc(router) else { return };
        for (id, ends) in self.loose_ends(router, only) {
            let Some(channel) = d
                .inputs
                .iter()
                .find(|j| j.kind == JackKind::Audio && !self.fed(router, j.key))
            else {
                return;
            };
            for end in ends {
                self.connect(
                    Self::jack(&id, end),
                    Self::jack(router, channel.key),
                    "the sound goes to the mixer",
                    5.0,
                );
            }
        }
    }

    fn gather_finish(&mut self, finish: &ModuleId, only: Only) {
        let routers: Vec<_> = self
            .ids()
            .into_iter()
            .filter(|r| only.allows(r) && self.desc(r).is_some_and(|d| d.kind == "Router"))
            .collect();
        if routers.is_empty() {
            for (id, ends) in self.loose_ends(finish, only) {
                self.wire_to_finish(&id, &ends, finish);
            }
        }
        for r in routers {
            self.mixer_to_master(&r, finish);
        }
    }

    /// Mixer outputs into the master's `l` and `r`, when both are unused.
    fn mixer_to_master(&mut self, router: &ModuleId, finish: &ModuleId) {
        if self.used(router, "mix_l") || self.used(router, "mix_r") {
            return;
        }
        for (out, side) in [("mix_l", "l"), ("mix_r", "r")] {
            if !self.fed(finish, side) {
                self.connect(
                    Self::jack(router, out),
                    Self::jack(finish, side),
                    "the mixer feeds the master",
                    4.0,
                );
            }
        }
    }

    /// A mixer that was just given a channel and has nowhere to go feeds the
    /// first master with both sides free.
    fn ensure_master(&mut self) {
        let touched: Vec<ModuleId> = self
            .plan
            .iter()
            .map(|c| c.to.module.clone())
            .filter(|m| self.desc(m).is_some_and(|d| d.kind == "Router"))
            .collect();
        for r in touched {
            let finish = self.ids().into_iter().find(|f| {
                self.desc(f).is_some_and(|d| d.kind == "Finish")
                    && !self.fed(f, "l")
                    && !self.fed(f, "r")
            });
            if let Some(f) = finish {
                self.mixer_to_master(&r, &f);
            }
        }
    }
}
