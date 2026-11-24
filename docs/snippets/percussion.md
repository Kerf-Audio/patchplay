# Percussion and texture

Extra rhythm and background sound: shakers, toms, a struck bar, crackle and noise, plus a dub siren, a radio and a sampled grain cloud.

### Noise shaker

Filtered noise chopped into short bursts on every 16th note.

```kerf snippet
# Noise shaker
# Filtered noise chopped into short bursts on every 16th note.
# Needs: clock, out. See the Clock and output snippet.
# Port: shaker_rhythm.a (gate): the shaker's trigger.
# Port: shaker_amp.out (audio): the shaker, already cabled to out.

part shaker "Shaker" colour #3d9c8a level=0.5
  module shaker_rhythm Ruler hits_a=1 steps_a=1 hits_b=0 hits_c=0 hits_d=0
  module shaker_noise Sawdust colour=0.8 crackle=0
  module shaker_filter Plane cutoff=0.8 res=0.2 mode=0.67 drive=0
  module shaker_env Bevel attack=0 decay=0.12 sustain=0 release=0.08 curve=0.3
  module shaker_amp Clamp level=0 response=0

cable clock.clock -> shaker_rhythm.clock
cable shaker_rhythm.a -> shaker_env.gate
cable shaker_noise.noise -> shaker_filter.in
cable shaker_filter.out -> shaker_amp.in
cable shaker_env.env -> shaker_amp.cv
cable shaker_amp.out -> out.l
cable shaker_amp.out -> out.r
```

### Tom polyrhythm

Two toms on lanes of different lengths, three hits over seven steps against three over nine, so the pattern drifts. Each hit plays four times in five.

```kerf snippet
# Tom polyrhythm
# Two toms on lanes of different lengths, three hits over seven steps against
#   three over nine, so the pattern drifts. Each hit plays four times in five.
# Needs: clock, out. See the Clock and output snippet.
# Port: toms_rhythm.b, toms_rhythm.c (gate): the two tom lanes.
# Port: toms_high.out, toms_low.out (audio): both already cabled to out.

part toms "Toms" colour #3d9c8a level=0.6
  module toms_rhythm Ruler hits_a=0 steps_a=1 hits_b=0.43 steps_b=0.4 hits_c=0.33 steps_c=0.53 hits_d=0 chance=0.8
  module toms_high Mortise engine=1 pitch=0.55 punch=0.6 decay=0.25 tone=0.5
  module toms_low Mortise engine=1 pitch=0.35 punch=0.6 decay=0.35 tone=0.4

cable clock.clock -> toms_rhythm.clock
cable toms_rhythm.b -> toms_high.trig
cable toms_rhythm.c -> toms_low.trig
cable toms_high.out -> out.l
cable toms_high.out -> out.r
cable toms_low.out -> out.l
cable toms_low.out -> out.r
```

### Wooden mallet

A struck wooden bar playing a sparse line in A minor.

```kerf snippet
# Wooden mallet
# A struck wooden bar playing a sparse line in A minor.
# Needs: clock, out. See the Clock and output snippet.
# Port: mallet_seq.pitch (pitch): the notes, before Fret snaps them to the
#   scale.
# Port: mallet_bar.out (audio): the bar, already cabled to out.

part mallet "Wooden mallet" colour #3d9c8a level=0.6
  module mallet_seq Dovetail steps_a=0.47 steps_b=0.6 interlock=0.6 chance=0.7 glide=0 range=0.3
  module mallet_scale Fret root=0.75 scale=0.43 octave=0.5
  module mallet_bar Mallet material=0.1 size=0.3 strike=0.5 damp=0.5

cable clock.clock -> mallet_seq.clock
cable mallet_seq.pitch -> mallet_scale.pitch
cable mallet_scale.pitch -> mallet_bar.voct
cable mallet_seq.gate -> mallet_bar.trig
cable mallet_bar.out -> out.l
cable mallet_bar.out -> out.r
```

### Vinyl crackle

Quiet random crackle over a soft bed of hiss.

```kerf snippet
# Vinyl crackle
# Quiet random crackle over a soft bed of hiss.
# Needs: out. See the Clock and output snippet.
# Port: crackle_noise.dust (audio): the raw crackle, for sending through
#   effects.
# Port: crackle_amp.out (audio): the texture, already cabled to out.

part crackle "Vinyl crackle" colour #3d9c8a level=0.4
  module crackle_noise Sawdust colour=0.7 crackle=0.35
  module crackle_filter Plane cutoff=0.75 res=0.1 mode=0 drive=0
  module crackle_amp Clamp level=0.3 response=0

cable crackle_noise.noise -> crackle_filter.in
cable crackle_filter.out -> crackle_amp.in
cable crackle_noise.dust -> crackle_amp.in
cable crackle_amp.out -> out.l
cable crackle_amp.out -> out.r
```

### Noise wash

Band-passed noise swept slowly up and down by an LFO, for a breathing background.

```kerf snippet
# Noise wash
# Band-passed noise swept slowly up and down by an LFO, for a breathing
#   background.
# Needs: out. See the Clock and output snippet.
# Port: wash_sweep.lfo1 (cv): the slow sweep, free to patch elsewhere.
# Port: wash_amp.out (audio): the wash, already cabled to out.

part wash "Noise wash" colour #3d9c8a level=0.4
  module wash_noise Sawdust colour=0.4 crackle=0
  module wash_sweep Spokeshave rate=0.08 spread=0.5 shape=0 phase=0.25
  module wash_filter Plane cutoff=0.4 res=0.55 mode=0.33 drive=0.1
  module wash_amp Clamp level=0.5 response=0

cable wash_noise.noise -> wash_filter.in
cable wash_sweep.lfo1 -> wash_filter.cutoff
cable wash_filter.out -> wash_amp.in
cable wash_amp.out -> out.l
cable wash_amp.out -> out.r
```

### Dub siren

A siren that dives from a high pitch twice a bar and rings out through a long release.

```kerf snippet
# Dub siren
# A siren that dives from a high pitch twice a bar and rings out through a
#   long release.
# Needs: clock, out. See the Clock and output snippet.
# Port: siren_rhythm.a (gate): when the siren fires.
# Port: siren_voice.lfo (cv): the pitch LFO, 0 to 10 V. Patch it elsewhere to
#   dive other things.
# Port: siren_voice.out (audio): the siren, already cabled to out.

part siren "Dub siren" colour #c94a3d level=0.6
  module siren_rhythm Ruler hits_a=0.125 steps_a=1 hits_b=0 hits_c=0 hits_d=0
  module siren_voice Siren pitch=0.45 range=0.5 rate=0.45 sync=0 shape=0 wave=0.33 release=0.75 cutoff=0.8 resonance=0.2 drive=0.2

cable clock.clock -> siren_rhythm.clock
cable siren_rhythm.a -> siren_voice.gate
cable siren_voice.out -> out.l
cable siren_voice.out -> out.r
```

### Radio static

A radio sitting between stations: hiss, a drifting whistle, and clicks and pops.

```kerf snippet
# Radio static
# A radio sitting between stations: hiss, a drifting whistle, and clicks and
#   pops.
# Needs: out. See the Clock and output snippet.
# Port: radio_static.tune (cv): turns the dial. A slow LFO makes it hunt for
#   stations.
# Port: radio_static.crackle (gate): a pulse for every click and pop.
# Port: radio_static.out (audio): the radio, already cabled to out.

part radio "Radio static" colour #8a8f98 level=0.4
  module radio_static Static tune=0.4 width=0.4 hiss=0.5 crackle=0.3 whistle=0.4 fade=0.3 ghost=0.3 level=0.5

cable radio_static.out -> out.l
cable radio_static.out -> out.r
```

### Sampled grains

A Splinter scans slowly through its sample. Without a file it plays the built-in demo sample.

```kerf snippet
# Sampled grains
# A Splinter scans slowly through its sample. Without a file it plays the
#   built-in demo sample.
# Needs: out. See the Clock and output snippet.
# Port: grain_scan.lfo1 (cv): the slow scan across the sample.
# Port: grain_cloud.trig (gate): fire a burst of eight grains instead of a
#   steady stream.
# Port: grain_cloud.l, grain_cloud.r (audio): the grains, already cabled to
#   out.

part grain "Sampled grains" colour #e0802e level=0.9
  module grain_cloud Splinter position=0.3 spray=0.6 size=0.45 density=0.8 pitch=0.5 scatter=0 reverse=0 shape=0.25 spread=0.5 level=0.8 hold=0
  module grain_scan Spokeshave rate=0.05 spread=0.5 shape=0 phase=0

cable grain_scan.lfo1 -> grain_cloud.position
cable grain_cloud.l -> out.l
cable grain_cloud.r -> out.r
```
