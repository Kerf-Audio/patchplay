//! patchplay core: the patch model, the command protocol shared by the
//! GUI, CLI and agents, module descriptors, and the `.kerf` text format.
//!
//! Pure data and logic: no I/O, no audio, no UI types.

pub mod command;
pub mod descriptor;
pub mod id;
pub mod patch;
pub mod text;

pub use command::{BreedWith, Command, LockTarget, ParamSet};
pub use descriptor::{
    Band, Catalogue, JackDescriptor, JackKind, ModuleDescriptor, ParamDescriptor,
};
pub use id::{Colour, IdError, JackRef, Key, ModuleId, PartId, Scene};
pub use patch::{Cable, Module, ParamValue, Part, Patch, SceneValue, StyleRef};
