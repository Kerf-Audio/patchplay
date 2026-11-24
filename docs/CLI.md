# The `kerf` command-line tool

`kerf` builds, changes and renders patches from a terminal or a script. It uses the same commands as
the app and the [MCP server](MCP.md), so anything you do here you can do there. Patches are plain
`.kerf` text files; see [KERF-FORMAT.md](KERF-FORMAT.md).

## Build

You need Rust (stable). From the repository root:

```sh
cargo build --release -p kerf-cli
target/release/kerf --help
```

The binary is `target/release/kerf`. The examples below assume it is on your `PATH`. `kerf --version`
prints the version (`kerf 0.1.0` at the time of writing).

## Commands

| Command | What it does |
|---|---|
| `kerf new` | Write a new patch: empty, or generated from a style. |
| `kerf describe` | List module kinds, or show one kind's params and jacks. |
| `kerf apply` | Apply JSON commands to a patch and print the result. |
| `kerf render` | Render a patch to a WAV file. |
| `kerf evolve` | Mutate a patch. |
| `kerf breed` | Cross two patches. |
| `kerf check` | Render a patch and report problems as JSON. |
| `kerf styles` | List the generator styles. |
| `kerf play` | Play a patch live. |
| `kerf mcp` | Run the MCP server on stdio. |

`kerf help <command>` or `kerf <command> --help` prints the options for one command.

### Conventions

- A **patch** argument is the path of a `.kerf` file. The starting points in `presets/` are `.kerf`
  files too, so a preset is referenced by its path: `kerf render presets/first-cut.kerf -o out.wav`.
  There is no preset name lookup.
- Commands that produce a patch (`new`, `apply`, `evolve`, `breed`) print `.kerf` text to **stdout**,
  or write it to a file with `-o`/`--out`. In that case nothing else goes to stdout, so you can pipe
  it on. A file given to `-o` must end in `.kerf`.
- Files are written atomically: the new content is written beside the target and moved into place,
  so a failed command never leaves half a file behind.
- Errors go to stderr as `Error: ...` and the exit code is 1. A command line clap cannot parse exits
  with 2. Exceptions are listed per command.
- Seeds are unsigned integers. The same patch, options and seed always give the same result.
- Only `play` and the MCP `play` tool open the audio device. Every other command is offline.

### `kerf new`

```
kerf new [--tempo <BPM>] [--style <STYLE>] [--seed <N>] [-o <FILE>]
```

Writes a new patch. Without `--style` it is empty: a header and a tempo.

| Option | Default | Meaning |
|---|---|---|
| `--tempo <BPM>` | 120, or the style's own choice | Tempo in beats per minute. With `--style` it overrides the style's tempo. |
| `--style <STYLE>` | none | Generate a patch in this style. See `kerf styles`. |
| `--seed <N>` | 1 | Seed for `--style`. |
| `-o, --out <FILE>` | stdout | Write to a file, whose name must end in `.kerf`. Refuses a path that already exists (exit 1), including a dangling symlink. |

```sh
$ kerf new --tempo 100
kerf 1
tempo 100

$ kerf new --style acid-line --seed 7 -o acid.kerf
$ kerf new --style acid-line -o acid.kerf
Error: acid.kerf already exists
```

### `kerf styles`

Lists the generator styles with tempo range, parts (a trailing `?` marks a part that may be left
out) and a one-line summary.

```
$ kerf styles
acid-line    126-136 bpm  parts: bass, drums
             A resonant 303-style bass line over a straight kick, polymetric hats and a clap on 2 and 4.
foundry      136-146 bpm  parts: drums, bass, drone?
             Four-on-the-floor kick, ducked low bass, polymetric hats and toms, and an optional drone.
...
```

The six styles are `acid-line`, `foundry`, `dub-chamber`, `slow-grain`, `broken-joinery` and
`tape-keys`.

### `kerf describe`

```
kerf describe [--json] [<KIND>]
```

Without `KIND`, lists every module kind by band (Time, Motion, Voices, Shape, Space, Out). With a
kind (case does not matter), prints its params with defaults, then its input and output jacks with
their signal types.

```
$ kerf describe Plane
Plane · Multimode filter · Shape band, 8 hp

Params (0–1, default shown)
  cutoff  0.45  Where the filter starts cutting.
  res      0.3  Boost at the cutoff. High settings whistle.
  mode       0  Low-pass, band-pass, high-pass, notch.
  drive    0.2  Saturates the input.

Inputs
  in      audio  The sound to filter.
  cutoff  cv     Moves the cutoff. An envelope here makes each note bloom.
  voct    pitch  Makes the cutoff follow the notes.

Outputs
  out     audio  The filtered sound.
```

`--json` prints the same data as JSON: an array of descriptors without `KIND`, one object with it
(keys `kind`, `summary`, `band`, `hp`, `params`, `inputs`, `outputs`). An unknown kind exits 1 and
lists the known ones.

The names in `describe` are what you use in commands: `set_param` takes a param key such as
`cutoff`, and `connect` takes jacks as `module.jack`, such as `seq.pitch`.

### `kerf apply`

```
kerf apply [-c <FILE>] [-o <FILE>] <PATCH>
```

Reads commands, one JSON object per line, applies them in order to `PATCH`, and prints the whole
resulting patch as `.kerf` text.

| Option | Default | Meaning |
|---|---|---|
| `-c, --commands <FILE>` | `-` (stdin) | The command file. |
| `-o, --out <FILE>` | stdout | Write to a file instead. It may be the input patch, to edit in place. |

Blank lines and lines starting with `#` are skipped. If a line is not valid JSON, is not a known
command, or fails against the patch, `kerf` stops, prints `<file>:<line>` with the reason, exits 1
and writes nothing, so a partly applied file is never left behind.

```sh
$ kerf new --tempo 110 -o song.kerf
$ cat edits.jsonl
{"op":"add_module","kind":"Tally","id":"clk"}
{"op":"add_module","kind":"Dovetail","id":"seq"}
{"op":"add_module","kind":"Chisel","id":"osc"}
{"op":"connect","from":"clk.clock","to":"seq.clock"}
{"op":"connect","from":"seq.pitch","to":"osc.voct"}
{"op":"set_param","module":"osc","param":"fine","value":0.55}
{"op":"store_scene","scene":"A"}
{"op":"lock","target":{"kind":"module","module":"osc"},"locked":true}
$ kerf apply song.kerf -c edits.jsonl
kerf 1
tempo 110

module clk Tally    tempo=0.5 swing=0.1 divide=0.5
module seq Dovetail steps_a=0.5 steps_b=0.38 interlock=0.6 chance=0.85 glide=0.15 range=0.4
module osc Chisel   tune=0.5 fine=0.55 shape=0.2 pw=0.5 fm=0 lock

cable clk.clock -> seq.clock
cable seq.pitch -> osc.voct

scene A clk.tempo=0.5 ...
```

(The scene line is shortened here.) From stdin:

```sh
echo '{"op":"set_tempo","bpm":100}' | kerf apply song.kerf -o song.kerf
```

A bad line:

```
$ echo '{"op":"bogus"}' | kerf apply song.kerf
Error: stdin:1: not a command

Caused by:
    unknown variant `bogus`, expected one of `add_part`, `add_module`, ...
```

#### Command reference

Every command is a JSON object with an `op` field. Unknown fields are rejected. This is the whole
protocol; the app and the MCP `patch_apply` tool use it too. The Rust definition is
`crates/kerf-core/src/command.rs`.

Ids (`module`, `part`, a module `id`) are the names used in the `.kerf` file. A jack is written
`"module.jack"`. Params are normalised: `value` is clamped to 0 to 1. A scene is one of `"A"`, `"B"`,
`"C"`, `"D"`.

| `op` | Fields | Notes |
|---|---|---|
| `add_part` | `id`, `name`, `colour` (`"#rrggbb"`) | Adds an empty part (a named group of modules). |
| `add_module` | `kind`, `id`?, `part`?, `auto_patch`? (bool), `near`? (module id) | Adds a module with default params. Without `id` one is made from the kind, such as `plane1`. `auto_patch` also wires it into the patch, as one undo step. `near` wires it to that module first and implies `auto_patch`. |
| `auto_patch` | `module`, `near`? | Wires an existing module the same way. Never replaces a cable, never closes a feedback loop; running it twice adds nothing. |
| `move_module` | `module`, `part` (id or `null`), `index`? | Moves a module into a part, or out of every part with `null`. Without `index` it goes last. |
| `replace_text` | `text` | Replaces the whole patch with `.kerf` text. Scenes and locks come from the text. |
| `remove_module` | `id` | Removes the module, its cables and its scene values. |
| `set_param` | `module`, `param`, `value` (0 to 1) | |
| `set_params` | `module`, `params` (a list of `{"param":key,"value":0 to 1}`) | Sets several params of one module as one undo step. If any key is unknown, nothing changes. |
| `set_sample` | `module`, `path`? | Gives a Veneer or Splinter an audio file. Omit `path` (or use `null`) to clear. The path is stored as given: absolute, or relative to the `.kerf` file. |
| `connect` | `from`, `to` | Output jack to input jack. |
| `disconnect` | `from`, `to` | |
| `lock` | `target`, `locked` (bool) | `target` is `{"kind":"module","module":id}`, `{"kind":"param","module":id,"param":key}` or `{"kind":"part","part":id}`. Locked things are left alone by evolve, breed and morph. |
| `mute` | `part`, `muted` (bool) | |
| `set_part_level` | `part`, `level` (0 to 1) | Gain on audio leaving the part. 1 is unity, 0.5 is -6 dB. |
| `set_tempo` | `bpm` | |
| `store_scene` | `scene` | Stores every param value as the scene. |
| `clear_scene` | `scene` | |
| `morph` | `a`, `b` (scenes), `t` (0 to 1) | Sets every unlocked param to `a + (b - a) * t`. |
| `undo`, `redo` | none | |
| `random` | `style`, `seed` | Replaces the patch with a new one from a style, safe-checked. |
| `evolve` | `strength` (0 to 1), `seed` | Mutates unlocked params and wiring. |
| `breed` | `with`, `seed` | `with` is `{"node": n}` (a lineage node) or `{"patch": "<.kerf text>"}`. Crosses the current patch with it, parameter by parameter. |
| `checkout` | `node` | Makes a lineage node the current patch. Lineage nodes exist only inside a session, so this mostly matters to the MCP server. |

`random`, `evolve` and `breed` take an optional `seed`. When it is left out, the session uses the
number of lineage nodes it holds, which is 1 for a patch just opened from a file. The MCP server
uses the same rule. A seed you give is always used as it stands.

### `kerf render`

```
kerf render [--seconds <S>] [--rate <HZ>] -o <FILE> <PATCH>
```

Renders offline to a 16-bit stereo WAV, as fast as the CPU allows. It does not touch the audio
device.

| Option | Default | Meaning |
|---|---|---|
| `-o, --out <FILE>` | required | The WAV to write. The name must end in `.wav`. An existing file is overwritten. |
| `--seconds <S>` | 16 | Length, from 0 to 600 (exit 1 outside that). |
| `--rate <HZ>` | 48000 | Sample rate. |

Prints one line to stdout: the file, length, peak and RMS in dBFS. If the patch uses a module that
makes no sound yet, a second line names it (`silent (DSP not written yet): ...`). Warnings, such as a
missing sample file falling back to the demo sample, go to stderr as `warning: ...`. Sample paths
inside the patch are resolved relative to the patch file.

```
$ kerf render acid.kerf -o acid.wav --seconds 4
acid.wav: 4 s, peak -2.4 dBFS, RMS -13.4 dBFS
```

### `kerf evolve`

```
kerf evolve [--strength <X>] [--lock <TARGET>]... [--seed <N>] [-o <FILE>] <PATCH>
```

Mutates the unlocked params and wiring of a patch and prints the result.

| Option | Default | Meaning |
|---|---|---|
| `--strength <X>` | 0.3 | 0 changes nothing, 1 changes a lot. |
| `--lock <TARGET>` | none | Lock something first. Repeatable. A part id, a module id or `module.param`. A name with a dot is a param; otherwise a part is looked up before a module. An unknown name exits 1. |
| `--seed <N>` | 1 | The same file evolves the same way unless you give another seed. |
| `-o, --out <FILE>` | stdout | May be the input patch. |

Locks added with `--lock` are written into the output. Locks already in the file are respected.

```sh
kerf evolve acid.kerf --strength 0.2 --lock bass --seed 5 -o acid2.kerf
```

### `kerf breed`

```
kerf breed [--seed <N>] [-o <FILE>] <A> <B>
```

Crosses two patches into a child that uses only parameters from the parents. The child starts from
`A`. Locks in `A` are respected.

| Option | Default | Meaning |
|---|---|---|
| `--seed <N>` | 1 | |
| `-o, --out <FILE>` | stdout | May be one of the parents. |

```sh
kerf new --style foundry --seed 3 -o foundry.kerf
kerf breed acid.kerf foundry.kerf --seed 2 -o child.kerf
```

### `kerf check`

```
kerf check <PATCH>
```

Renders the patch for 6 seconds at 32 kHz and prints a JSON report to stdout. Exits 0 if `pass` is
true and 1 if not. The same check is applied to every patch the generators make.

```
$ kerf check acid.kerf
{
  "pass": true,
  "seconds": 6.0,
  "sample_rate": 32000.0,
  "peak_db": -2.3999999,
  "rms_db": -13.614531,
  "ceiling_db": -2.3999996,
  "silent_fraction": 0.0,
  "dc_offset": 0.000779716,
  "finite": true,
  "clipped": false,
  "runaway": false,
  "silent_kinds": [],
  "problems": []
}
```

It fails on NaN or infinite samples, a level that is too quiet or too hot, too much of the render
silent, DC offset above 0.02, clipping, or a level that keeps growing (feedback). `problems` says
which, in words. `silent_kinds` lists module kinds in the patch that make no sound yet.

### `kerf play`

```
kerf play [--seconds <S>] <PATCH>
```

Plays the patch on the default audio output. While it plays, `kerf` reads commands from stdin, one
JSON object per line (the format of `kerf apply`), and applies each one live. Blank lines and `#`
lines are skipped. A bad or failing command prints `{"error": "..."}` to stdout and playback goes
on. Nothing is written back to the patch file.

| Option | Default | Meaning |
|---|---|---|
| `--seconds <S>` | until Ctrl-C | Stop after this long. Above 0 and at most 86400. |

Ctrl-C fades out and exits. A status line goes to stderr when playback starts. As with `render`,
relative sample paths are resolved against the patch file.

```sh
kerf play acid.kerf
# then type, one per line:
{"op":"set_param","module":"flt1","param":"cutoff","value":0.2}
```

### `kerf mcp`

```
kerf mcp [<PATCH>]
```

Runs the MCP server on stdin and stdout. With `PATCH` the session starts from that file; without it
the session starts empty. It runs until stdin closes. See [MCP.md](MCP.md) for the tools and for how
to connect a client.

## Exit codes

| Code | When |
|---|---|
| 0 | Success. |
| 1 | Any error (unreadable or invalid patch, bad command, bad option value, existing `--out` for `new`), or `check` failing. |
| 2 | The command line could not be parsed. |
