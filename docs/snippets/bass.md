# Bass

Bass lines play notes of A minor, two octaves down. Change `root` and `scale` on the Fret to change the key.

### Acid bass

A resonant saw line in A minor. The filter opens and closes with every note.

```kerf snippet
# Acid bass
# A resonant saw line in A minor. The filter opens and closes with every note.
# Needs: clock, out. See the Clock and output snippet.
# Port: acid_seq.pitch (pitch): the line's notes, before Fret snaps them to
#   the scale.
# Port: acid_seq.gate (gate): one pulse per note.
# Port: acid_amp.out (audio): the bass, already cabled to out.

part acid "Acid bass" colour #3d63c9 level=0.8
  module acid_seq Dovetail steps_a=1 steps_b=1 interlock=0.6 chance=1 glide=0.25 range=0.3
  module acid_scale Fret root=0.75 scale=0.43 octave=0.25
  module acid_osc Chisel tune=0.5 shape=0 pw=0.5
  module acid_env Bevel attack=0 decay=0.38 sustain=0 release=0.2 curve=0.3
  module acid_filter Plane cutoff=0.28 res=0.8 mode=0 drive=0.3
  module acid_amp Clamp level=0 response=0.1

cable clock.clock -> acid_seq.clock
cable acid_seq.pitch -> acid_scale.pitch
cable acid_scale.pitch -> acid_osc.voct
cable acid_seq.gate -> acid_env.gate
cable acid_osc.saw -> acid_filter.in
cable acid_env.env -> acid_filter.cutoff
cable acid_filter.out -> acid_amp.in
cable acid_env.env -> acid_amp.cv
cable acid_amp.out -> out.l
cable acid_amp.out -> out.r
```

### Sub bass

A slow sine with an octave-down layer, one note per beat, gliding between four notes of A minor.

```kerf snippet
# Sub bass
# A slow sine with an octave-down layer, one note per beat, gliding between
#   four notes of A minor.
# Needs: clock, out. See the Clock and output snippet.
# Port: sub_seq.pitch (pitch): the notes, before Fret snaps them to the scale.
# Port: sub_amp.out (audio): the bass, already cabled to out.

part sub "Sub bass" colour #3d63c9 level=0.9
  module sub_seq Dovetail steps_a=0.2 steps_b=0.2 interlock=1 chance=1 glide=0.1 range=0.1
  module sub_scale Fret root=0.75 scale=0.43 octave=0.25
  module sub_osc Chisel tune=0.5 shape=0 pw=0.5
  module sub_env Bevel attack=0.02 decay=0.5 sustain=0.7 release=0.3 curve=0.5
  module sub_amp Clamp level=0 response=0

cable clock.div4 -> sub_seq.clock
cable sub_seq.pitch -> sub_scale.pitch
cable sub_scale.pitch -> sub_osc.voct
cable sub_seq.gate -> sub_env.gate
cable sub_osc.sine -> sub_amp.in
cable sub_osc.sub -> sub_amp.in
cable sub_env.env -> sub_amp.cv
cable sub_amp.out -> out.l
cable sub_amp.out -> out.r
```

### Plucked bass

A short, round square-wave bass: a low-pass gate closes after each note, eight steps to a bar.

```kerf snippet
# Plucked bass
# A short, round square-wave bass: a low-pass gate closes after each note,
#   eight steps to a bar.
# Needs: clock, out. See the Clock and output snippet.
# Port: pluck_seq.pitch (pitch): the notes, before Fret snaps them to the
#   scale.
# Port: pluck_amp.out (audio): the bass, already cabled to out.

part pluck "Plucked bass" colour #3d63c9 level=0.8
  module pluck_seq Dovetail steps_a=0.47 steps_b=0.47 interlock=0.5 chance=1 glide=0 range=0.2
  module pluck_scale Fret root=0.75 scale=0.43 octave=0.25
  module pluck_osc Chisel tune=0.5 shape=0 pw=0.5
  module pluck_env Bevel attack=0 decay=0.25 sustain=0 release=0.1 curve=0.3
  module pluck_amp Clamp level=0 response=0.8

cable clock.clock -> pluck_seq.clock
cable pluck_seq.pitch -> pluck_scale.pitch
cable pluck_scale.pitch -> pluck_osc.voct
cable pluck_seq.gate -> pluck_env.gate
cable pluck_osc.square -> pluck_amp.in
cable pluck_env.env -> pluck_amp.cv
cable pluck_amp.out -> out.l
cable pluck_amp.out -> out.r
```
