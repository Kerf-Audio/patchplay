//! The lineage tree: every patch the generator commands produced or started
//! from, as snapshots. No timestamps, so a session replays identically.

use kerf_core::Patch;
use kerf_core::text::print;
use serde::{Serialize, Serializer};

/// How a node came to be.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    /// The patch the session opened with.
    Root,
    /// Hand edits, saved before a generator command moved on from them.
    Edited,
    /// A patch brought in as breeding stock.
    Imported,
    Random {
        style: String,
        seed: u64,
    },
    Evolve {
        strength: f32,
        seed: u64,
    },
    Breed {
        seed: u64,
    },
}

impl Origin {
    /// The seed a generator command ran with, whether given or defaulted.
    pub fn seed(&self) -> Option<u64> {
        match *self {
            Self::Random { seed, .. } | Self::Evolve { seed, .. } | Self::Breed { seed } => {
                Some(seed)
            }
            Self::Root | Self::Edited | Self::Imported => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Node {
    pub id: u32,
    /// One parent, or two for a breed. Empty only for the root.
    pub parents: Vec<u32>,
    pub origin: Origin,
    /// Serialised as `.kerf` text.
    #[serde(serialize_with = "as_text")]
    pub patch: Patch,
}

fn as_text<S: Serializer>(patch: &Patch, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&print(patch))
}

/// All nodes in creation order (a node's id is its index) and the one the
/// session currently stands on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lineage {
    pub nodes: Vec<Node>,
    pub current: u32,
}

impl Lineage {
    pub(crate) fn new(root: Patch) -> Self {
        Self {
            nodes: vec![Node {
                id: 0,
                parents: Vec::new(),
                origin: Origin::Root,
                patch: root,
            }],
            current: 0,
        }
    }

    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    pub fn current_node(&self) -> &Node {
        &self.nodes[self.current as usize]
    }

    /// Adds a node and returns its id. Does not move `current`.
    pub(crate) fn push(&mut self, parents: Vec<u32>, origin: Origin, patch: Patch) -> u32 {
        let id = u32::try_from(self.nodes.len()).expect("fewer than 2^32 nodes");
        self.nodes.push(Node {
            id,
            parents,
            origin,
            patch,
        });
        id
    }
}
