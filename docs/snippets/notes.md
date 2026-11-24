# Notes and chords

Melodies that are not written note by note: a chord maker feeding an arpeggiator, and a tune drawn on a piano roll. They follow the beat clock; the arpeggio plays A minor.

### Chord arpeggio

A slow root note becomes a minor seventh chord, and a Rung plays its notes up and down.

```kerf snippet
# Chord arpeggio
# A slow root note becomes a minor seventh chord, and a Rung plays its notes
#   up and down.
# Needs: clock, out. See the Clock and output snippet.
# Port: arp_seq.pitch (pitch): the chord roots, before Fret snaps them to the
#   scale.
# Port: arp_chord.voice1..voice4 (pitch): the four chord notes, free to patch
#   elsewhere.
# Port: arp_rung.pitch, arp_rung.gate (pitch, gate): the arpeggio, one note
#   per 16th.
# Port: arp_amp.out (audio): the sound, already cabled to out.

part arp "Chord arpeggio" colour #7a4fb5 level=0.5
  module arp_seq Dovetail steps_a=0.2 steps_b=0.2 interlock=0 chance=1 glide=0 range=0.2
  module arp_scale Fret root=0.75 scale=0.43 octave=0.5
  module arp_chord Mitre chord=0.538 inversion=0 spread=0.33 root=0.75
  module arp_rung Rung pattern=0.286 octaves=0.33 length=0.5 swing=0 ratchet=0
  module arp_osc Chisel tune=0.5 shape=0.2 pw=0.5
  module arp_env Bevel attack=0 decay=0.2 sustain=0 release=0.15 curve=0.35
  module arp_filter Plane cutoff=0.6 res=0.2 mode=0 drive=0
  module arp_amp Clamp level=0 response=0.1

cable clock.div4 -> arp_seq.clock
cable arp_seq.pitch -> arp_scale.pitch
cable arp_scale.pitch -> arp_chord.pitch
cable clock.clock -> arp_rung.clock
cable arp_chord.voice1 -> arp_rung.voice1
cable arp_chord.voice2 -> arp_rung.voice2
cable arp_chord.voice3 -> arp_rung.voice3
cable arp_chord.voice4 -> arp_rung.voice4
cable arp_rung.pitch -> arp_osc.voct
cable arp_rung.gate -> arp_env.gate
cable arp_osc.saw -> arp_filter.in
cable arp_env.env -> arp_filter.cutoff
cable arp_filter.out -> arp_amp.in
cable arp_env.env -> arp_amp.cv
cable arp_amp.out -> out.l
cable arp_amp.out -> out.r
```

### Drawn melody

A sixteen-step tune drawn on an Inlay. Steps 5 and 6 tie, and step 9 is accented.

```kerf snippet
# Drawn melody
# A sixteen-step tune drawn on an Inlay. Steps 5 and 6 tie, and step 9 is
#   accented.
# Needs: clock, out. See the Clock and output snippet.
# Port: draw_roll.pitch (pitch): the notes. Change the steps, transpose or
#   octave to rewrite it.
# Port: draw_roll.gate (gate): one gate per note; tied steps share one.
# Port: draw_amp.out (audio): the melody, already cabled to out.

part draw "Drawn melody" colour #e0802e level=0.5
  module draw_roll Inlay length=1 transpose=0.5 octave=0.375 rate=0.6 gate=0.6 glide=0.05 step1=0.04 step3=0.16 step5=0.32 step6=0.32 step7=0.4 step9=0.52 step11=0.16 step13=0.32 step15=0.4
  module draw_osc Chisel tune=0.5 shape=0.15 pw=0.5
  module draw_env Bevel attack=0.01 decay=0.3 sustain=0.3 release=0.2 curve=0.4
  module draw_filter Plane cutoff=0.55 res=0.2 mode=0 drive=0.1
  module draw_amp Clamp level=0 response=0.1

cable clock.clock -> draw_roll.clock
cable draw_roll.pitch -> draw_osc.voct
cable draw_roll.gate -> draw_env.gate
cable draw_osc.saw -> draw_filter.in
cable draw_env.env -> draw_filter.cutoff
cable draw_filter.out -> draw_amp.in
cable draw_env.env -> draw_amp.cv
cable draw_amp.out -> out.l
cable draw_amp.out -> out.r
```
