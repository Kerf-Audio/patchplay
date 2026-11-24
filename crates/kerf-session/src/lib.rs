//! The session owns one patch. Front ends (GUI, CLI, MCP) change it only by
//! sending `Command`s; each is validated against the catalogue and either
//! applied whole or rejected with the patch untouched.

use std::collections::VecDeque;

use kerf_core::{BreedWith, Catalogue, Command, JackRef, Key, ModuleId, PartId, Patch, Scene};
use kerf_gen::GenError;

mod autopatch;
mod edit;
mod language;
mod lineage;
mod snippets;
mod validate;

pub use autopatch::PlannedCable;
pub use language::{Completions, LanguageDocs, Section, completions, fenced_blocks, format_guide, language_docs, writing_guide};
pub use snippets::{CATEGORIES, Category, Snippet, doc_files, snippets};
pub use lineage::{Lineage, Node, Origin};

#[cfg(test)]
mod autopatch_tests;
#[cfg(test)]
mod lineage_tests;
#[cfg(test)]
mod snippets_tests;
#[cfg(test)]
mod tests;

/// How many steps `undo` can go back.
pub const UNDO_LIMIT: usize = 256;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SessionError {
    #[error("no module kind `{0}`; `kerf describe` lists them")]
    UnknownKind(String),
    #[error("no module `{0}` in the patch")]
    UnknownModule(ModuleId),
    #[error("no part `{0}` in the patch")]
    UnknownPart(PartId),
    #[error("{kind} has no param `{param}`")]
    UnknownParam { kind: String, param: Key },
    #[error("`{0}` is not an output")]
    NotAnOutput(JackRef),
    #[error("`{0}` is not an input")]
    NotAnInput(JackRef),
    #[error("module `{0}` already exists")]
    DuplicateModule(ModuleId),
    #[error("part `{0}` already exists")]
    DuplicatePart(PartId),
    #[error("cable {0} -> {1} already exists")]
    DuplicateCable(JackRef, JackRef),
    #[error("no cable {0} -> {1}")]
    NoSuchCable(JackRef, JackRef),
    #[error("scene {0} is empty; store it first")]
    EmptyScene(Scene),
    #[error("{what} {value} is outside {range}")]
    OutOfRange {
        what: &'static str,
        value: f32,
        range: &'static str,
    },
    #[error("part name {0:?} must not be empty or contain quotes or line breaks")]
    BadName(String),
    #[error("module `{0}` is a {1}; only a Veneer or Splinter plays a sample")]
    NoSample(ModuleId, String),
    #[error("sample path {0:?} must not be empty or contain quotes or line breaks")]
    BadSamplePath(String),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("no lineage node {0}")]
    UnknownNode(u32),
    #[error("cannot read the patch to breed with: {0}")]
    BadPatch(String),
    /// `.kerf` text that cannot be applied; the message starts with
    /// `line L, column C:`.
    #[error("{0}")]
    BadText(String),
    #[error("{0}")]
    BadRequest(String),
    #[error(transparent)]
    Gen(#[from] GenError),
}

/// What `Session::suggest_cables` is asked: a module in the patch (`module`)
/// or one not added yet (`kind`), optionally next to another module, and for a
/// new module the part it would join.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Suggest {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub module: Option<ModuleId>,
    #[serde(default)]
    pub near: Option<ModuleId>,
    #[serde(default)]
    pub part: Option<PartId>,
}

/// A restorable state: the patch and the lineage node it was standing on.
#[derive(Clone)]
struct Snapshot {
    patch: Patch,
    node: u32,
}

/// Cloning is how a front end tries a batch of commands: apply them to a copy
/// and keep it only if all succeed.
#[derive(Clone)]
pub struct Session<C> {
    catalogue: C,
    patch: Patch,
    lineage: Lineage,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    last_plan: Vec<PlannedCable>,
}

impl<C: Catalogue> Session<C> {
    /// A session on an empty patch.
    pub fn new(catalogue: C) -> Self {
        Self::start(catalogue, Patch::default())
    }

    /// Opens a patch (from text or JSON), checking every kind, param, cable and
    /// scene against the catalogue. Missing params get their defaults; module
    /// kinds and param order follow the catalogue.
    pub fn open(catalogue: C, mut patch: Patch) -> Result<Self, SessionError> {
        validate::normalise(&catalogue, &mut patch)?;
        Ok(Self::start(catalogue, patch))
    }

    fn start(catalogue: C, patch: Patch) -> Self {
        Self {
            catalogue,
            lineage: Lineage::new(patch.clone()),
            patch,
            undo: VecDeque::new(),
            redo: Vec::new(),
            last_plan: Vec::new(),
        }
    }

    pub fn patch(&self) -> &Patch {
        &self.patch
    }

    pub fn catalogue(&self) -> &C {
        &self.catalogue
    }

    /// The tree of generated patches. Serialises to JSON.
    pub fn lineage(&self) -> &Lineage {
        &self.lineage
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The cables the last command added by auto-patching (`AutoPatch`, or
    /// `AddModule` with `auto_patch` or `near`); empty after any other
    /// command, including undo.
    pub fn last_plan(&self) -> &[PlannedCable] {
        &self.last_plan
    }

    /// The cables auto-patching would add, without adding them: exactly what
    /// `AutoPatch` (or `AddModule`) would do right now, best first.
    pub fn suggest_cables(&self, q: &Suggest) -> Result<Vec<PlannedCable>, SessionError> {
        let mut patch = self.patch.clone();
        let id = match (&q.kind, &q.module) {
            (Some(kind), None) => {
                // The module that is not in the patch yet gets a placeholder id.
                let id = (1..)
                    .map(|n| match n {
                        1 => "new".to_owned(),
                        n => format!("new{n}"),
                    })
                    .map(|s| ModuleId::new(s).expect("valid id"))
                    .find(|id| patch.module(id).is_none())
                    .expect("an unused id exists");
                let add = Command::AddModule {
                    kind: kind.clone(),
                    id: Some(id.clone()),
                    part: q.part.clone(),
                    auto_patch: false,
                    near: None,
                };
                edit::edit(&self.catalogue, &mut patch, add)?;
                id
            }
            (None, Some(module)) => module.clone(),
            _ => {
                return Err(SessionError::BadRequest(
                    "give exactly one of `kind` (a module not added yet) and `module`".into(),
                ));
            }
        };
        let near = q.near.as_ref();
        let mut plan = autopatch::plan(&self.catalogue, &mut patch, &id, near, near.is_some())?;
        plan.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(plan)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            patch: self.patch.clone(),
            node: self.lineage.current,
        }
    }

    fn restore(&mut self, to: Snapshot) -> Snapshot {
        let from = self.snapshot();
        self.patch = to.patch;
        self.lineage.current = to.node;
        from
    }

    /// Applies one command. On error the patch, the lineage and the undo
    /// history are unchanged.
    pub fn apply(&mut self, cmd: Command) -> Result<(), SessionError> {
        self.last_plan.clear();
        match cmd {
            Command::Undo => {
                let prev = self.undo.pop_back().ok_or(SessionError::NothingToUndo)?;
                let from = self.restore(prev);
                self.redo.push(from);
            }
            Command::Redo => {
                let next = self.redo.pop().ok_or(SessionError::NothingToRedo)?;
                let from = self.restore(next);
                self.undo.push_back(from);
            }
            Command::Random { .. }
            | Command::Evolve { .. }
            | Command::Breed { .. }
            | Command::Checkout { .. } => {
                let before = self.snapshot();
                let lineage = self.lineage.clone();
                if let Err(e) = self.generate(cmd) {
                    self.patch = before.patch;
                    self.lineage = lineage;
                    return Err(e);
                }
                self.record(before);
            }
            cmd => {
                let patching = matches!(cmd, Command::AutoPatch { .. });
                let mut next = self.patch.clone();
                let plan = edit::edit(&self.catalogue, &mut next, cmd)?;
                // An auto-patch with nothing to add is not a step to undo.
                if patching && next == self.patch {
                    return Ok(());
                }
                let before = self.snapshot();
                self.patch = next;
                self.last_plan = plan;
                self.record(before);
            }
        }
        Ok(())
    }

    fn record(&mut self, before: Snapshot) {
        if self.undo.len() == UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(before);
        self.redo.clear();
    }

    /// Runs a generator command: saves hand edits as a node, computes the new
    /// patch, files it in the lineage and makes it current.
    fn generate(&mut self, cmd: Command) -> Result<(), SessionError> {
        // Taken before the command adds any node, so it is the same however a
        // breed's second parent arrives.
        let default_seed = self.lineage.nodes.len() as u64;
        let (patch, origin, mate) = match cmd {
            Command::Checkout { node } => {
                let patch = self
                    .lineage
                    .node(node)
                    .ok_or(SessionError::UnknownNode(node))?
                    .patch
                    .clone();
                self.save_edits();
                self.lineage.current = node;
                self.patch = patch;
                return Ok(());
            }
            Command::Random { style, seed } => {
                let seed = seed.unwrap_or(default_seed);
                (
                    kerf_gen::random(&style, seed)?,
                    Origin::Random { style, seed },
                    None,
                )
            }
            Command::Evolve { strength, seed } => {
                let seed = seed.unwrap_or(default_seed);
                (
                    kerf_gen::evolve(&self.patch, strength, seed)?,
                    Origin::Evolve { strength, seed },
                    None,
                )
            }
            Command::Breed { with, seed } => {
                let seed = seed.unwrap_or(default_seed);
                let (mate, node) = match with {
                    BreedWith::Node(n) => {
                        let node = self.lineage.node(n).ok_or(SessionError::UnknownNode(n))?;
                        (node.patch.clone(), n)
                    }
                    BreedWith::Patch(text) => {
                        let mut mate = kerf_core::text::parse(&text)
                            .map_err(|e| SessionError::BadPatch(e.to_string()))?;
                        validate::normalise(&self.catalogue, &mut mate)?;
                        let node = self
                            .lineage
                            .push(Vec::new(), Origin::Imported, mate.clone());
                        (mate, node)
                    }
                };
                (
                    kerf_gen::breed(&self.patch, &mate, seed)?,
                    Origin::Breed { seed },
                    Some(node),
                )
            }
            _ => unreachable!("only generator commands reach here"),
        };
        let mut patch = patch;
        validate::normalise(&self.catalogue, &mut patch)?;
        let parents = std::iter::once(self.save_edits()).chain(mate).collect();
        self.lineage.current = self.lineage.push(parents, origin, patch.clone());
        self.patch = patch;
        Ok(())
    }

    /// If the working patch has moved on from the current node, files it as an
    /// `Edited` node and stands on that. Returns the node to descend from.
    fn save_edits(&mut self) -> u32 {
        if self.patch != self.lineage.current_node().patch {
            let id = self.lineage.push(
                vec![self.lineage.current],
                Origin::Edited,
                self.patch.clone(),
            );
            self.lineage.current = id;
        }
        self.lineage.current
    }
}
