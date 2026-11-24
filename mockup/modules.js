// Kerf Modular mockup: module catalogue. Mirrors kerf-modules descriptors.
// Every knob and jack carries its tooltip; the UI reads help text only from here.

const K = (n, tip, v = 0.5) => ({ n, tip, v });
const I = (n, kind, tip) => ({ n, kind, tip });
const O = (n, kind, tip) => ({ n, kind, tip });

export const KINDS = {
  audio: { label: 'audio', tip: 'Audio: the sound itself, swinging around 0 V.' },
  cv: { label: 'CV', tip: 'Control voltage: moves a knob for you.' },
  pitch: { label: 'pitch', tip: 'Pitch: 1 V per octave.' },
  gate: { label: 'gate', tip: 'Gate or trigger: on above 1 V.' },
  clock: { label: 'clock', tip: 'Clock: steady pulses that keep time.' },
};

export const BANDS = [
  { id: 'time', name: 'Time', tip: 'Decides when things happen and which notes.' },
  { id: 'motion', name: 'Motion', tip: 'Moves knobs for you so the patch changes over time.' },
  { id: 'voices', name: 'Voices', tip: 'Makes sound.' },
  { id: 'shape', name: 'Shape', tip: 'Changes the tone and level of a sound.' },
  { id: 'space', name: 'Space', tip: 'Puts the sound in a room, an echo or a cloud.' },
  { id: 'out', name: 'Out', tip: 'Mixes everything and sends it to your speakers.' },
];

export const MODULES = {
  Tally: {
    band: 'time', hp: 6, desc: 'Clock and transport',
    knobs: [K('Tempo', 'Beats per minute, 40–240. Shift-drag for fine steps.', 0.45), K('Swing', 'Delays every second step. 0 is straight, 1 is a heavy shuffle.', 0.1), K('Divide', 'Sets the speed of the ×2 and ÷4 outputs relative to the beat.')],
    ins: [I('Run', 'gate', 'High = playing. Leave empty to follow the transport.'), I('Reset', 'gate', 'A pulse here jumps back to the first beat.')],
    outs: [O('Clock', 'clock', 'One pulse per 16th note.'), O('×2', 'clock', 'Twice as fast as Clock.'), O('÷4', 'clock', 'One pulse per beat.'), O('Bar', 'gate', 'One pulse at the start of every bar.')],
  },
  Dovetail: {
    band: 'time', hp: 14, desc: 'Two interlocking step lanes', steps: true,
    knobs: [K('Steps A', 'Length of lane A, 1–16 steps.', 0.5), K('Steps B', 'Length of lane B. Different lengths drift against each other.', 0.38), K('Interlock', 'How lane B fits lane A: 0 plays both freely, 1 lets B play only where A rests.', 0.6), K('Chance', 'Probability that each step plays.', 0.85), K('Glide', 'Slides pitch from one step to the next.', 0.15), K('Range', 'Spread of the step pitches, up to 3 octaves.', 0.4)],
    ins: [I('Clock', 'clock', 'Advances one step per pulse.'), I('Reset', 'gate', 'Back to step 1.'), I('Transpose', 'pitch', 'Added to every step. 1 V = 1 octave.')],
    outs: [O('Pitch', 'pitch', 'Pitch of the current step.'), O('Gate', 'gate', 'High while a step plays.'), O('Accent', 'cv', 'Per-step accent level.')],
  },
  Ruler: {
    band: 'time', hp: 12, desc: 'Four Euclidean trigger lanes', steps: true,
    knobs: [K('Hits A', 'How full lane A is: hits spread as evenly as possible over its steps.', 0.25), K('Hits B', 'How full lane B is.', 0.4), K('Hits C', 'How full lane C is.', 0.7), K('Hits D', 'How full lane D is.', 0.15), K('Steps A', 'Length of lane A, 1–16 steps.', 1), K('Steps B', 'Length of lane B. Lanes of different lengths drift against each other.', 0.73), K('Steps C', 'Length of lane C.', 0.4), K('Steps D', 'Length of lane D.', 0.27), K('Rotate', 'Slides every pattern later in time.', 0), K('Chance', 'Probability that each hit on lanes B to D plays. Lane A always plays, so it can hold a straight beat.', 0.9)],
    ins: [I('Clock', 'clock', 'Advances one step per pulse.'), I('Rotate', 'cv', 'Moves Rotate from a voltage.')],
    outs: [O('A', 'gate', 'Lane A triggers.'), O('B', 'gate', 'Lane B triggers.'), O('C', 'gate', 'Lane C triggers.'), O('D', 'gate', 'Lane D triggers.')],
  },
  Fret: {
    band: 'time', hp: 6, desc: 'Quantiser',
    knobs: [K('Root', 'Key centre, C to B.', 0.75), K('Scale', 'Which notes are allowed: major, minor, dorian, pentatonic and more.', 0.3), K('Octave', 'Shifts the output up or down in whole octaves.', 0.5)],
    ins: [I('Pitch', 'pitch', 'Any pitch. It is moved to the nearest note in the scale.')],
    outs: [O('Pitch', 'pitch', 'The incoming pitch, snapped to the scale.'), O('Changed', 'gate', 'Pulses when the note changes.')],
  },
  Spokeshave: {
    band: 'motion', hp: 8, desc: 'Four LFOs spaced like spokes',
    knobs: [K('Rate', 'Speed of LFO 1. The others follow at related rates.', 0.3), K('Spread', 'How far apart the four rates are. 0 = all the same.', 0.5), K('Shape', 'Sine, triangle, ramp, square, and between.', 0.2), K('Phase', 'Offsets the four LFOs from each other.', 0.25)],
    ins: [I('Sync', 'clock', 'Locks the rates to a clock.'), I('Rate', 'cv', 'Speeds up or slows down all four.')],
    outs: [O('1', 'cv', 'LFO 1.'), O('2', 'cv', 'LFO 2.'), O('3', 'cv', 'LFO 3.'), O('4', 'cv', 'LFO 4.')],
  },
  Burl: {
    band: 'motion', hp: 6, desc: 'Random voltages',
    knobs: [K('Rate', 'How often a new random value is picked, when nothing is patched into Trig.', 0.4), K('Smooth', 'How slowly Drift glides to each new value.', 0.6), K('Bias', 'Pushes the random values up or down.', 0.5)],
    ins: [I('Trig', 'gate', 'Picks a new value on every pulse.')],
    outs: [O('Stepped', 'cv', 'Holds each random value until the next.'), O('Drift', 'cv', 'Glides smoothly between random values.'), O('Coin', 'gate', 'Passes each trigger on, or not, at random.')],
  },
  Bevel: {
    band: 'motion', hp: 6, desc: 'Envelope',
    knobs: [K('Attack', 'Time to rise after the gate opens.', 0.05), K('Decay', 'Time to fall to the sustain level.', 0.35), K('Sustain', 'Level held while the gate stays high.', 0.4), K('Release', 'Time to fall to zero after the gate closes.', 0.3), K('Curve', 'Straight lines at 0.5, snappy below, swelling above.', 0.4)],
    ins: [I('Gate', 'gate', 'Starts the envelope; holding it keeps sustain.')],
    outs: [O('Env', 'cv', 'The envelope, 0 to 10 V.'), O('Inverted', 'cv', 'The envelope upside down.'), O('End', 'gate', 'Pulses when the envelope finishes.')],
  },
  Level: {
    band: 'motion', hp: 4, desc: 'Envelope follower',
    knobs: [K('Attack', 'How quickly it follows a sound getting louder.', 0.1), K('Release', 'How quickly it lets go when the sound gets quieter.', 0.4), K('Gain', 'Scales the output.', 0.5)],
    ins: [I('In', 'audio', 'The sound to follow.')],
    outs: [O('Env', 'cv', 'Rises and falls with the loudness of In.'), O('Over', 'gate', 'High while the sound is loud.')],
  },
  Jig: {
    band: 'motion', hp: 8, desc: 'Four macros',
    knobs: [K('M1', 'Macro 1. Patch its output to anything.', 0.5), K('M2', 'Macro 2.', 0.5), K('M3', 'Macro 3.', 0.5), K('M4', 'Macro 4.', 0.5)],
    ins: [I('Morph', 'cv', 'Glides all four macros between scenes A and B.')],
    outs: [O('M1', 'cv', 'Macro 1 as a voltage.'), O('M2', 'cv', 'Macro 2.'), O('M3', 'cv', 'Macro 3.'), O('M4', 'cv', 'Macro 4.')],
  },
  Chisel: {
    band: 'voices', hp: 10, desc: 'Analogue-style oscillator',
    knobs: [K('Tune', 'Coarse pitch, ±2 octaves.', 0.5), K('Fine', 'Fine pitch, ±1 semitone.', 0.5), K('Shape', 'Morphs the Saw output towards a softer triangle.', 0.2), K('PW', 'Pulse width of the Square output.', 0.5), K('FM', 'How much the FM input bends the pitch.', 0)],
    ins: [I('V/Oct', 'pitch', 'Pitch input. 1 V up = 1 octave up.'), I('FM', 'audio', 'Patch audio here for metallic, bell-like tones.'), I('PW', 'cv', 'Moves the pulse width.')],
    outs: [O('Saw', 'audio', 'Bright and buzzy.'), O('Square', 'audio', 'Hollow, woody.'), O('Sine', 'audio', 'Pure and round.'), O('Sub', 'audio', 'One octave down, for weight.')],
  },
  Lathe: {
    band: 'voices', hp: 10, desc: 'Wavetable voice',
    knobs: [K('Tune', 'Coarse pitch, ±2 octaves.', 0.5), K('Table', 'Picks one of 32 wavetables.', 0.3), K('Turn', 'Position in the table. Move it and the timbre turns, like wood on a lathe.', 0.4), K('Warp', 'Bends the waveform for extra harmonics.', 0.2)],
    ins: [I('V/Oct', 'pitch', 'Pitch input.'), I('Turn', 'cv', 'Moves Turn. An LFO here makes the tone evolve.')],
    outs: [O('Out', 'audio', 'The wavetable voice.')],
  },
  Ply: {
    band: 'voices', hp: 10, desc: 'Stacked, detuned voices',
    knobs: [K('Layers', 'How many detuned copies are stacked, 1–9.', 0.6), K('Detune', 'How far apart the layers are tuned.', 0.35), K('Width', 'Spreads the layers across the stereo field.', 0.7), K('Tone', 'Darkens or brightens the stack.', 0.5)],
    ins: [I('V/Oct', 'pitch', 'Pitch of the voice, for example from Fret.'), I('Gate', 'gate', 'Opens each voice.')],
    outs: [O('L', 'audio', 'Left.'), O('R', 'audio', 'Right.')],
  },
  Mallet: {
    band: 'voices', hp: 10, desc: 'Modal percussion',
    knobs: [K('Material', 'Wood, glass, metal, skin.', 0.3), K('Size', 'Bigger objects ring lower.', 0.5), K('Strike', 'Where and how hard the mallet hits.', 0.5), K('Damp', 'How quickly the ringing dies away.', 0.4)],
    ins: [I('Trig', 'gate', 'Strikes the object.'), I('V/Oct', 'pitch', 'Tunes the object.')],
    outs: [O('Out', 'audio', 'The struck sound.')],
  },
  Mortise: {
    band: 'voices', hp: 10, desc: 'Drum voice',
    knobs: [K('Engine', 'Kick, snare, hat, clap or tom.', 0), K('Pitch', 'Tuning of the drum.', 0.4), K('Punch', 'Snap at the start of each hit.', 0.6), K('Decay', 'Length of each hit.', 0.4), K('Tone', 'Darker to brighter.', 0.5)],
    ins: [I('Trig', 'gate', 'Plays a hit.'), I('Accent', 'cv', 'Makes a hit louder and brighter.')],
    outs: [O('Out', 'audio', 'The drum.')],
  },
  Veneer: {
    band: 'voices', hp: 12, desc: 'Sampler and slicer', wave: true,
    knobs: [K('Slice', 'Which slice plays.', 0.2), K('Speed', 'Playback speed and pitch.', 0.5), K('Reverse', 'Plays slices backwards above 0.5.', 0), K('Loop', 'Loops the slice while triggered.', 0)],
    ins: [I('Trig', 'gate', 'Plays the current slice.'), I('Slice', 'cv', 'Picks the slice from a voltage.'), I('V/Oct', 'pitch', 'Repitches playback.')],
    outs: [O('L', 'audio', 'Left.'), O('R', 'audio', 'Right.')],
  },
  Sawdust: {
    band: 'voices', hp: 4, desc: 'Noise',
    knobs: [K('Colour', 'Dark rumble to bright hiss.', 0.5), K('Crackle', 'Density of the vinyl-like crackle.', 0.3)],
    ins: [],
    outs: [O('Noise', 'audio', 'Coloured noise.'), O('Dust', 'audio', 'Random crackles.')],
  },
  Plane: {
    band: 'shape', hp: 8, desc: 'Multimode filter',
    knobs: [K('Cutoff', 'Where the filter starts cutting.', 0.45), K('Res', 'Boost at the cutoff. High settings whistle.', 0.3), K('Mode', 'Low-pass, band-pass, high-pass, notch.', 0), K('Drive', 'Saturates the input.', 0.2)],
    ins: [I('In', 'audio', 'The sound to filter.'), I('Cutoff', 'cv', 'Moves the cutoff. An envelope here makes each note bloom.'), I('V/Oct', 'pitch', 'Makes the cutoff follow the notes.')],
    outs: [O('Out', 'audio', 'The filtered sound.')],
  },
  Clamp: {
    band: 'shape', hp: 6, desc: 'VCA and low-pass gate',
    knobs: [K('Level', 'Base level when nothing is patched into CV.', 0), K('Response', 'Clean VCA at 0, plucky low-pass gate at 1.', 0.2)],
    ins: [I('In', 'audio', 'The sound.'), I('CV', 'cv', 'Opens the clamp. Patch an envelope to turn a drone into notes.')],
    outs: [O('Out', 'audio', 'The shaped sound.')],
  },
  Rasp: {
    band: 'shape', hp: 6, desc: 'Drive and wavefolder',
    knobs: [K('Drive', 'Gain into the clipper.', 0.3), K('Fold', 'Folds loud peaks back for rich harmonics.', 0.1), K('Bias', 'Uneven clipping, for a warmer edge.', 0.5), K('Mix', 'Dry to fully driven.', 1)],
    ins: [I('In', 'audio', 'The sound.'), I('Drive', 'cv', 'Moves Drive.')],
    outs: [O('Out', 'audio', 'The driven sound.')],
  },
  Vise: {
    band: 'shape', hp: 6, desc: 'Compressor',
    knobs: [K('Squeeze', 'How hard loud parts are pushed down.', 0.4), K('Speed', 'Slow and smooth to fast and grabby.', 0.5), K('Makeup', 'Gain added after squeezing.', 0.3)],
    ins: [I('In', 'audio', 'The sound.'), I('Side', 'audio', 'Another sound that triggers the squeeze, for pumping.')],
    outs: [O('Out', 'audio', 'The compressed sound.')],
  },
  Bandsaw: {
    band: 'space', hp: 8, desc: 'Tape delay',
    knobs: [K('Time', 'Delay time, synced when Clock is patched.', 0.375), K('Feedback', 'How many echoes. Near the top it builds up on itself.', 0.45), K('Wear', 'Tape wobble and darkening on each echo.', 0.3), K('Mix', 'Dry to wet.', 0.3)],
    ins: [I('In', 'audio', 'The sound to echo.'), I('Clock', 'clock', 'Syncs the delay time to tempo.'), I('Time', 'cv', 'Moves the delay time; fast moves bend pitch.')],
    outs: [O('L', 'audio', 'Left.'), O('R', 'audio', 'Right.')],
  },
  Shavings: {
    band: 'space', hp: 8, desc: 'Granular cloud',
    knobs: [K('Size', 'Length of each grain.', 0.4), K('Density', 'Grains per second.', 0.5), K('Spray', 'Randomises where each grain is taken from.', 0.3), K('Pitch', 'Transposes the grains.', 0.5)],
    ins: [I('In', 'audio', 'The sound to scatter.'), I('Freeze', 'gate', 'Holds the current buffer.')],
    outs: [O('L', 'audio', 'Left.'), O('R', 'audio', 'Right.')],
  },
  Kiln: {
    band: 'space', hp: 8, desc: 'Reverb',
    knobs: [K('Size', 'Small room to cathedral.', 0.6), K('Heat', 'Damping. Hotter is darker.', 0.4), K('Bloom', 'How slowly the tail swells in.', 0.3), K('Mix', 'Dry to wet.', 0.25)],
    ins: [I('In L', 'audio', 'Left in, or mono.'), I('In R', 'audio', 'Right in.')],
    outs: [O('L', 'audio', 'Left.'), O('R', 'audio', 'Right.')],
  },
  Router: {
    band: 'out', hp: 14, desc: 'Six-channel mixer',
    knobs: [K('1', 'Level of channel 1.', 0.7), K('2', 'Level of channel 2.', 0.7), K('3', 'Level of channel 3.', 0.7), K('4', 'Level of channel 4.', 0.7), K('5', 'Level of channel 5.', 0.7), K('6', 'Level of channel 6.', 0.7), K('Send', 'How much goes to the Send output, for effects.', 0.35)],
    ins: [I('In 1', 'audio', 'Channel 1.'), I('In 2', 'audio', 'Channel 2.'), I('In 3', 'audio', 'Channel 3.'), I('In 4', 'audio', 'Channel 4.'), I('In 5', 'audio', 'Channel 5.'), I('In 6', 'audio', 'Channel 6.')],
    outs: [O('Mix L', 'audio', 'Stereo mix, left.'), O('Mix R', 'audio', 'Stereo mix, right.'), O('Send', 'audio', 'Effects send.')],
  },
  Finish: {
    band: 'out', hp: 6, desc: 'Master output', meter: true,
    knobs: [K('Volume', 'Master level.', 0.7), K('Glue', 'Gentle bus compression.', 0.3), K('Ceiling', 'Limiter ceiling; nothing goes louder than this.', 0.8)],
    ins: [I('L', 'audio', 'Left to speakers.'), I('R', 'audio', 'Right to speakers.')],
    outs: [],
  },
};
