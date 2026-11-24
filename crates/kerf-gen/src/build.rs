//! Random: one patch from a rulebook and a seed.

use kerf_core::{Part, Patch, StyleRef};
use kerf_dsp::Rng;

use crate::place::{add_cables, add_modules};
use crate::rulebook::{Layout, Rulebook};
use crate::util::{index, key_root};

pub(crate) fn build(rb: &Rulebook, seed: u64) -> Patch {
    let mut rng = Rng::new(seed);
    let [lo, hi] = rb.tempo;
    let key = rb.keys[index(&mut rng, rb.keys.len())].clone();
    let root = key_root(&key).expect("rulebook keys are validated");
    let mut patch = Patch {
        tempo: (lo + (hi - lo) * rng.unit()).round(),
        key: Some(key),
        style: Some(StyleRef {
            name: rb.name.clone(),
            seed,
        }),
        ..Patch::default()
    };

    add_modules(&mut patch, &rb.modules, None, root, &mut rng);
    let mut layouts: Vec<Layout> = Vec::new();
    for rule in &rb.parts {
        if rule.chance < 1.0 && rng.unit() >= rule.chance {
            continue;
        }
        let layout = rule.layout(rule.pick_layout(&mut rng, None));
        patch.parts.push(Part {
            id: rule.id.clone(),
            name: rule.name.clone(),
            colour: rule.colour.clone(),
            locked: false,
            muted: false,
            level: Part::unity(),
        });
        add_modules(&mut patch, layout.modules, Some(&rule.id), root, &mut rng);
        layouts.push(layout);
    }
    add_cables(&mut patch, &rb.cables, &mut rng);
    for layout in layouts {
        add_cables(&mut patch, layout.cables, &mut rng);
    }
    patch
}
