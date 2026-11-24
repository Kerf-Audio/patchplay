# Drums

Each drum snippet is one part with its own rhythm lane, so drums layer without clashing. The kick, hats and snare are dead straight; only a few lanes use chance.

### Four-on-the-floor kick

A kick on every beat, dead straight.

```kerf snippet
# Four-on-the-floor kick
# A kick on every beat, dead straight.
# Needs: clock, out. See the Clock and output snippet.
# Port: kick_rhythm.a (gate): the kick's trigger. Other modules can follow it.
# Port: kick_drum.out (audio): the kick, already cabled to out.

part kick "Kick" colour #c94a3d level=0.9
  module kick_rhythm Ruler hits_a=0.25 steps_a=1 hits_b=0 hits_c=0 hits_d=0
  module kick_drum Mortise engine=0 pitch=0.35 punch=0.7 decay=0.3 tone=0.45

cable clock.clock -> kick_rhythm.clock
cable kick_rhythm.a -> kick_drum.trig
cable kick_drum.out -> out.l
cable kick_drum.out -> out.r
```

### Eighth hats with ghosts

Closed hats on every eighth note, plus a quiet 16th ghost hat that plays about half the time.

```kerf snippet
# Eighth hats with ghosts
# Closed hats on every eighth note, plus a quiet 16th ghost hat that plays
#   about half the time.
# Needs: clock, out. See the Clock and output snippet.
# Port: hats_rhythm.a (gate): the steady hat. hats_rhythm.b is the ghost lane.
# Port: hats_closed.out, hats_ghost.out (audio): both already cabled to out.

part hats "Hats" colour #c94a3d level=0.6
  module hats_rhythm Ruler hits_a=0.5 steps_a=1 hits_b=0.3 steps_b=1 hits_c=0 hits_d=0 chance=0.45
  module hats_closed Mortise engine=0.5 pitch=0.55 punch=0.5 decay=0.1 tone=0.7
  module hats_ghost Mortise engine=0.5 pitch=0.6 punch=0.3 decay=0.06 tone=0.8

cable clock.clock -> hats_rhythm.clock
cable hats_rhythm.a -> hats_closed.trig
cable hats_rhythm.b -> hats_ghost.trig
cable hats_closed.out -> out.l
cable hats_closed.out -> out.r
cable hats_ghost.out -> out.l
cable hats_ghost.out -> out.r
```

### Backbeat snare

A snare on beats 2 and 4.

```kerf snippet
# Backbeat snare
# A snare on beats 2 and 4.
# Needs: clock, out. See the Clock and output snippet.
# Port: snare_rhythm.a (gate): the snare's trigger.
# Port: snare_drum.out (audio): the snare, already cabled to out.

part snare "Snare" colour #c94a3d level=0.75
  module snare_rhythm Ruler hits_a=0.125 steps_a=1 hits_b=0 hits_c=0 hits_d=0 rotate=0.25
  module snare_drum Mortise engine=0.25 pitch=0.45 punch=0.55 decay=0.4 tone=0.5

cable clock.clock -> snare_rhythm.clock
cable snare_rhythm.a -> snare_drum.trig
cable snare_drum.out -> out.l
cable snare_drum.out -> out.r
```

### Clap with a stray

A clap on beats 2 and 4, and now and then a stray clap between them.

```kerf snippet
# Clap with a stray
# A clap on beats 2 and 4, and now and then a stray clap between them.
# Needs: clock, out. See the Clock and output snippet.
# Port: clap_rhythm.a (gate): the backbeat. clap_rhythm.b is the stray lane.
# Port: clap_drum.out, clap_stray.out (audio): both already cabled to out.

part clap "Clap" colour #c94a3d level=0.7
  module clap_rhythm Ruler hits_a=0.125 steps_a=1 hits_b=0.1875 steps_b=1 hits_c=0 hits_d=0 rotate=0.25 chance=0.3
  module clap_drum Mortise engine=0.75 pitch=0.5 punch=0.6 decay=0.4 tone=0.5
  module clap_stray Mortise engine=0.75 pitch=0.55 punch=0.4 decay=0.25 tone=0.6

cable clock.clock -> clap_rhythm.clock
cable clap_rhythm.a -> clap_drum.trig
cable clap_rhythm.b -> clap_stray.trig
cable clap_drum.out -> out.l
cable clap_drum.out -> out.r
cable clap_stray.out -> out.l
cable clap_stray.out -> out.r
```

### Full kit

Kick, closed hats, backbeat snare and offbeat open hats on the grid, with one ghost lane that plays about half the time.

```kerf snippet
# Full kit
# Kick, closed hats, backbeat snare and offbeat open hats on the grid, with
#   one ghost lane that plays about half the time.
# Needs: clock, out. See the Clock and output snippet.
# Port: kit_beat.a, kit_back.a, kit_off.a (gate): the kick, snare and open hat
#   triggers.
# Port: kit_kick.out, kit_chat.out, kit_snare.out, kit_ohat.out, kit_ghost.out
#   (audio): all already cabled to out.

part kit "Drum kit" colour #c94a3d level=0.8
  module kit_beat Ruler hits_a=0.25 steps_a=1 hits_b=0.5 steps_b=1 hits_c=0 hits_d=0 chance=1
  module kit_back Ruler hits_a=0.125 steps_a=1 hits_b=0 hits_c=0 hits_d=0 rotate=0.25
  module kit_off Ruler hits_a=0.25 steps_a=1 hits_b=0.4 steps_b=0.53 hits_c=0 hits_d=0 rotate=0.125 chance=0.5
  module kit_kick Mortise engine=0 pitch=0.35 punch=0.7 decay=0.3 tone=0.45
  module kit_chat Mortise engine=0.5 pitch=0.55 punch=0.5 decay=0.1 tone=0.65
  module kit_snare Mortise engine=0.25 pitch=0.45 punch=0.55 decay=0.4 tone=0.5
  module kit_ohat Mortise engine=0.5 pitch=0.5 punch=0.4 decay=0.35 tone=0.6
  module kit_ghost Mortise engine=1 pitch=0.8 punch=0.7 decay=0.1 tone=0.6

cable clock.clock -> kit_beat.clock
cable clock.clock -> kit_back.clock
cable clock.clock -> kit_off.clock
cable kit_beat.a -> kit_kick.trig
cable kit_beat.b -> kit_chat.trig
cable kit_back.a -> kit_snare.trig
cable kit_off.a -> kit_ohat.trig
cable kit_off.b -> kit_ghost.trig
cable kit_kick.out -> out.l
cable kit_kick.out -> out.r
cable kit_chat.out -> out.l
cable kit_chat.out -> out.r
cable kit_snare.out -> out.l
cable kit_snare.out -> out.r
cable kit_ohat.out -> out.l
cable kit_ohat.out -> out.r
cable kit_ghost.out -> out.l
cable kit_ghost.out -> out.r
```
