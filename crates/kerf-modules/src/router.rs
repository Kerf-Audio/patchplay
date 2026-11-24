use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Router",
    band: Band::Out,
    hp: 14,
    summary: "Six-channel mixer",
    params: &[
        param("level1", "1", 0.7, "Level of channel 1."),
        param("level2", "2", 0.7, "Level of channel 2."),
        param("level3", "3", 0.7, "Level of channel 3."),
        param("level4", "4", 0.7, "Level of channel 4."),
        param("level5", "5", 0.7, "Level of channel 5."),
        param("level6", "6", 0.7, "Level of channel 6."),
        param(
            "send",
            "Send",
            0.35,
            "How much goes to the Send output, for effects.",
        ),
    ],
    inputs: &[
        jack("in1", "In 1", Audio, "Channel 1."),
        jack("in2", "In 2", Audio, "Channel 2."),
        jack("in3", "In 3", Audio, "Channel 3."),
        jack("in4", "In 4", Audio, "Channel 4."),
        jack("in5", "In 5", Audio, "Channel 5."),
        jack("in6", "In 6", Audio, "Channel 6."),
    ],
    outputs: &[
        jack("mix_l", "Mix L", Audio, "Stereo mix, left."),
        jack("mix_r", "Mix R", Audio, "Stereo mix, right."),
        jack("send", "Send", Audio, "Effects send."),
    ],
};

pub struct State;

/// Level knob to gain: unity at 0.7, +6 dB at the top, silent at 0.
fn gain(level: f32) -> f32 {
    (level / 0.7).powi(2)
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let (levels, send) = io.params.split_at(6);
        let mix: f32 = io
            .inputs
            .iter()
            .zip(levels)
            .map(|(x, &l)| x * gain(l))
            .sum();
        io.outputs[0] = mix;
        io.outputs[1] = mix;
        io.outputs[2] = mix * send[0];
    }
}
