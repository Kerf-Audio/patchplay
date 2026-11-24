# Effects

Each effect has an input you feed with a sound and an output already cabled to `out`. Echoes, reverb, grains and FM are wet only: keep the source's own cable to `out` for the dry sound. For the filter, drive and ducking, take the source's cable to `out` away so you hear only the processed sound.

### Tape echo

A tape delay locked to the clock, wet only. Feed it from any sound and keep that sound's own cable to out for the dry signal.

```kerf snippet
# Tape echo
# A tape delay locked to the clock, wet only. Feed it from any sound and keep
#   that sound's own cable to out for the dry signal.
# Needs: clock, out. See the Clock and output snippet.
# Feed it audio: echo_tape.in
# Port: echo_tape.time (cv): move the delay time; fast moves bend the pitch.
# Port: echo_tape.l, echo_tape.r (audio): the echoes, already cabled to out.

part echo "Tape echo" colour #d1a233
  module echo_tape Bandsaw time=0.5 feedback=0.45 wear=0.35 mix=1

cable clock.clock -> echo_tape.clock
cable echo_tape.l -> out.l
cable echo_tape.r -> out.r
```

### Reverb

A large, dark room, wet only. Send sounds into it and keep their own cables to out for the dry signal.

```kerf snippet
# Reverb
# A large, dark room, wet only. Send sounds into it and keep their own cables
#   to out for the dry signal.
# Needs: out. See the Clock and output snippet.
# Feed it audio: room_verb.in_l, room_verb.in_r
# Port: room_verb.l, room_verb.r (audio): the room, already cabled to out. For
#   a mono source, feed in_l alone.

part room "Reverb" colour #d1a233
  module room_verb Kiln size=0.7 heat=0.5 bloom=0.3 mix=1

cable room_verb.l -> out.l
cable room_verb.r -> out.r
```

### Dub echo into reverb

A feedback-heavy tape echo whose repeats run into a room, so each echo gets smeared further back. Wet only.

```kerf snippet
# Dub echo into reverb
# A feedback-heavy tape echo whose repeats run into a room, so each echo gets
#   smeared further back. Wet only.
# Needs: clock, out. See the Clock and output snippet.
# Feed it audio: dub_echo.in
# Port: dub_echo.time (cv): move the echo time; fast moves bend the pitch.
# Port: dub_room.l, dub_room.r (audio): echoes and room together, already
#   cabled to out.

part dub "Dub chain" colour #d1a233
  module dub_echo Bandsaw time=0.5 feedback=0.65 wear=0.5 mix=1
  module dub_room Kiln size=0.85 heat=0.55 bloom=0.6 mix=1

cable clock.clock -> dub_echo.clock
cable dub_echo.l -> dub_room.in_l
cable dub_echo.r -> dub_room.in_r
cable dub_room.l -> out.l
cable dub_room.r -> out.r
```

### Filter sweep

A resonant low-pass opened and closed by a slow LFO. Take the source's cable to out away to hear only the filtered sound.

```kerf snippet
# Filter sweep
# A resonant low-pass opened and closed by a slow LFO. Take the source's cable
#   to out away to hear only the filtered sound.
# Needs: out. See the Clock and output snippet.
# Feed it audio: sweep_filter.in
# Port: sweep_lfo.lfo1 (cv): the sweep. Raise sweep_lfo's rate param to speed
#   it up.
# Port: sweep_filter.out (audio): the filtered sound, already cabled to out.

part sweep "Filter sweep" colour #d1a233
  module sweep_lfo Spokeshave rate=0.1 spread=0.4 shape=0 phase=0.25
  module sweep_filter Plane cutoff=0.25 res=0.55 mode=0 drive=0.1

cable sweep_lfo.lfo1 -> sweep_filter.cutoff
cable sweep_filter.out -> out.l
cable sweep_filter.out -> out.r
```

### Drive and fold

Clips and folds the sound, then rolls off the harsh top. Take the source's cable to out away to hear only the driven sound.

```kerf snippet
# Drive and fold
# Clips and folds the sound, then rolls off the harsh top. Take the source's
#   cable to out away to hear only the driven sound.
# Needs: out. See the Clock and output snippet.
# Feed it audio: drive_rasp.in
# Port: drive_rasp.drive (cv): move the drive from an envelope or LFO.
# Port: drive_tone.out (audio): the driven sound, already cabled to out.

part drive "Drive and fold" colour #d1a233
  module drive_rasp Rasp drive=0.6 fold=0.25 bias=0.55 mix=1
  module drive_tone Plane cutoff=0.75 res=0.1 mode=0 drive=0

cable drive_rasp.out -> drive_tone.in
cable drive_tone.out -> out.l
cable drive_tone.out -> out.r
```

### Ducking on the beat

Pulls a sound down on every beat and lets it swell back, so it sits around a four-on-the-floor kick. Move the source's cable from out to duck_amp.in.

```kerf snippet
# Ducking on the beat
# Pulls a sound down on every beat and lets it swell back, so it sits around a
#   four-on-the-floor kick. Move the source's cable from out to duck_amp.in.
# Needs: clock, out. See the Clock and output snippet.
# Feed it audio: duck_amp.in
# Port: duck_env.inverted (cv): the duck shape, for ducking other things too.
# Port: duck_amp.out (audio): the ducked sound, already cabled to out.

part duck "Ducking" colour #d1a233
  module duck_rhythm Ruler hits_a=0.25 steps_a=1 hits_b=0 hits_c=0 hits_d=0
  module duck_env Bevel attack=0 decay=0.4 sustain=0 release=0.4 curve=0.3
  module duck_amp Clamp level=0 response=0

cable clock.clock -> duck_rhythm.clock
cable duck_rhythm.a -> duck_env.gate
cable duck_env.inverted -> duck_amp.cv
cable duck_amp.out -> out.l
cable duck_amp.out -> out.r
```

### Sidechain compressor

A compressor that squeezes one sound whenever another is loud, for real pumping. Cable a pad to in and the kick (kick_drum.out) to side, and move the pad's cable from out to this.

```kerf snippet
# Sidechain compressor
# A compressor that squeezes one sound whenever another is loud, for real
#   pumping. Cable a pad to in and the kick (kick_drum.out) to side, and move
#   the pad's cable from out to this.
# Needs: out. See the Clock and output snippet.
# Feed it audio: pump_comp.in, pump_comp.side
# Port: pump_comp.out (audio): the squeezed sound, already cabled to out.

part pump "Sidechain" colour #d1a233
  module pump_comp Vise squeeze=0.8 speed=0.6 makeup=0.4

cable pump_comp.out -> out.l
cable pump_comp.out -> out.r
```

### Grain cloud

Scatters whatever you feed it into a cloud of short grains, wet only. Good on a pad or a mallet.

```kerf snippet
# Grain cloud
# Scatters whatever you feed it into a cloud of short grains, wet only. Good
#   on a pad or a mallet.
# Needs: out. See the Clock and output snippet.
# Feed it audio: cloud_grains.in
# Port: cloud_grains.freeze (gate): hold the current buffer while the gate is
#   high.
# Port: cloud_grains.l, cloud_grains.r (audio): the cloud, already cabled to
#   out.

part cloud "Grain cloud" colour #d1a233
  module cloud_grains Shavings size=0.4 density=0.6 spray=0.4 pitch=0.5

cable cloud_grains.l -> out.l
cable cloud_grains.r -> out.r
```

### Dado dub echo

A dub echo whose repeats get duller and thinner. Each one comes back a little more worn. Wet only.

```kerf snippet
# Dado dub echo
# A dub echo whose repeats get duller and thinner. Each one comes back a
#   little more worn. Wet only.
# Needs: out. See the Clock and output snippet.
# Feed it audio: dado_echo.in_l
# Port: dado_echo.throw (gate): when patched, sound enters the echoes only
#   while this is high.
# Port: dado_echo.time (cv): move the echo time; sweeps make the repeats dive
#   and climb.
# Port: dado_echo.l, dado_echo.r (audio): the echoes, already cabled to out.

part dado "Dado echo" colour #d1a233
  module dado_echo Dado time=0.68 sync=0 feedback=0.6 tone=0.45 thin=0.25 wobble=0.3 drive=0.25 mix=1

cable dado_echo.l -> out.l
cable dado_echo.r -> out.r
```

### Splice FM

Bends a sound's pitch with a built-in sine tuned an octave above it. Gives bell and metal tones.

```kerf snippet
# Splice FM
# Bends a sound's pitch with a built-in sine tuned an octave above it. Gives
#   bell and metal tones.
# Needs: out. See the Clock and output snippet.
# Feed it audio: splice_fx.carrier
# Port: splice_fx.mod (audio): patch a second sound here to modulate with it
#   instead of the sine.
# Port: splice_fx.out (audio): the modulated sound, already cabled to out.

part splice "Splice FM" colour #3d9c8a
  module splice_fx Splice mode=0 amount=0.5 coarse=0.5 fine=0.5 mix=1 level=0.5

cable splice_fx.out -> out.l
cable splice_fx.out -> out.r
```
