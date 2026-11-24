//! A small egui editor: pick a factory starting point, name the patch, turn the macros and the
//! master level, and see whether the patch is playing.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use kerf_bridge::PresetInfo;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Color32, RichText};
use nih_plug_egui::{create_egui_editor, widgets};

use crate::params::PatchplayParams;
use crate::shared::Shared;

struct Ui {
    presets: Vec<PresetInfo>,
    filter: String,
    name: String,
    /// The patch has these controls (the first Jig and the first Finish).
    has_jig: bool,
    has_finish: bool,
    meter: f32,
    /// The document version `name` was read at.
    name_version: Option<u32>,
}

pub fn create(params: Arc<PatchplayParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    let state = params.editor_state.clone();
    let ui_state = Ui {
        presets: shared.presets(),
        filter: String::new(),
        name: String::new(),
        has_jig: false,
        has_finish: false,
        meter: 0.0,
        name_version: None,
    };
    create_egui_editor(
        state,
        ui_state,
        |_, _| {},
        move |ctx, setter, ui_state| draw(ctx, setter, ui_state, &params, &shared),
    )
}

fn draw(
    ctx: &egui::Context,
    setter: &ParamSetter,
    state: &mut Ui,
    params: &PatchplayParams,
    shared: &Shared,
) {
    // A patch has been loaded (a preset, the saved state, a restored project): the host's
    // parameters take its values, so the sliders show what is sounding.
    if shared.take_unsynced() {
        let controls = shared.patch_controls();
        sync(setter, params, controls.macros, controls.level);
    }
    state.has_jig = shared.has_jig.load(Ordering::Relaxed);
    state.has_finish = shared.has_finish.load(Ordering::Relaxed);
    let patch_tempo = f32::from_bits(shared.patch_tempo.load(Ordering::Relaxed));
    let peak = f32::from_bits(shared.peak.swap(0, Ordering::Relaxed));
    state.meter = peak.max(state.meter * 0.9);
    let running = shared.running.load(Ordering::Relaxed);
    let host_tempo = f32::from_bits(shared.host_tempo.load(Ordering::Relaxed));

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("patchplay");

        ui.horizontal(|ui| {
            ui.label("Patch name");
            let edit = ui.text_edit_singleline(&mut state.name);
            if edit.changed() {
                shared.cell().set_name(&state.name);
            } else if !edit.has_focus() && state.name_version != Some(shared.cell().version()) {
                state.name_version = Some(shared.cell().version());
                state.name = shared.cell().name();
            }
        });

        ui.horizontal(|ui| {
            let (text, colour) = if running {
                ("Playing", Color32::from_rgb(110, 190, 120))
            } else {
                ("Stopped", Color32::GRAY)
            };
            ui.label(RichText::new(text).color(colour));
            ui.label(format!("patch {patch_tempo:.0} bpm"));
            if params.follow_tempo.value() && host_tempo > 0.0 {
                ui.label(format!("host {host_tempo:.1} bpm"));
            }
            ui.add(egui::ProgressBar::new(state.meter.clamp(0.0, 1.0)).desired_width(80.0));
        });
        let status = shared.status();
        if !status.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, status);
        }

        ui.separator();
        ui.label("Follow transport");
        ui.add(widgets::ParamSlider::for_param(
            &params.follow_transport,
            setter,
        ));
        ui.label("Follow host tempo");
        ui.add(widgets::ParamSlider::for_param(
            &params.follow_tempo,
            setter,
        ));

        ui.separator();
        if state.has_jig {
            for param in params.macros() {
                ui.label(param.name());
                ui.add(widgets::ParamSlider::for_param(param, setter));
            }
        } else {
            ui.label("This patch has no Jig, so the macros do nothing.");
        }
        if state.has_finish {
            ui.label("Level");
            ui.add(widgets::ParamSlider::for_param(&params.level, setter));
        } else {
            ui.label("This patch has no Finish, so Level does nothing.");
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Starting points");
            ui.text_edit_singleline(&mut state.filter);
        });
        let filter = state.filter.to_lowercase();
        let mut chosen = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for preset in &state.presets {
                let tags = preset.tags.join(" ");
                if !filter.is_empty()
                    && !preset.name.to_lowercase().contains(&filter)
                    && !tags.to_lowercase().contains(&filter)
                {
                    continue;
                }
                let label = format!("{}  ({:.0} bpm) {tags}", preset.name, preset.tempo);
                if ui.selectable_label(false, label).clicked() {
                    chosen = Some(preset.name.clone());
                }
            }
        });
        if let Some(name) = chosen {
            if let Err(e) = shared.load_preset(&name) {
                shared.report(e);
            }
        }
    });
}

/// Moves the host parameters to what the freshly loaded patch holds, so the patch sounds as its
/// author left it and the host sees the new positions.
fn sync(
    setter: &ParamSetter,
    params: &PatchplayParams,
    macros: Option<[f32; 4]>,
    level: Option<f32>,
) {
    let set = |param: &FloatParam, value: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, value);
        setter.end_set_parameter(param);
    };
    if let Some(values) = macros {
        for (param, value) in params.macros().into_iter().zip(values) {
            set(param, value);
        }
    }
    if let Some(value) = level {
        set(&params.level, value);
    }
}
