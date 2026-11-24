//! Tool definitions. Kinds and styles in the descriptions come from the
//! module descriptors and the generator's rulebooks, so they cannot drift.

use std::fmt::Write;

use kerf_core::Catalogue;
use kerf_modules::Builtin;
use serde_json::{Value, json};

pub const INSTRUCTIONS: &str = "Kerf Modular is a patchable modular synth. The server holds one \
patch. Read it with patch_get (`.kerf` text), change it with patch_apply, or start from a style \
with random, then evolve and breed it. Use check and render to judge it, play to hear it live.";

const COMMANDS: &str = "\
Each command is an object with an `op` field, plus:
add_part {id, name, colour:\"#rrggbb\"}
add_module {kind, id?, part?, auto_patch?: bool, near?: id}: auto_patch wires the new module sensibly (clock, pitch, audio in, output) as the same undo step; `near` wires it to that module and implies auto_patch. patch_apply reports the cables it made.
auto_patch {module, near?}: wires an existing module the same way; never overwrites a cable or makes a loop; nothing to add is not an error.
move_module {module, part: id|null, index?}
replace_text {text}: replace the whole patch with `.kerf` text; a parse error says line and column.
set_part_level {part, level: 0..1}: gain on audio leaving the part; 1 is unity, 0.5 is -6 dB.
remove_module {id}
set_param {module, param, value: 0..1}
set_params {module, params: [{param, value: 0..1}, ...]}: several params of one module as one undo step; any bad key changes nothing.
connect {from:\"module.jack\", to:\"module.jack\"} (output to input)
disconnect {from, to}
lock {target: {kind:\"module\", module} | {kind:\"param\", module, param} | {kind:\"part\", part}, locked}
mute {part, muted}
set_tempo {bpm}
set_sample {module, path?}: a Veneer plays this audio file (WAV, AIFF, FLAC, MP3, Ogg Vorbis) in 16 slices, and a Splinter grains it; omit path to clear it. Use an absolute path; a missing file falls back to the demo sample with a warning in render.
store_scene {scene} / clear_scene {scene}, scene is \"A\"|\"B\"|\"C\"|\"D\"
morph {a, b, t: 0..1}
undo / redo
random {style, seed} / evolve {strength: 0..1, seed} / breed {with: {node: n} | {patch: kerf text}, seed}
checkout {node}: make a lineage node the current patch (see the lineage tool).
Example: {\"op\":\"set_param\",\"module\":\"flt1\",\"param\":\"cutoff\",\"value\":0.4}.
Params are normalised 0..1. Use describe_module for params and jacks.";

fn tool(name: &str, description: &str, properties: &Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
    })
}

fn kinds() -> String {
    Builtin.all().iter().fold(String::new(), |mut s, d| {
        let _ = writeln!(s, "{}: {}", d.kind, d.summary);
        s
    })
}

fn styles() -> (Vec<String>, String) {
    let styles = kerf_gen::styles();
    let names = styles.iter().map(|s| s.name.clone()).collect();
    let text = styles.iter().fold(String::new(), |mut s, st| {
        let _ = writeln!(
            s,
            "{}: {}, {}-{} BPM. {}",
            st.name, st.title, st.tempo[0], st.tempo[1], st.summary
        );
        s
    });
    (names, text)
}

const SEED: &str = "Integer. Omit to use the number of lineage nodes so far: it differs from call to call, but a session replays the same way. The reply reports the seed used; the same seed on the same patch repeats the result.";

pub fn list() -> Value {
    let (style_names, style_text) = styles();
    let none = json!({});
    json!([
        tool(
            "patch_get",
            "The current patch as `.kerf` text.",
            &none,
            &[]
        ),
        tool(
            "patch_apply",
            &format!(
                "Apply a list of commands in order. All or none: if one fails the patch is \
                 unchanged and the error names the command. Returns the new `.kerf` text. A patch \
                 that is playing changes live.\n\n{COMMANDS}\n\nModule kinds:\n{}",
                kinds()
            ),
            &json!({ "commands": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "op": { "type": "string", "description": "The command name, e.g. set_param." } },
                    "required": ["op"],
                },
                "description": "Commands in order, each an object with an `op` field.",
            }}),
            &["commands"]
        ),
        tool(
            "random",
            &format!(
                "Replace the patch with a new one from a style, safe-checked. Returns the \
                 lineage node and the `.kerf` text.\n\nStyles:\n{style_text}"
            ),
            &json!({
                "style": { "type": "string", "enum": style_names },
                "seed": { "type": "integer", "minimum": 0, "description": SEED },
            }),
            &["style"]
        ),
        tool(
            "evolve",
            "Mutate the unlocked params and wiring of the current patch into a new lineage node. \
             Lock things first with patch_apply.",
            &json!({
                "strength": {
                    "type": "number", "minimum": 0, "maximum": 1, "default": 0.3,
                    "description": "0 changes nothing, 1 changes a lot.",
                },
                "seed": { "type": "integer", "minimum": 0, "description": SEED },
            }),
            &[]
        ),
        tool(
            "breed",
            "Cross the current patch with another, parameter by parameter, into a new lineage \
             node. To breed two earlier patches, checkout the first with patch_apply.",
            &json!({
                "with": {
                    "oneOf": [{ "type": "integer", "minimum": 0 }, { "type": "string" }],
                    "description": "A lineage node id, or a patch as `.kerf` text.",
                },
                "seed": { "type": "integer", "minimum": 0, "description": SEED },
            }),
            &["with"]
        ),
        tool(
            "lineage",
            "The tree of generated patches: id, parents, how each came about, and which is \
             current. Go back to one with patch_apply and {\"op\":\"checkout\",\"node\":id}.",
            &none,
            &[]
        ),
        tool(
            "render",
            "Render the current patch offline to a 16-bit stereo WAV. Returns the path, peak \
             and RMS in dBFS, and the module kinds that make no sound yet.",
            &json!({
                "seconds": {
                    "type": "number", "exclusiveMinimum": 0, "maximum": 120, "default": 8,
                    "description": "Length to render.",
                },
                "rate": {
                    "type": "integer", "minimum": 8000, "maximum": 192000, "default": 48000,
                    "description": "Sample rate in Hz.",
                },
                "path": { "type": "string", "description": "Where to write the WAV. Omit for a file in the temp folder." },
            }),
            &[]
        ),
        tool(
            "check",
            "Safe-check the current patch: render it and report peak, RMS, silence, DC, \
             clipping and runaway feedback as JSON. `pass` is false if any rule fails.",
            &none,
            &[]
        ),
        tool(
            "describe_module",
            "Params, jacks and tooltips for one module kind; without `kind`, every kind in one line.",
            &json!({ "kind": { "type": "string", "description": "A module kind, e.g. Plane. Omit to list them all." } }),
            &[]
        ),
        tool(
            "suggest_cables",
            "The cables auto-patching would add, best first, without changing the patch. Give \
             `module` (in the patch) or `kind` (a module not added yet); `near` limits it to \
             cables between that module and another; `part` is the part a new module would join.",
            &json!({
                "kind": { "type": "string", "description": "A module kind not in the patch yet." },
                "module": { "type": "string", "description": "A module id already in the patch." },
                "near": { "type": "string", "description": "A module id in the patch; only cables between it and `module` or `kind`." },
                "part": { "type": "string", "description": "The part id a new module would join." },
            }),
            &[]
        ),
        tool(
            "language_docs",
            "How to write `.kerf` starting points: the file format, worked examples, tips and the \
             module reference. Without `section`, lists the section ids.",
            &json!({ "section": { "type": "string", "description": "A section id from the list. Omit to list them." } }),
            &[]
        ),
        tool(
            "play",
            "Play the current patch on the default audio output (this opens the audio device, \
             the only tool that does). Later patch_apply, random, evolve and breed calls are \
             heard as they happen. Use stop to end it.",
            &none,
            &[]
        ),
        tool("stop", "Stop live playback.", &none, &[]),
    ])
}
