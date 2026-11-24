//! The one command protocol. The GUI, the CLI and agents (MCP) all change a
//! patch only by sending these; the session validates and applies them.
//!
//! Wire format is JSON with an `op` tag, e.g.
//! `{"op":"set_param","module":"flt1","param":"cutoff","value":0.4}`.
//! Generator commands (random, evolve, breed, checkout) are run by
//! `kerf-session` through `kerf-gen`; transport arrives with `kerf-audio` in
//! phase 4.

use serde::{Deserialize, Serialize};

use crate::id::{Colour, JackRef, Key, ModuleId, PartId, Scene};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    AddPart {
        id: PartId,
        name: String,
        colour: Colour,
    },
    /// Adds a module with default params. Without `id`, one is made from the
    /// kind, e.g. `plane1`. With `auto_patch` (or `near`), the new module is
    /// also wired into the patch (see `AutoPatch`) in the same undo step.
    AddModule {
        kind: String,
        #[serde(default)]
        id: Option<ModuleId>,
        #[serde(default)]
        part: Option<PartId>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        auto_patch: bool,
        /// Wire to this module first, then to the rest of the patch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        near: Option<ModuleId>,
    },
    /// Wires an existing module into the patch with sensible cables: clock,
    /// gate, pitch and audio inputs from the sources that fit, audio outputs
    /// into the first free mixer channel (or the master). Never replaces a
    /// cable and never closes a feedback loop; running it twice adds nothing.
    /// With `near`, cables to that module come first.
    AutoPatch {
        module: ModuleId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        near: Option<ModuleId>,
    },
    /// Moves a module into `part` (or out of every part when `part` is null)
    /// at position `index` among that part's modules; without `index` it goes
    /// last. Cables and params are untouched.
    MoveModule {
        module: ModuleId,
        #[serde(default)]
        part: Option<PartId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    /// Replaces the whole patch with the one written in `.kerf` text, as one
    /// undoable step. Scenes and locks are taken from the text.
    ReplaceText {
        text: String,
    },
    /// Removes the module, its cables and its scene values.
    RemoveModule {
        id: ModuleId,
    },
    /// `value` is normalised and clamped to `0.0..=1.0`.
    SetParam {
        module: ModuleId,
        param: Key,
        value: f32,
    },
    /// Sets several params of one module as one undo step, e.g. a stroke on a
    /// piano roll. Every key is checked first: on any error nothing changes.
    /// Values are normalised and clamped as for `SetParam`.
    SetParams {
        module: ModuleId,
        params: Vec<ParamSet>,
    },
    /// Gives a Veneer or Splinter an audio file, or clears it with `null`. The path is
    /// stored as given: absolute, or relative to the `.kerf` file.
    SetSample {
        module: ModuleId,
        #[serde(default)]
        path: Option<String>,
    },
    Connect {
        from: JackRef,
        to: JackRef,
    },
    Disconnect {
        from: JackRef,
        to: JackRef,
    },
    /// Locked things are left alone by evolve, breed and morph.
    Lock {
        target: LockTarget,
        locked: bool,
    },
    Mute {
        part: PartId,
        muted: bool,
    },
    /// The part's level, `0.0..=1.0`: the gain on audio leaving the part for
    /// the rest of the patch. 1.0 is unity, 0.5 is half the amplitude (-6 dB).
    SetPartLevel {
        part: PartId,
        level: f32,
    },
    SetTempo {
        bpm: f32,
    },
    StoreScene {
        scene: Scene,
    },
    ClearScene {
        scene: Scene,
    },
    /// Sets every unlocked param to `a + (b - a) * t`.
    Morph {
        a: Scene,
        b: Scene,
        t: f32,
    },
    Undo,
    Redo,
    /// A new patch from a style rulebook (see `kerf styles`), safe-checked.
    /// Without `seed` the session picks one: the number of lineage nodes so far.
    Random {
        style: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seed: Option<u64>,
    },
    /// Mutates the unlocked params and wiring; `strength` is `0.0..=1.0`.
    /// `seed` defaults as for `Random`.
    Evolve {
        strength: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seed: Option<u64>,
    },
    /// Crosses the current patch with another, parameter by parameter.
    /// `seed` defaults as for `Random`.
    Breed {
        with: BreedWith,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seed: Option<u64>,
    },
    /// Makes a lineage node the current patch again.
    Checkout {
        node: u32,
    },
}

/// One param and its new value, in a `SetParams`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamSet {
    pub param: Key,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LockTarget {
    Module { module: ModuleId },
    Param { module: ModuleId, param: Key },
    Part { part: PartId },
}

/// The other parent of a `Breed`: a lineage node, or a patch as `.kerf` text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreedWith {
    Node(u32),
    Patch(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_is_flat_json_with_op_tag() {
        let cmd: Command = serde_json::from_str(
            r#"{"op":"set_param","module":"flt1","param":"cutoff","value":0.4}"#,
        )
        .unwrap();
        assert_eq!(
            cmd,
            Command::SetParam {
                module: "flt1".parse().unwrap(),
                param: "cutoff".parse().unwrap(),
                value: 0.4
            }
        );
        let lock: Command = serde_json::from_str(
            r#"{"op":"lock","target":{"kind":"part","part":"drums"},"locked":true}"#,
        )
        .unwrap();
        assert!(matches!(
            lock,
            Command::Lock {
                target: LockTarget::Part { .. },
                locked: true
            }
        ));
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"op":"undo"}"#).unwrap(),
            Command::Undo
        );
    }

    #[test]
    fn set_params_wire_format_lists_param_and_value_pairs() {
        let cmd: Command = serde_json::from_str(
            r#"{"op":"set_params","module":"inl1","params":[{"param":"step1","value":0.5}]}"#,
        )
        .unwrap();
        assert_eq!(
            cmd,
            Command::SetParams {
                module: "inl1".parse().unwrap(),
                params: vec![ParamSet {
                    param: "step1".parse().unwrap(),
                    value: 0.5
                }],
            }
        );
    }

    #[test]
    fn generator_commands_have_flat_json_shapes() {
        let cases = [
            (
                r#"{"op":"random","style":"foundry","seed":7}"#,
                Command::Random {
                    style: "foundry".into(),
                    seed: Some(7),
                },
            ),
            (
                r#"{"op":"evolve","strength":0.3,"seed":2}"#,
                Command::Evolve {
                    strength: 0.3,
                    seed: Some(2),
                },
            ),
            (
                r#"{"op":"breed","with":{"node":3},"seed":9}"#,
                Command::Breed {
                    with: BreedWith::Node(3),
                    seed: Some(9),
                },
            ),
            (
                r#"{"op":"breed","with":{"patch":"kerf 1\n"}}"#,
                Command::Breed {
                    with: BreedWith::Patch("kerf 1\n".into()),
                    seed: None,
                },
            ),
            (
                r#"{"op":"checkout","node":4}"#,
                Command::Checkout { node: 4 },
            ),
        ];
        for (json, cmd) in cases {
            assert_eq!(serde_json::from_str::<Command>(json).unwrap(), cmd);
            assert_eq!(serde_json::to_string(&cmd).unwrap(), json);
        }
    }

    #[test]
    fn patching_commands_have_flat_json_shapes() {
        let add = Command::AddModule {
            kind: "Mortise".into(),
            id: None,
            part: None,
            auto_patch: true,
            near: Some("ruler1".parse().unwrap()),
        };
        let json = r#"{"op":"add_module","kind":"Mortise","id":null,"part":null,"auto_patch":true,"near":"ruler1"}"#;
        assert_eq!(serde_json::from_str::<Command>(json).unwrap(), add);
        // Older callers send neither field.
        let old: Command = serde_json::from_str(r#"{"op":"add_module","kind":"Plane"}"#).unwrap();
        assert!(matches!(
            old,
            Command::AddModule {
                auto_patch: false,
                near: None,
                ..
            }
        ));
        let cases = [
            (
                r#"{"op":"auto_patch","module":"kick"}"#,
                Command::AutoPatch {
                    module: "kick".parse().unwrap(),
                    near: None,
                },
            ),
            (
                r#"{"op":"move_module","module":"kick","part":"drums","index":1}"#,
                Command::MoveModule {
                    module: "kick".parse().unwrap(),
                    part: Some("drums".parse().unwrap()),
                    index: Some(1),
                },
            ),
            (
                r#"{"op":"replace_text","text":"kerf 1\n"}"#,
                Command::ReplaceText {
                    text: "kerf 1\n".into(),
                },
            ),
        ];
        for (json, cmd) in cases {
            assert_eq!(serde_json::from_str::<Command>(json).unwrap(), cmd);
            assert_eq!(serde_json::to_string(&cmd).unwrap(), json);
        }
        let loose: Command =
            serde_json::from_str(r#"{"op":"move_module","module":"kick","part":null}"#).unwrap();
        assert!(matches!(
            loose,
            Command::MoveModule {
                part: None,
                index: None,
                ..
            }
        ));
    }

    #[test]
    fn bad_ids_and_unknown_fields_fail_at_the_wire() {
        assert!(serde_json::from_str::<Command>(r#"{"op":"remove_module","id":"Osc 1"}"#).is_err());
        assert!(
            serde_json::from_str::<Command>(r#"{"op":"connect","from":"osc1","to":"flt1.in"}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<Command>(r#"{"op":"set_tempo","bpm":120,"swing":1}"#).is_err()
        );
    }
}
