# Modulation

Each snippet holds a slow source of movement and a held saw it moves, so it makes sound on its own. Patch the CV output into any other module to move that instead. Depth on each source scales how far it goes.

### Rocker filter sweep

A held saw whose filter is rocked by a Rocker. Depth sets how far the sweep goes.

```kerf snippet
# Rocker filter sweep
# A held saw whose filter is rocked by a Rocker. Depth sets how far the sweep
#   goes.
# Needs: out. See the Clock and output snippet.
# Port: rock_lfo.out (cv): the wave, above and below 0 V. Patch it anywhere.
# Port: rock_lfo.pulse (gate): a clock at the speed of the wave.
# Port: rock_amp.out (audio): the sound, already cabled to out.

part rock "Rocker sweep" colour #3d9c8a level=0.4
  module rock_lfo Rocker rate=0.3 sync=0 shape=0.1 depth=0.6 offset=0.5 phase=0
  module rock_osc Chisel tune=0.35 shape=0.1 pw=0.5
  module rock_filter Plane cutoff=0.4 res=0.45 mode=0 drive=0.1
  module rock_amp Clamp level=0.5 response=0

cable rock_lfo.out -> rock_filter.cutoff
cable rock_osc.saw -> rock_filter.in
cable rock_osc.sub -> rock_filter.in
cable rock_filter.out -> rock_amp.in
cable rock_amp.out -> out.l
cable rock_amp.out -> out.r
```

### Tread step filter

Eight steps of filter movement. A Tread walks them forward, one step per 16th.

```kerf snippet
# Tread step filter
# Eight steps of filter movement. A Tread walks them forward, one step per
#   16th.
# Needs: clock, out. See the Clock and output snippet.
# Port: steps_tread.cv (cv): the current step, above and below 0 V.
# Port: steps_tread.step, steps_tread.end (gate): a pulse per step, and one
#   per walk.
# Port: steps_amp.out (audio): the sound, already cabled to out.

part steps "Tread steps" colour #3d63c9 level=0.4
  module steps_tread Tread step1=0.5 step2=0.8 step3=0.35 step4=0.9 step5=0.5 step6=0.2 step7=0.7 step8=0.6 length=1 direction=0 glide=0.1 depth=0.8
  module steps_osc Chisel tune=0.3 shape=0.1 pw=0.5
  module steps_filter Plane cutoff=0.35 res=0.4 mode=0 drive=0.15
  module steps_amp Clamp level=0.5 response=0

cable clock.clock -> steps_tread.clock
cable steps_tread.cv -> steps_filter.cutoff
cable steps_osc.saw -> steps_filter.in
cable steps_filter.out -> steps_amp.in
cable steps_amp.out -> out.l
cable steps_amp.out -> out.r
```

### Warp pitch drift

A Warp lets a held note wander out of tune, the way an old oscillator would.

```kerf snippet
# Warp pitch drift
# A Warp lets a held note wander out of tune, the way an old oscillator would.
# Needs: out. See the Clock and output snippet.
# Port: drift_warp.out (pitch): the wobbling pitch. Patch it to any V/Oct
#   input.
# Port: drift_warp.noise (cv): the wandering on its own, in volts.
# Port: drift_amp.out (audio): the sound, already cabled to out.

part drift "Warp drift" colour #7a4fb5 level=0.4
  module drift_warp Warp amount=0.35 speed=0.4 rough=0.2 chaos=0.3
  module drift_osc Chisel tune=0.4 shape=0.2 pw=0.5
  module drift_filter Plane cutoff=0.55 res=0.2 mode=0 drive=0
  module drift_amp Clamp level=0.5 response=0

cable drift_warp.out -> drift_osc.voct
cable drift_osc.saw -> drift_filter.in
cable drift_osc.sub -> drift_filter.in
cable drift_filter.out -> drift_amp.in
cable drift_amp.out -> out.l
cable drift_amp.out -> out.r
```

### Chatter spring

A spring knocked once a beat. Its swing moves the filter, and it settles before the next knock.

```kerf snippet
# Chatter spring
# A spring knocked once a beat. Its swing moves the filter, and it settles
#   before the next knock.
# Needs: clock, out. See the Clock and output snippet.
# Port: spring_chatter.position (cv): where the spring is, above and below 0
#   V.
# Port: spring_chatter.velocity (cv): how fast it moves. Patch it for a
#   second, offset motion.
# Port: spring_amp.out (audio): the sound, already cabled to out.

part spring "Chatter spring" colour #c94a3d level=0.4
  module spring_chatter Chatter energy=0.7 tension=0.5 damping=0.4 rate=0 depth=1
  module spring_osc Chisel tune=0.3 shape=0.1 pw=0.5
  module spring_filter Plane cutoff=0.4 res=0.4 mode=0 drive=0.1
  module spring_amp Clamp level=0.5 response=0

cable clock.div4 -> spring_chatter.kick
cable spring_chatter.position -> spring_filter.cutoff
cable spring_osc.saw -> spring_filter.in
cable spring_filter.out -> spring_amp.in
cable spring_amp.out -> out.l
cable spring_amp.out -> out.r
```
