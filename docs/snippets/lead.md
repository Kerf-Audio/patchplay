# Lead

Lead lines play notes of A minor up in the middle of the range, and rest now and then because their `chance` is below 1.

### Saw lead

A mono lead mixing a saw and a square, with a filter that blooms on each note. Plays about three steps in four.

```kerf snippet
# Saw lead
# A mono lead mixing a saw and a square, with a filter that blooms on each
#   note. Plays about three steps in four.
# Needs: clock, out. See the Clock and output snippet.
# Port: lead_seq.pitch (pitch): the notes, before Fret snaps them to the
#   scale.
# Port: lead_seq.gate (gate): one pulse per note.
# Port: lead_amp.out (audio): the lead, already cabled to out.

part lead "Saw lead" colour #e0802e level=0.6
  module lead_seq Dovetail steps_a=0.47 steps_b=0.6 interlock=0.4 chance=0.75 glide=0.2 range=0.4
  module lead_scale Fret root=0.75 scale=0.43 octave=0.625
  module lead_osc Chisel tune=0.5 shape=0.1 pw=0.5
  module lead_env Bevel attack=0.01 decay=0.3 sustain=0.3 release=0.25 curve=0.4
  module lead_filter Plane cutoff=0.55 res=0.25 mode=0 drive=0.1
  module lead_amp Clamp level=0 response=0.1

cable clock.clock -> lead_seq.clock
cable lead_seq.pitch -> lead_scale.pitch
cable lead_scale.pitch -> lead_osc.voct
cable lead_osc.saw -> lead_filter.in
cable lead_osc.square -> lead_filter.in
cable lead_seq.gate -> lead_env.gate
cable lead_env.env -> lead_filter.cutoff
cable lead_filter.out -> lead_amp.in
cable lead_env.env -> lead_amp.cv
cable lead_amp.out -> out.l
cable lead_amp.out -> out.r
```

### Wavetable lead

A glassy lead whose timbre turns slowly as an LFO moves through the wavetable.

```kerf snippet
# Wavetable lead
# A glassy lead whose timbre turns slowly as an LFO moves through the
#   wavetable.
# Needs: clock, out. See the Clock and output snippet.
# Port: wt_seq.pitch (pitch): the notes, before Fret snaps them to the scale.
# Port: wt_turn.lfo1 (cv): the slow LFO on the table position. Patch it
#   elsewhere for more movement.
# Port: wt_amp.out (audio): the lead, already cabled to out.

part wavelead "Wavetable lead" colour #e0802e level=0.6
  module wt_seq Dovetail steps_a=0.33 steps_b=0.4 interlock=0.5 chance=0.8 glide=0.1 range=0.35
  module wt_scale Fret root=0.75 scale=0.43 octave=0.625
  module wt_osc Lathe tune=0.5 table=0.3 turn=0.4 warp=0.2
  module wt_turn Spokeshave rate=0.05 spread=0.3 shape=0 phase=0.25
  module wt_env Bevel attack=0.01 decay=0.4 sustain=0.2 release=0.3 curve=0.4
  module wt_amp Clamp level=0 response=0.15

cable clock.clock -> wt_seq.clock
cable wt_seq.pitch -> wt_scale.pitch
cable wt_scale.pitch -> wt_osc.voct
cable wt_turn.lfo1 -> wt_osc.turn
cable wt_osc.out -> wt_amp.in
cable wt_seq.gate -> wt_env.gate
cable wt_env.env -> wt_amp.cv
cable wt_amp.out -> out.l
cable wt_amp.out -> out.r
```

### Supersaw lead

Seven detuned saws spread across the stereo field, opened and closed by the sequencer's gate.

```kerf snippet
# Supersaw lead
# Seven detuned saws spread across the stereo field, opened and closed by the
#   sequencer's gate.
# Needs: clock, out. See the Clock and output snippet.
# Port: super_seq.pitch (pitch): the notes, before Fret snaps them to the
#   scale.
# Port: super_voice.l, super_voice.r (audio): the lead, already cabled to out.

part super "Supersaw lead" colour #e0802e level=0.45
  module super_seq Dovetail steps_a=0.47 steps_b=0.47 interlock=0.5 chance=0.8 glide=0.05 range=0.3
  module super_scale Fret root=0.75 scale=0.43 octave=0.5
  module super_voice Ply layers=0.7 detune=0.4 width=0.8 tone=0.55

cable clock.clock -> super_seq.clock
cable super_seq.pitch -> super_scale.pitch
cable super_scale.pitch -> super_voice.voct
cable super_seq.gate -> super_voice.gate
cable super_voice.l -> out.l
cable super_voice.r -> out.r
```
