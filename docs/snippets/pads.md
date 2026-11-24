# Pads and chords

Slow, sustained sounds. They follow the beat clock, so the notes change once a beat or more slowly.

### Drifting pad

A wide, stacked pad that drones and swells in and out, moving between four notes of A minor.

```kerf snippet
# Drifting pad
# A wide, stacked pad that drones and swells in and out, moving between four
#   notes of A minor.
# Needs: clock, out. See the Clock and output snippet.
# Port: pad_seq.pitch (pitch): the notes, before Fret snaps them to the scale.
# Port: pad_swell.lfo1 (cv): the slow swell. Patch it elsewhere to move
#   something with the pad.
# Port: pad_amp_l.out, pad_amp_r.out (audio): the pad, already cabled to out.

part pad "Drifting pad" colour #7a4fb5 level=0.5
  module pad_seq Dovetail steps_a=0.2 steps_b=0.2 interlock=0 chance=1 glide=0.6 range=0.2
  module pad_scale Fret root=0.75 scale=0.43 octave=0.5
  module pad_voice Ply layers=0.7 detune=0.3 width=0.9 tone=0.35
  module pad_swell Spokeshave rate=0.05 spread=0.3 shape=0 phase=0.25
  module pad_amp_l Clamp level=0.4 response=0
  module pad_amp_r Clamp level=0.4 response=0

cable clock.div4 -> pad_seq.clock
cable pad_seq.pitch -> pad_scale.pitch
cable pad_scale.pitch -> pad_voice.voct
cable pad_voice.l -> pad_amp_l.in
cable pad_voice.r -> pad_amp_r.in
cable pad_swell.lfo1 -> pad_amp_l.cv
cable pad_swell.lfo1 -> pad_amp_r.cv
cable pad_amp_l.out -> out.l
cable pad_amp_r.out -> out.r
```

### Wavetable pad

A held wavetable tone that changes its timbre slowly, with a gentle low-pass.

```kerf snippet
# Wavetable pad
# A held wavetable tone that changes its timbre slowly, with a gentle
#   low-pass.
# Needs: clock, out. See the Clock and output snippet.
# Port: wpad_seq.pitch (pitch): the notes, before Fret snaps them to the
#   scale.
# Port: wpad_turn.lfo1 (cv): the slow LFO on the table position.
# Port: wpad_filter.out (audio): the pad, already cabled to out.

part wpad "Wavetable pad" colour #7a4fb5 level=0.5
  module wpad_seq Dovetail steps_a=0.2 steps_b=0.2 interlock=0 chance=1 glide=0.5 range=0.2
  module wpad_scale Fret root=0.75 scale=0.43 octave=0.5
  module wpad_osc Lathe tune=0.5 table=0.55 turn=0.5 warp=0.1
  module wpad_turn Spokeshave rate=0.04 spread=0.4 shape=0 phase=0.25
  module wpad_filter Plane cutoff=0.55 res=0.15 mode=0 drive=0

cable clock.div4 -> wpad_seq.clock
cable wpad_seq.pitch -> wpad_scale.pitch
cable wpad_scale.pitch -> wpad_osc.voct
cable wpad_turn.lfo1 -> wpad_osc.turn
cable wpad_osc.out -> wpad_filter.in
cable wpad_filter.out -> out.l
cable wpad_filter.out -> out.r
```

### Minor chord stab

Three saws a minor triad apart, stabbed on a three-over-sixteen rhythm. The root changes every beat.

```kerf snippet
# Minor chord stab
# Three saws a minor triad apart, stabbed on a three-over-sixteen rhythm. The
#   root changes every beat.
# Needs: clock, out. See the Clock and output snippet.
# Port: chords_seq.pitch (pitch): the chord roots, before Fret snaps them to
#   the scale.
# Port: chords_rhythm.a (gate): the stab rhythm.
# Port: chords_amp.out (audio): the chord, already cabled to out.

part chords "Chord stab" colour #7a4fb5 level=0.4
  module chords_rhythm Ruler hits_a=0.1875 steps_a=1 hits_b=0 hits_c=0 hits_d=0
  module chords_seq Dovetail steps_a=0.2 steps_b=0.2 interlock=0 chance=1 glide=0 range=0.15
  module chords_scale Fret root=0.75 scale=0.43 octave=0.5
  module chords_root Chisel tune=0.5 shape=0.2 pw=0.5
  module chords_third Chisel tune=0.5625 shape=0.2 pw=0.5
  module chords_fifth Chisel tune=0.646 shape=0.2 pw=0.5
  module chords_env Bevel attack=0 decay=0.3 sustain=0 release=0.2 curve=0.35
  module chords_filter Plane cutoff=0.6 res=0.2 mode=0 drive=0
  module chords_amp Clamp level=0 response=0.4

cable clock.div4 -> chords_seq.clock
cable clock.clock -> chords_rhythm.clock
cable chords_seq.pitch -> chords_scale.pitch
cable chords_scale.pitch -> chords_root.voct
cable chords_scale.pitch -> chords_third.voct
cable chords_scale.pitch -> chords_fifth.voct
cable chords_rhythm.a -> chords_env.gate
cable chords_root.saw -> chords_filter.in
cable chords_third.saw -> chords_filter.in
cable chords_fifth.saw -> chords_filter.in
cable chords_env.env -> chords_filter.cutoff
cable chords_filter.out -> chords_amp.in
cable chords_env.env -> chords_amp.cv
cable chords_amp.out -> out.l
cable chords_amp.out -> out.r
```
