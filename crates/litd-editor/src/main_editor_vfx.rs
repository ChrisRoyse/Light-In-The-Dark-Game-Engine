use egui::RichText;
use litd_engine::render::vfx::{
    VfxInitializeModule, VfxOutputModule, VfxSpawnModule, VfxUpdateModule,
};

use super::main_editor_frame::panel_width;
use super::*;

impl EditorShellRuntime {
    pub(crate) fn vfx_controls(&mut self, ui: &mut egui::Ui) {
        let width = panel_width(ui);
        let evidence = self.vfx_editor.evidence();
        ui.label(RichText::new(evidence.effect_id.clone()).color(TEXT_DIM));
        ui.label(
            RichText::new(format!(
                "{} emitters / {} particles / {} draw groups",
                evidence.emitter_count, evidence.total_max_particles, evidence.draw_group_count
            ))
            .size(12.0)
            .color(TEXT),
        );

        ui.separator();
        self.vfx_emitter_stack(ui, width);
        ui.separator();
        self.vfx_module_controls(ui);
        ui.separator();
        self.vfx_preview_controls(ui, width);
    }

    fn vfx_emitter_stack(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.label(RichText::new("Emitter Stack").strong().color(TEXT));
        for emitter in &self.vfx_editor.effect().emitters {
            let selected = emitter.display_name == self.vfx_editor.evidence().selected_emitter;
            let label = format!(
                "{}  {}p  {}/{}/{}/{}",
                emitter.display_name,
                emitter.max_particles,
                emitter.spawn.len(),
                emitter.initialize.len(),
                emitter.update.len(),
                emitter.output.len()
            );
            ui.add_sized(
                [width, 24.0],
                egui::Button::selectable(selected, RichText::new(label).color(TEXT)),
            );
        }
    }

    fn vfx_module_controls(&mut self, ui: &mut egui::Ui) {
        let mut spawn_rate = first_spawn_rate(self.vfx_editor.effect()).unwrap_or(2048.0);
        if compact_drag(ui, "Spawn Rate", &mut spawn_rate, 16.0..=8192.0).changed() {
            if let Err(err) = self.vfx_editor.set_spawn_rate(spawn_rate) {
                tracing::error!(error = %err, "vfx spawn-rate edit failed");
            } else if let Err(err) = self.vfx_editor.refresh_preview() {
                tracing::error!(error = %err, "vfx live preview refresh failed");
            }
        }

        let effect = self.vfx_editor.effect();
        let emitter = &effect.emitters[0];
        ui.label(
            RichText::new(module_summary(emitter))
                .size(12.0)
                .color(TEXT_DIM),
        );
        ui.horizontal(|ui| {
            if ui.button("Remove Size Curve").clicked() {
                if let Err(err) = self.vfx_editor.remove_size_over_life() {
                    tracing::error!(error = %err, "vfx size-curve removal failed");
                } else if let Err(err) = self.vfx_editor.refresh_preview() {
                    tracing::error!(error = %err, "vfx preview after size-curve removal failed");
                }
            }
            if ui.button("Restore Curve").clicked() {
                if let Err(err) = self.vfx_editor.restore_size_over_life() {
                    tracing::error!(error = %err, "vfx size-curve restore failed");
                } else if let Err(err) = self.vfx_editor.refresh_preview() {
                    tracing::error!(error = %err, "vfx preview after size-curve restore failed");
                }
            }
        });
    }

    fn vfx_preview_controls(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.horizontal(|ui| {
            if ui.button("Save Asset").clicked()
                && let Err(err) = self.vfx_editor.save_effect()
            {
                tracing::error!(error = %err, "vfx asset save failed");
            }
            if ui.button("Render Preview").clicked()
                && let Err(err) = self.vfx_editor.refresh_preview()
            {
                tracing::error!(error = %err, "vfx preview render failed");
            }
        });
        let evidence = self.vfx_editor.evidence();
        ui.add_sized(
            [width, 22.0],
            egui::Label::new(
                RichText::new(format!(
                    "Preview {} / lit {:.3} / draws {}",
                    compact_path(&evidence.last_preview_path),
                    evidence.last_preview_lit_ratio,
                    evidence.last_preview_draw_calls
                ))
                .size(12.0)
                .color(TEXT_DIM),
            ),
        );
        ui.add_sized(
            [width, 22.0],
            egui::Label::new(
                RichText::new(format!(
                    "Store {} / {}",
                    compact_path(&evidence.last_object_path),
                    short_digest(&evidence.last_digest)
                ))
                .size(12.0)
                .color(TEXT_DIM),
            ),
        );
        if !evidence.last_error.is_empty() {
            ui.label(RichText::new(evidence.last_error).size(12.0).color(ERROR));
        }
    }
}

fn first_spawn_rate(effect: &litd_engine::render::vfx::VfxEffectAsset) -> Option<f32> {
    effect
        .emitters
        .first()?
        .spawn
        .iter()
        .find_map(|module| match module {
            VfxSpawnModule::ConstantRate {
                particles_per_second,
            } => Some(*particles_per_second),
            VfxSpawnModule::Burst { .. } => None,
        })
}

fn module_summary(emitter: &litd_engine::render::vfx::VfxEmitter) -> String {
    let initialize = emitter
        .initialize
        .iter()
        .map(initialize_label)
        .collect::<Vec<_>>()
        .join(", ");
    let update = emitter
        .update
        .iter()
        .map(update_label)
        .collect::<Vec<_>>()
        .join(", ");
    let output = emitter
        .output
        .iter()
        .map(output_label)
        .collect::<Vec<_>>()
        .join(", ");
    format!("Init: {initialize} | Update: {update} | Output: {output}")
}

fn initialize_label(module: &VfxInitializeModule) -> &'static str {
    match module {
        VfxInitializeModule::Shape(_) => "shape",
        VfxInitializeModule::Velocity(_) => "velocity",
        VfxInitializeModule::Color { .. } => "color",
        VfxInitializeModule::Size { .. } => "size",
        VfxInitializeModule::Lifetime { .. } => "lifetime",
    }
}

fn update_label(module: &VfxUpdateModule) -> &'static str {
    match module {
        VfxUpdateModule::Acceleration { .. } => "acceleration",
        VfxUpdateModule::Drag { .. } => "drag",
        VfxUpdateModule::Noise { .. } => "noise",
        VfxUpdateModule::ColorOverLife { .. } => "color/life",
        VfxUpdateModule::SizeOverLife { .. } => "size/life",
        VfxUpdateModule::Collision { .. } => "collision",
    }
}

fn output_label(module: &VfxOutputModule) -> &'static str {
    match module {
        VfxOutputModule::Sprite { .. } => "sprite",
        VfxOutputModule::Mesh { .. } => "mesh",
        VfxOutputModule::Trail { .. } => "trail",
        VfxOutputModule::Light { .. } => "light",
    }
}

fn compact_drag(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_sized(
            [84.0, 18.0],
            egui::Label::new(RichText::new(label).color(TEXT_DIM).size(12.0)),
        );
        ui.add(egui::DragValue::new(value).range(range).speed(16.0))
    })
    .inner
}

fn compact_path(path: &str) -> String {
    if path == "none" {
        return path.to_string();
    }
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn short_digest(digest: &str) -> String {
    digest
        .strip_prefix("sha256:")
        .unwrap_or(digest)
        .chars()
        .take(12)
        .collect()
}
