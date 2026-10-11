//! egui material-inspector panel for the no-code PBR material editor (#862).
//!
//! The human front-end over the shared [`crate::material_editor`] capability
//! (the same one the MCP tools drive, #915). The panel owns pure form state that
//! lowers to a [`PbrMaterialAuthorRequest`]; [`MaterialInspectorPanel::show`]
//! draws the widgets and reports the requested action. Actual persistence + GPU
//! preview render are performed by the shell against the shared capability, so
//! the button handlers are thin and the form logic is unit-testable without egui.

use crate::material_editor::{MaterialMapAssignment, MaterialMapSlot, PbrMaterialAuthorRequest};

/// Editable per-map asset-id slot in the inspector form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialMapField {
    pub slot: MaterialMapSlot,
    pub label: &'static str,
    pub asset_id: String,
}

/// Pure form state for the material inspector. Holds exactly the author-facing
/// fields plus the latest preview summary shown beneath the ball.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialInspectorPanel {
    pub material_id: String,
    pub display_name: String,
    pub author: String,
    pub base_color: [f32; 4],
    pub emissive: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
    pub normal_scale: f32,
    pub occlusion: f32,
    pub maps: Vec<MaterialMapField>,
    pub status: String,
    pub preview_summary: Option<MaterialPreviewSummary>,
}

/// Compact preview readout displayed under the live ball.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialPreviewSummary {
    pub foreground_pixels: u32,
    pub mean_luminance: f32,
    pub image_sha256: String,
    pub uniform_fingerprint: String,
}

/// Action the shell should perform after drawing the panel this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialInspectorAction {
    None,
    /// Persist the current form through `author_pbr_material` and re-render preview.
    Author,
    /// Re-render the live preview only (throttled edit feedback).
    Preview,
}

impl Default for MaterialInspectorPanel {
    fn default() -> Self {
        Self {
            material_id: "new_material".to_string(),
            display_name: "New Material".to_string(),
            author: "editor-user".to_string(),
            base_color: [0.8, 0.8, 0.8, 1.0],
            emissive: [0.0, 0.0, 0.0],
            metallic: 0.0,
            roughness: 0.5,
            normal_scale: 1.0,
            occlusion: 1.0,
            maps: vec![
                map_field(MaterialMapSlot::BaseColor, "Albedo (base color)"),
                map_field(MaterialMapSlot::MetallicRoughness, "Metallic-roughness"),
                map_field(MaterialMapSlot::Normal, "Normal"),
                map_field(MaterialMapSlot::AmbientOcclusion, "Occlusion"),
                map_field(MaterialMapSlot::Emissive, "Emissive"),
            ],
            status: String::new(),
            preview_summary: None,
        }
    }
}

impl MaterialInspectorPanel {
    /// Lower the current form to the shared capability request. Empty map asset
    /// ids are dropped (an unassigned slot is simply not bound).
    pub fn to_request(&self) -> PbrMaterialAuthorRequest {
        PbrMaterialAuthorRequest {
            material_id: self.material_id.trim().to_string(),
            display_name: self.display_name.trim().to_string(),
            author: self.author.trim().to_string(),
            base_color: self.base_color,
            metallic: self.metallic,
            roughness: self.roughness,
            normal_scale: self.normal_scale,
            occlusion: self.occlusion,
            emissive: self.emissive,
            maps: self
                .maps
                .iter()
                .filter(|field| !field.asset_id.trim().is_empty())
                .map(|field| MaterialMapAssignment {
                    slot: field.slot,
                    asset_id: field.asset_id.trim().to_string(),
                })
                .collect(),
        }
    }

    /// Draw the inspector; returns the action the shell should perform. Editing a
    /// factor requests a throttled [`MaterialInspectorAction::Preview`] so the
    /// ball updates without re-persisting (perf doctrine #1461/#1462).
    pub fn show(&mut self, ui: &mut egui::Ui) -> MaterialInspectorAction {
        let mut action = MaterialInspectorAction::None;
        let mut factor_changed = false;

        ui.heading("PBR Material");
        ui.horizontal(|ui| {
            ui.label("Id");
            ui.text_edit_singleline(&mut self.material_id);
        });
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.display_name);
        });

        ui.separator();
        ui.label("Base color (albedo)");
        if ui
            .color_edit_button_rgba_unmultiplied(&mut self.base_color)
            .changed()
        {
            factor_changed = true;
        }
        ui.label("Emissive");
        if ui.color_edit_button_rgb(&mut self.emissive).changed() {
            factor_changed = true;
        }

        factor_changed |= self.slider(ui, "Metallic", 0, 0.0, 1.0);
        factor_changed |= self.slider(ui, "Roughness", 1, 0.04, 1.0);
        factor_changed |= self.slider(ui, "Normal scale", 2, 0.0, 8.0);
        factor_changed |= self.slider(ui, "Occlusion", 3, 0.0, 1.0);

        ui.separator();
        ui.label("Texture maps (asset ids)");
        for field in &mut self.maps {
            ui.horizontal(|ui| {
                ui.label(field.label);
                ui.text_edit_singleline(&mut field.asset_id);
            });
        }

        ui.separator();
        if let Some(summary) = &self.preview_summary {
            ui.label(format!(
                "Preview: {} lit px, mean luma {:.1}",
                summary.foreground_pixels, summary.mean_luminance
            ));
            ui.label(format!(
                "uniform fp {}",
                short(&summary.uniform_fingerprint)
            ));
            ui.label(format!("image  fp {}", short(&summary.image_sha256)));
        } else {
            ui.label("Preview: (not yet rendered)");
        }

        ui.horizontal(|ui| {
            if ui.button("Author material").clicked() {
                action = MaterialInspectorAction::Author;
            }
            if ui.button("Refresh preview").clicked() {
                action = MaterialInspectorAction::Preview;
            }
        });
        if !self.status.is_empty() {
            ui.label(&self.status);
        }

        if action == MaterialInspectorAction::None && factor_changed {
            MaterialInspectorAction::Preview
        } else {
            action
        }
    }

    fn slider(&mut self, ui: &mut egui::Ui, label: &str, index: usize, min: f32, max: f32) -> bool {
        let value = match index {
            0 => &mut self.metallic,
            1 => &mut self.roughness,
            2 => &mut self.normal_scale,
            _ => &mut self.occlusion,
        };
        ui.add(egui::Slider::new(value, min..=max).text(label))
            .changed()
    }
}

fn map_field(slot: MaterialMapSlot, label: &'static str) -> MaterialMapField {
    MaterialMapField {
        slot,
        label,
        asset_id: String::new(),
    }
}

fn short(fingerprint: &str) -> &str {
    fingerprint.get(..16).unwrap_or(fingerprint)
}

#[cfg(test)]
#[path = "material_editor_panel_tests.rs"]
mod tests;
