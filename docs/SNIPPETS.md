# Snippets

Parts to paste into the code editor: drums, bass, lead, pads, chords and melodies, percussion, modulation and effects. This page and the pages it links to are generated from the `.kerf` files in `snippets/`, so edit those rather than the pages.

Each snippet is one `part`: its modules, plus the cables that connect it to a clock and the master output, as plain `.kerf` lines.

- In the app, **Add to patch** appends the snippet to the end of the code editor's draft. It does not apply anything: read it, change it, then apply the draft when you are ready.
- Every module id starts with the snippet's own prefix (`kick_`, `acid_`, `echo_`), so several snippets can sit in one patch without clashing.
- Sound snippets cable to a module called `clock` (a Tally) and one called `out` (a Finish). The Start snippet makes both; paste it first.
- A comment header on each snippet lists what it needs, what to feed an effect, and the jacks worth patching elsewhere.
- Set the tempo and key in the patch header. The snippets follow the tempo and play in A minor until you change their Fret.

See [WRITING-PATCHES.md](WRITING-PATCHES.md) for how to write a patch from scratch and [KERF-FORMAT.md](KERF-FORMAT.md) for the full format.

## Pages

- [Start](snippets/start.md): Clock and output
- [Drums](snippets/drums.md): Four-on-the-floor kick, Eighth hats with ghosts, Backbeat snare, Clap with a stray, Full kit
- [Bass](snippets/bass.md): Acid bass, Sub bass, Plucked bass
- [Lead](snippets/lead.md): Saw lead, Wavetable lead, Supersaw lead
- [Pads and chords](snippets/pads.md): Drifting pad, Wavetable pad, Minor chord stab
- [Notes and chords](snippets/notes.md): Chord arpeggio, Drawn melody
- [Percussion and texture](snippets/percussion.md): Noise shaker, Tom polyrhythm, Wooden mallet, Vinyl crackle, Noise wash, Dub siren, Radio static, Sampled grains
- [Modulation](snippets/modulation.md): Rocker filter sweep, Tread step filter, Warp pitch drift, Chatter spring
- [Effects](snippets/effects.md): Tape echo, Reverb, Dub echo into reverb, Filter sweep, Drive and fold, Ducking on the beat, Sidechain compressor, Grain cloud, Dado dub echo, Splice FM
