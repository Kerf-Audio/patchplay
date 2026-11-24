# Writing Kerf starting points

A Kerf starting point is a plain text `.kerf` file: a few header lines, the modules, and the cables between them. The GUI, the CLI and MCP all read and write the same text, so anything you can patch with a mouse you can write by hand, and the reverse.

Examples marked "complete" in this guide are real, tested patches; you can paste any of them into the editor and apply it.

## File anatomy

A file is read line by line. Blank lines and anything after a `#` that starts a comment are ignored (a `#` inside a colour such as `#c94a3d` is part of the colour). Lines come in this order:

1. `kerf 1`: the format version. It must be first.
2. Header lines: `tempo`, `key`, `style`, `tags` (all optional).
3. Loose modules: `module` lines at the left margin, before any part.
4. Parts, each a `part` line followed by its indented modules.
5. `cable` lines.
6. `scene` lines.

Modules are written before the cables that use them. Indentation matters: an indented `module` line belongs to the `part` above it, and a module at the left margin is loose, in no part. Only `module` lines are indented, and an indented module needs a part above it. Kerf prints loose modules first, then each part.

## Header lines

```kerf
kerf 1
tempo 118
key A minor
style dub-chamber seed 4821
```

- `tempo` is beats per minute, 20 to 300.
- `key` is a label (`A minor`, `D dorian`). The engine does not read it; the Fret module's `root` and `scale` params choose the notes. Generators use it to stay in key.
- `style <name> seed <n>` records which style rulebook grew the patch. It changes nothing when the file is played.
- `tags <word> ...` files the patch under category words such as `four-four`, `abstract` or `crazy` (lowercase letters, digits and `-`). The starting-point browser groups by them; the engine ignores them. Leave the line out when there are none.

## Modules and params

```
module <id> <Kind> [param=value ...]
```

- `<id>` names this module in cables and scenes: lowercase letters, digits and `_`, starting with a letter.
- `<Kind>` is a module kind such as `Tally` or `Mortise`. The Modules panel and the generated reference at the end of this guide list every kind.
- Params are `key=value`. Every param runs from `0` to `1`: the module maps that range to something musical (a filter cutoff, a number of hits). Missing params take their defaults, so you only write what you change.

```
module kick Mortise decay=0.3 punch=0.8
```

Values outside 0 to 1 are an error, not clamped.

Some modules store a whole pattern as params. Inlay keeps its notes in `step1` to `step16`, one value each: `0` is a rest, and `round(value * 25)` from `1` to `12` is a note (C to B) and from `13` to `24` the same note accented. Every note is a multiple of `0.04`, so the file stays short. Tread keeps its eight step values the same way, as `step1` to `step8`, each a plain value from 0 to 1.

## Parts

A part groups modules under a name and a colour, so a patch reads as drums, bass, lead.

```
part drums "Drums" colour #c94a3d
  module kick Mortise
```

- The id is lowercase; the name is in double quotes; `colour` takes a hex colour.
- `lock` after the colour protects the part from evolve, breed and morph. `mute` silences it.
- `level=0.8` sets the part's level (see below). Leave it out for full level.

### Part levels

`level` runs from 0 to 1 and is the gain on audio that leaves the part for the rest of the patch: 1 is unity (nothing changes), 0.5 is half the amplitude (about -6 dB), 0 is silence. It is a straight multiplier on the amplitude, smoothed so moving it live does not click.

Only audio cables that cross the part's boundary are scaled. Cables between modules inside the part, and cables carrying pitch, gates, clocks or CV, are untouched, and modules outside any part are never scaled.

```
part bass "Bass" colour #3d63c9 level=0.8
```

Use levels to balance parts against each other without opening the mixer. A file without `level` plays exactly as before.

## Cables

```
cable <module>.<jack> -> <module>.<jack>
```

A cable runs from an output jack to an input jack. Every jack has a kind, and the GUI colours cables by the kind of the jack they leave:

- Audio: sound.
- Cv: a slow control signal, such as an envelope or LFO, that moves a param.
- Pitch: a note, as a voltage; plug it into a `voct` jack.
- Gate: on or off, for triggers and note lengths.
- Clock: a steady pulse that keeps modules in time.

Rules:

- Fan-out is free: one output can feed many inputs.
- Fan-in is for audio and Cv inputs, which sum. Two cables into a Gate, Clock or Pitch input are a mistake and are reported as a warning.
- Audio can go to Audio, Cv can go to Cv; the engine is forgiving between kinds, but a cable between unlike kinds is usually a slip.
- A cable may not make a loop of modules that feed each other with no delay in between.
- Jack names are in the generated module reference.

## Scenes and morph

A scene stores a set of param values under the name A, B, C or D:

```
scene A filt.cutoff=0.2 osc.shape=0.3
scene B filt.cutoff=0.85 osc.shape=0.6
```

The Morph control (or `{"op":"morph","a":"A","b":"B","t":0.5}`) blends every unlocked param between two scenes. Scene values are only the params you write; others keep their value. Part levels are not stored in scenes.

## Locks

`lock` freezes things so generators and morph leave them alone:

- On a part line: the whole part.
- On a module line: `lock` freezes the module, `lock=cutoff,res` freezes those params only.

A locked part still plays; it just stops changing when you evolve, breed or morph.

## Samples

A Veneer or a Splinter plays an audio file you point it at:

```
module hat Veneer sample="samples/hat.wav"
```

The path is absolute or relative to the `.kerf` file. Only a Veneer or a Splinter takes `sample=`; on any other module it is an error. A Veneer cuts the file into 16 slices; a Splinter reads grains from it.

## Style and seed

`style <name> seed <n>` records where a generated patch came from. The Styles panel (or `kerf styles`) lists the names. The same style and seed always grow the same patch, so you can share a seed instead of a file.

## Worked examples

### A four-module beat

A clock, a rhythm, a kick and the output:

```kerf complete
kerf 1
tempo 120

module clock Tally
module rhythm Ruler
module kick Mortise
module out Finish

cable clock.clock -> rhythm.clock
cable rhythm.a -> kick.trig
cable kick.out -> out.l
cable kick.out -> out.r
```

The Tally ticks, the Ruler turns the ticks into hits on lane `a`, the Mortise plays a kick on each hit and the Finish sends it to the speakers.

### Adding a bass

A sequencer plays notes, a Fret snaps them to a scale, a Chisel makes the tone and a Plane filters it:

```kerf complete
kerf 1
tempo 120
key A minor

module clock Tally
module rhythm Ruler
module kick Mortise
module seq Dovetail
module scale Fret
module osc Chisel
module filt Plane
module out Finish

cable clock.clock -> rhythm.clock
cable clock.clock -> seq.clock
cable rhythm.a -> kick.trig
cable seq.pitch -> scale.pitch
cable scale.pitch -> osc.voct
cable osc.saw -> filt.in
cable kick.out -> out.l
cable kick.out -> out.r
cable filt.out -> out.l
cable filt.out -> out.r
```

### A multi-part patch

Parts, a mixer, an envelope on the bass and a reverb send:

```kerf complete
kerf 1
tempo 112
key A minor

module clock Tally
module mix Router
module room Kiln
module out Finish

part drums "Drums" colour #c94a3d
  module rhythm Ruler
  module kick Mortise

part bass "Bass" colour #3d63c9 level=0.8
  module seq Dovetail
  module scale Fret
  module osc Chisel
  module filt Plane
  module env Bevel
  module amp Clamp

cable clock.clock -> rhythm.clock
cable clock.clock -> seq.clock
cable rhythm.a -> kick.trig
cable seq.pitch -> scale.pitch
cable scale.pitch -> osc.voct
cable osc.saw -> filt.in
cable seq.gate -> env.gate
cable filt.out -> amp.in
cable env.env -> amp.cv
cable kick.out -> mix.in1
cable amp.out -> mix.in2
cable mix.mix_l -> out.l
cable mix.mix_r -> out.r
cable mix.send -> room.in_l
cable room.l -> out.l
cable room.r -> out.r

scene A filt.cutoff=0.3
scene B filt.cutoff=0.8
```

The `bass` part's level of 0.8 turns its audio down a little where `amp.out` leaves the part for the mixer; the cables inside the part are unchanged.

## Commands

Everything that changes a patch is a command: a JSON object with an `op` field. The GUI, the CLI (`kerf apply`) and MCP (`patch_apply`) send the same ones, each is one undo step, and a failed command changes nothing.

- `add_part {id, name, colour}`
- `add_module {kind, id?, part?, auto_patch?, near?}`: `auto_patch: true` also wires the new module sensibly; `near` names a module to wire it to, and implies wiring.
- `auto_patch {module, near?}`: wires an existing module into the patch. It never overwrites a cable, never makes a loop, and does nothing if there is nothing to add.
- `move_module {module, part, index?}`: `part` is a part id or `null` for loose.
- `remove_module {id}`
- `set_param {module, param, value}`
- `set_params {module, params: [{param, value}, ...]}`: several params of one module as one undo step.
- `set_sample {module, path}`
- `connect {from, to}` and `disconnect {from, to}`: jacks as `"module.jack"`.
- `lock {target, locked}`, with target `{"kind":"module"|"param"|"part", ...}`
- `mute {part, muted}`
- `set_part_level {part, level}`
- `set_tempo {bpm}`
- `store_scene {scene}`, `clear_scene {scene}`, `morph {a, b, t}`
- `replace_text {text}`: replaces the whole patch with `.kerf` text, as one step. A parse error rejects it with the line and column.
- `random {style, seed}`, `evolve {strength, seed}`, `breed {with, seed}`, `checkout {node}`
- `undo`, `redo`

## Writing factory starting points

The starting points in the app's browser are the files in the repository's `presets/` folder. To add one, put a `.kerf` file there; there is no list to edit. The build embeds every `presets/*.kerf`, and the file name without `.kerf` becomes the name shown.

- Give it a `tempo`, a `key` and one or more `tags` (lowercase words). The browser groups by the first tag and lets you filter by any of them.
- It must be complete: a master `Finish` module with the output cabled in, so it makes sound the moment it opens.
- It must pass the same checks as any patch: it parses, every module and param is valid, no module is left silent, and the output stays under the Finish ceiling. The factory-presets test renders every file in `presets/` and fails on one that does not.
- Keep a few voices straight and give the interest to a few others (see Tips), so each starting point is easy to build on.
- Check it with `kerf check presets/<name>.kerf` before running the tests.

## Tips

- Rhythm rule: keep a few voices dead straight, such as the kick and one hat, and give the interest to a few others with chance or polyrhythm. If everything is wild it sounds like noise; if everything is straight it sounds like a metronome.
- Levels: start with part levels at 1 and lower what is too loud. Balance by ear with the kick and bass first. A part at 0 is a fast way to hear what the rest sounds like.
- Ducking: feed the kick's hits into a Level module and use its envelope to pull the bass or pads down on every beat.
- Build up in layers: get the beat playing first, then add one part at a time. After each, use Undo if it is worse.
- Let Kerf wire things: add a module with auto-patch on, then adjust the cables it chose.
- Lock the parts you like before you evolve.

## Common errors

- `unknown module kind`: check the spelling and capitals; kinds are `Tally`, not `tally`.
- `unknown param` or `unknown jack`: the name is not on that module; hover it in the editor or read the module reference.
- `no module ... in a cable`: the cable names a module no `module` line defines.
- `value must be between 0 and 1`: params are normalised; scale your number down.
- `defined twice`: ids are unique across modules; part ids are unique across parts.
- `sample=` on something other than a Veneer or a Splinter.
- Two writers into one gate, clock or pitch input: use a Router or a Ruler lane to combine them.
