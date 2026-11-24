# Writing patches in the editor

The code editor shows the patch as `.kerf` text. You can read it, change it and write new parts by hand, and anything you can patch with the mouse is there as text too. This guide builds a small patch from nothing, then covers the numbers you will keep looking up and the mistakes that are easy to make. [KERF-FORMAT.md](KERF-FORMAT.md) is the reference for the file itself; [SNIPPETS.md](SNIPPETS.md) is a library of ready-made parts.

Blocks marked "complete" are tested patches you can paste into the editor and apply. The step blocks after them are parts to append, in order.

## How the editor works

The editor lives in the footer of the main window and can be popped out into its own window. Both show the same patch.

- The text you type is a draft. Nothing changes in the patch, and nothing changes in the sound, until you press Apply.
- Apply reads the whole draft. If it parses, the patch is replaced in one step and Undo takes you back to what you had. If it does not, you get the line and column of the first mistake and the patch stays as it was.
- Hovering a module kind, a param or a jack shows what it does and its range. The completion list offers the same names while you type.
- "Add to patch" on a snippet in the docs panel appends it to the end of the draft. It does not apply it.

## Reading a patch

```
part drums "Drums" colour #c94a3d
  module kick Mortise engine=0 decay=0.3

cable clock.clock -> kick.trig
```

- A `module` line makes one thing: `module <id> <Kind> <params>`. The id is yours, in lowercase, and every cable and scene refers to it. The kind is one of the modules in the Modules panel, spelt with a capital: `Mortise`, `Ruler`, `Plane`.
- A `part` line groups the indented modules below it under a name and colour. Parts are for reading and for levels, mutes and locks; they do not change how the cables work.
- A `cable` line joins `module.jack` to `module.jack`, from an output to an input.
- Params run from 0 to 1, always. The module turns that into a frequency, a time or a count. A param you leave out keeps its default, so a line like `module kick Mortise` is already a kick.
- A `#` starts a comment, except inside a colour.

The order matters in one way: modules come before the cables that name them, and a module outside any part has to come before the first `part` line. Everything in this guide puts modules inside parts, so you can always append a new part and its cables at the end.

## Step 1: a clock and a kick

Every sound here runs from a Tally, which is the clock. It ticks once per 16th note, and its speed comes from the `tempo` line. A Finish is the master output. Put those in a part called `master`, then add a kick: a Ruler that decides when, and a Mortise that makes the sound.

```kerf complete
kerf 1
tempo 120
key A minor

part master "Master" colour #8a8f98
  module clock Tally
  module out Finish

part drums "Drums" colour #c94a3d
  module kick_rhythm Ruler hits_a=0.25 steps_a=1 hits_b=0 hits_c=0 hits_d=0
  module kick Mortise engine=0 decay=0.3

cable clock.clock -> kick_rhythm.clock
cable kick_rhythm.a -> kick.trig
cable kick.out -> out.l
cable kick.out -> out.r
```

The Ruler has four lanes, `a` to `d`. `steps_a=1` makes lane `a` sixteen steps long and `hits_a=0.25` spreads four hits evenly across them: one on every beat. Lanes `b` to `d` are set to no hits so only the kick lane plays. The Mortise plays a drum on each trigger; `engine=0` selects the kick.

Paste it into the editor, press Apply, then press play. Change `decay` and Apply again to hear a longer or shorter kick.

## Step 2: a second voice

Add hats on their own part, with their own Ruler. Part ids, module ids and cable ends must not repeat anything already there, so every module here starts with `hats`.

```kerf step
part hats "Hats" colour #c94a3d level=0.6
  module hats_rhythm Ruler hits_a=0.5 steps_a=1 hits_b=0 hits_c=0 hits_d=0
  module hats_voice Mortise engine=0.5 pitch=0.55 decay=0.1 tone=0.7

cable clock.clock -> hats_rhythm.clock
cable hats_rhythm.a -> hats_voice.trig
cable hats_voice.out -> out.l
cable hats_voice.out -> out.r
```

`hits_a=0.5` is eight hits over sixteen steps: every eighth note. `level=0.6` turns the whole part down, because a hat at full level is louder than it needs to be. Two sounds cabled to `out.l` add together; that is how everything reaches the speakers.

## Step 3: a bass line

A bass takes more modules. A Dovetail writes a melody and a gate for each note, a Fret snaps the melody to a scale, a Chisel is the oscillator, a Bevel shapes each note, a Plane filters it and a Clamp sets the volume.

```kerf step
part bass "Bass" colour #3d63c9 level=0.8
  module bass_seq Dovetail steps_a=1 steps_b=1 interlock=0.6 chance=1 glide=0.25 range=0.3
  module bass_scale Fret root=0.75 scale=0.43 octave=0.25
  module bass_osc Chisel tune=0.5 shape=0 pw=0.5
  module bass_env Bevel attack=0 decay=0.38 sustain=0 release=0.2 curve=0.3
  module bass_filter Plane cutoff=0.28 res=0.8 mode=0 drive=0.3
  module bass_amp Clamp level=0 response=0.1

cable clock.clock -> bass_seq.clock
cable bass_seq.pitch -> bass_scale.pitch
cable bass_scale.pitch -> bass_osc.voct
cable bass_seq.gate -> bass_env.gate
cable bass_osc.saw -> bass_filter.in
cable bass_env.env -> bass_filter.cutoff
cable bass_filter.out -> bass_amp.in
cable bass_env.env -> bass_amp.cv
cable bass_amp.out -> out.l
cable bass_amp.out -> out.r
```

Read the cables from top to bottom and you have followed the signal: notes go through the scale to the oscillator; the saw goes through the filter and the amp to the output. The envelope is cabled twice. Into `bass_filter.cutoff` it opens the filter on each note; into `bass_amp.cv` it opens the volume. With `level=0` the Clamp is silent until the envelope lifts it, which is how a note stops.

`root=0.75` is A and `scale=0.43` is natural minor, matching the `key` line. `octave=0.25` puts the line two octaves down. Move `bass_filter.cutoff` and `res` and listen; they are the most expressive two numbers here.

## Step 4: an effect

An effect is a module with an input you feed. Here a Bandsaw tape echo listens to the bass. Its cables to `out` carry only the echoes, so the bass's own cable to `out` stays and is the dry signal.

```kerf step
part fx "Effects" colour #d1a233
  module echo Bandsaw time=0.5 feedback=0.45 wear=0.35 mix=1

cable clock.clock -> echo.clock
cable bass_amp.out -> echo.in
cable echo.l -> out.l
cable echo.r -> out.r
```

The Bandsaw takes its delay time from the clock, so the echoes stay in time if you change the tempo. `mix=1` means the module outputs echo only. To use it in series instead, lower `mix` so it carries some dry sound, and take the source's own cable to `out` away.

## Numbers worth knowing

Most params are plain 0 to 1 sweeps. These few map to something you will want to calculate.

| Module and param | Mapping |
| --- | --- |
| Mortise `engine` | 0 kick, 0.25 snare, 0.5 hat, 0.75 clap, 1 tom |
| Ruler `steps_a` | The lane is `1 + round(steps * 15)` steps long: 1 is sixteen, 0.47 is eight, 0.27 is five |
| Ruler `hits_a` | `round(hits * length)` hits, spread evenly, starting on step 1 |
| Ruler `rotate` | Slides every lane by `round(rotate * length)` steps |
| Ruler `chance` | The chance a hit plays on lanes `b` to `d`. Lane `a` always plays |
| Fret `root` | `round(root * 12)` semitones above C: 0.75 is A, 0.5 is F#, 0 is C |
| Fret `scale` | 0 major pentatonic, 0.14 minor pentatonic, 0.29 major, 0.43 natural minor, 0.57 dorian, 0.71 mixolydian, 0.86 phrygian, 1 harmonic minor |
| Fret `octave` | `round((octave - 0.5) * 8)` octaves: 0.25 is two down, 0.375 one down, 0.5 none, 0.625 one up |
| Inlay `step1`..`step16` | `round(step * 25)`: 0 rest, 1 to 12 the notes C to B, 13 to 24 the same notes accented. Each note is a multiple of 0.04 (C is 0.04, B is 0.48; accented C is 0.52) |
| Inlay `length` | `1 + round(length * 15)` steps: 1 is sixteen |
| Inlay `transpose`, `octave` | `round(transpose * 24) - 12` semitones and `round(octave * 4) - 2` octaves; 0.5 is none for both. Row C with no transposition sounds as C4 |
| Inlay `rate` | Only when no clock is patched. `round(rate * 5)` picks a note length: 0 is a quarter, then an eighth, eighth triplet, 16th, 16th triplet, 1 is a 32nd |
| Inlay `gate` | The gate is high for 5% to 95% of each step; a tied note holds across the join |
| Mitre `chord` | `round(chord * 13)` picks one of 14 chords: major, minor, sus2, sus4, diminished, augmented, major 7th, minor 7th, dominant 7th, minor 9th, open major, open minor, stacked fifths, stacked fourths. Triads put the octave on voice 4 |
| Mitre `inversion`, `spread` | Each is `round(knob * 3)` steps: inversion lifts that many of the lowest notes an octave, spread lifts the upper notes by whole octaves |
| Mitre `root`, `snap` | `root` works like Fret's (`round(root * 12)` semitones above C). `snap` 0 is off; `round(snap * 8)` from 1 to 8 snaps the notes to the Fret scale of that number (1 major pentatonic ... 8 harmonic minor) in the key of `root` |
| Rung `pattern` | `round(pattern * 7)`: 0 up, 1 down, 2 up-down, 3 down-up, 4 random, 5 as played, 6 converge, 7 bounce |
| Rung `rate` | Only when no clock is patched. `round(rate * 8)` picks a note length: 0 is a half note, then a quarter, dotted eighth, eighth, eighth triplet, dotted 16th, 16th, 16th triplet, 1 is a 32nd |
| Splice `mode` | `round(mode * 3)`: 0 FM, 1 AM, 2 ring, 3 sync |
| Splice `coarse` | The built-in modulator's ratio to the carrier, picked by `round(coarse * 10)`: 1/4, 1/2, 3/4, 1, 1.5, 2, 3, 4, 5, 6, 8. So 0.3 is 1 and 0.5 is 2 |
| Tread `length`, `direction` | `1 + round(length * 7)` steps. `direction` is cut in quarters: 0 forward, 0.25 back, 0.5 pendulum, 0.75 and up random |
| `depth` | On Bevel, Burl, Level, Jig, Rocker, Tread and Chatter it scales the CV out: 1 is full, 0 holds it at 0 V. On Spokeshave it shrinks the swing towards 5 V instead |
| Chisel `tune` | Centred on 0.5. One semitone is about 0.02 (1/48): a fifth up is about 0.646 |
| Plane `mode` | 0 low-pass, 0.33 band-pass, 0.67 high-pass, 1 notch |
| Plane `cutoff` | Exponential: 0 is about 20 Hz, each 0.1 is one octave up, 1 is about 20 kHz |
| Tally outputs | `clock` one per 16th, `x2` twice that, `div4` one per beat, `bar` one per bar |

A pitch cable carries one volt per octave, with 0 V as C4. A Cv cable adds to the param it is plugged into; the Plane's `cutoff` jack moves the cutoff by half an octave per volt, and envelopes and LFOs can run up to 10 V, so they can open a filter a long way.

## Rhythm and notes

Keep a few voices dead straight and put the interest in a few others. The kick on every beat and a hat on every eighth are the anchor; give the Dovetail a `chance` below 1, or a lane of five steps against the others' sixteen, and it will wander against the anchor without losing it. A part where everything wanders sounds like noise.

Lengths that do not divide sixteen make polyrhythm for free. A Ruler lane of seven steps with three hits (`steps_b=0.4 hits_b=0.43`) repeats every seven 16ths, so it lines up with the sixteen-step lanes only every 112 steps.

To play in a different key, change `key` for the record and change the Fret's `root` and `scale` for the sound. Several Frets can sit in one patch, each with its own scale, but they stay in step only if you give them the same `root` and `scale`.

For chords, patch a melody's `pitch` and `gate` into a Mitre and take its four `voice` outputs to four oscillators; its `gate` is the input gate passed straight through. For an arpeggio, patch the four `voice` outputs into a Rung and its `pitch` and `gate` to one oscillator and envelope. A Rung with no clock patched runs from the patch tempo and its `rate` knob; with a clock patched it follows that clock and ignores `rate`. The Mitre reads its own `root` and `snap` for snapping to a scale, because the engine does not read the patch `key` label. (`lock` and `sample` are words of the text format, so no module uses them as a param name.)

To draw a melody instead of writing one, add an Inlay. In the app, click a cell on its piano roll to put a note on that step (the bottom row is C, the top row is B), click it again to clear it, and drag across steps to draw or erase several. Shift-click sets an accent. Patch its `pitch` and `gate` like a Dovetail's. Two notes on the same row side by side tie into one long note. `length` shortens the loop, `transpose` and `octave` move it, and a cable into its `transpose` jack adds to that one volt per octave. In a `.kerf` file each step is a param, `step1` to `step16`: `0` is a rest, and a note is `round(step * 25)` from `1` to `12` (C to B) or `13` to `24` (the same note, accented). So `step1=0.04` is a C, `step2=0.2` is an E and `step3=0.52` is a C with an accent.

## Recipes

Each recipe has a runnable version in the Snippets section of the docs panel. Paste the Start snippet first, then the one you want.

### Modulators and depth

A modulator is a module whose output you cable into another module's jack, such as a Plane's `cutoff`, so that param moves by itself. Rocker is a plain LFO. Tread is eight values you set by hand. Chatter is a spring you knock. Warp is slow random drift for pitch. Spokeshave, Burl, Bevel, Level and Jig work the same way. A Cv cable adds their movement to whatever the knob already says.

Each has a `depth` param, which is the quickest way to tame one without re-cabling. At 1 you get the full swing; at 0 the output holds still. Set `depth=0.3` and the same cable becomes a gentle shimmer.

For a slow filter sweep, cable a Rocker's `out` into a Plane's `cutoff` with `rate=0.3` and `depth=0.6`. Turn up `sync` to lock the rate to the tempo. A Tread does the same with steps: change the eight `step` values, shorten `length`, or set `direction` to walk them backwards, there and back, or at random. For pitch drift, cable a Warp's `out` into an oscillator's `voct`. With nothing in its `in`, it sends only the wobble, so a small `amount` is an oscillator that never quite stays put. A Chatter needs knocking: cable a clock into `kick`, or raise `rate` to let it knock itself. Raise `damping` to stop it quickly and lower it to let it swing. The runnable versions are Rocker, Tread, Warp and Chatter in the Modulation snippets.

### Chords and arpeggios

Mitre turns one note into four, and a Rung plays those four one at a time. The Rhythm and notes section above says how to cable them. A good start is `chord` on a minor seventh, a Rung `pattern` of up-down and `octaves` at two. `ratchet` plays some steps twice and `swing` pushes every second step late. The runnable version is Chord arpeggio in the Notes and chords snippets.

### Dub echo and siren

A Dado is an echo built for dub. Feed it into `in_l`, keep `mix=1` and cable `l` and `r` to the output. Each repeat gets darker (`tone`), thinner (`thin`) and dirtier (`drive`), and `feedback` past about 0.85 turns the echoes into a howl. With `sync` on, `time` picks a note length at the tempo, so the echoes stay in time. Cable a gate into `throw` and sound goes in only while the gate is high, which lets you fire single echoes. Sweep `time` with an LFO and the repeats dive and climb in pitch. The runnable version is Dado in the Effects snippets.

A Siren makes the sound worth throwing. A gate starts it, the LFO dives the pitch from `range` above `pitch` back down to it, and `release` decides how long it rings after the gate drops. One or two hits a bar from a Ruler lane is plenty. Cable the Siren into the Dado and you have the whole routine. The runnable version is Dub siren in the Percussion snippets.

### FM, AM and ring

Splice sits in the Shape band and changes a sound you feed into `carrier`. `mode` picks FM, AM, ring or sync, `amount` is how hard it works, and `coarse` and `fine` tune the built-in modulator against the carrier. Whole ratios sound like a clean bell, and a nudge of `fine` makes them clangorous. To modulate with another sound instead, cable it into `mod`. `mix=1` gives only the modulated sound, so take the source's own cable to `out` away if you do not want it as well. The runnable version is Splice in the Effects snippets.

### Radio static

A Static is a radio between stations. `tune` is the dial, and turning it sweeps whistles; a faint station hides at three spots. `hiss`, `whistle`, `crackle` and `ghost` set how much of each you hear, and `fade` makes the signal come and go. Cable a slow LFO into `tune` and it hunts for stations by itself. Its `crackle` output is a pulse for every click, so you can use the pops as triggers. The runnable version is Radio static in the Percussion snippets.

### Granular sampling

A Splinter plays tiny grains from an audio file. In the app, drop a file on it; in a `.kerf` file write `sample="path/to/file.wav"`. A Veneer takes files the same way, and with no file both play the built-in demo sample. `position` is where the grains read from and `spray` scatters that. `size` and `density` set the grain length and how many play each second, and `pitch` and `scatter` transpose them. Cable a slow LFO into `position` to scan through the file, or set `hold` above 0.5 to freeze the read point. With nothing in `trig` the grains stream on their own; cable a gate in and each pulse fires a burst of eight. The runnable version is Sampled grains in the Percussion snippets.

### Drawing patterns

An Inlay lets you draw notes instead of typing them; the Rhythm and notes section explains it. For rhythm the equivalent is a Ruler lane, and for steps of movement rather than notes it is a Tread. In a `.kerf` file a drawn pattern is just numbers, so it copies out of the app as text and back in. The runnable version is Drawn melody in the Notes and chords snippets.

## Finding port names

Jack names come from the module. The quickest ways to see them:

- Hover a module kind, a param or a jack in the editor to see what it is and its range. The completion list offers the names while you type.
- The Modules section of the docs panel lists every module's params, inputs and outputs. `kerf describe Plane` prints the same for one module in a terminal.

A cable runs from an output to an input.

## Using the snippets

The Snippets sections of the docs panel hold parts that are ready to paste: drums, bass, leads, pads, chords and melodies, percussion, modulation and effects. They follow the pattern in this guide: one part each, ids with the snippet's own prefix, and cables to a `clock` and an `out`.

1. Paste the Start snippet into an empty draft, or keep your own clock and output with the same ids.
2. Add sound snippets one at a time with "Add to patch" and apply after each.
3. For an effect, add a cable from the sound you want to treat to the effect's input. The snippet's header comment names the input.

Ids with a prefix (`kick_`, `acid_`) never clash with each other. If you paste the same snippet twice you get an error for the duplicate ids; rename the prefix with a search and replace first.

## When it will not apply

The editor stops at the first parse error and shows the line and column. The ones you are likely to meet:

- An unknown module kind: capitals matter, so `Tally`, not `tally`.
- An unknown param or jack: the name is not on that module. Hover it, or read the module reference.
- A value outside 0 to 1: params are never clamped, so `cutoff=1.5` is an error.
- A cable that names a module nobody defines: check the id's spelling, and that the part that holds it is in the draft.
- An id defined twice: module ids are unique across the whole patch, part ids across the parts.
- A module with indentation and no part above it, or a module at the left margin after the first part. Loose modules go before the first `part` line.
- `sample=` on anything but a Veneer or Splinter.

Some mistakes parse and are still wrong, and show as warnings in the editor: two cables into one gate, clock or pitch input (use a Router, or a second Ruler lane), or a loop of modules that feed each other with nothing to delay the signal.

If a patch applies and is silent, the usual causes are a part's `level` or `mute`, a cable from the sound to nothing (check both `out.l` and `out.r`), a Clamp at `level=0` with nothing opening it, or a Ruler lane with no hits.

## Commands and cheat sheet

Press ⌘K (Ctrl-K elsewhere), or the Commands button in the editor bar, to open a list of things to insert. Type to filter it; arrows and Enter pick. Nothing is applied: the text goes into the draft and you press Apply when you are ready.

| Command | What it inserts |
| --- | --- |
| New patch | Replaces the draft with a clock, a Router mixer and a Finish master, cabled together |
| tempo, key | A `tempo 120` or `key A minor` line at the cursor |
| part | A `part` line with a fresh id, ready for indented modules |
| connect | `cable ` at the cursor, then opens the list of jacks to finish it |
| add module \<Kind\> | A `module` line for any module kind, with every default param written out and an id that is not yet in the draft |
| Each snippet | Appended to the end of the draft, after a blank line |

The statements, in the order a file uses them:

```
kerf 1
tempo 120
key A minor
module <id> <Kind> [param=value ...]
part <id> "Name" colour #rrggbb [level=0.8] [mute] [lock]
  module <id> <Kind> [param=value ...]
cable <module>.<jack> -> <module>.<jack>
scene A <module>.<param>=<value> ...
```

The module ids and the Kind names are the ones in the Modules section of the docs panel.

## Where next

[SNIPPETS.md](SNIPPETS.md) has the whole library. [KERF-FORMAT.md](KERF-FORMAT.md) covers scenes and morph, locks, samples and the commands behind Apply and Undo.
