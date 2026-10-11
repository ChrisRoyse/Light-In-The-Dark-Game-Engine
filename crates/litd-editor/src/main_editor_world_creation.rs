use std::path::PathBuf;

use egui::RichText;
use litd_editor::{
    asset_placement::{AssetPlacementReport, ContentBrowserAssetRef, place_assets_in_world_record},
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties, EditorWorldRecord,
        MAX_WORLD_DIMENSION_TILES, MAX_WORLD_LEVEL, MIN_WORLD_DIMENSION_TILES, MIN_WORLD_LEVEL,
        WorldAsterReadback, aster_dir_from_env, create_editor_world_record, parse_world_record,
        write_world_record,
    },
};
use litd_engine::{EngineError, math::Vec3};
use serde::{Deserialize, Serialize};
use tracing::info;

use super::main_editor_frame::panel_width;
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct WorldCreationDraft {
    calyx_document_id: String,
    revision_id: String,
    display_name: String,
    description: String,
    width_tiles: u32,
    height_tiles: u32,
    tileset_id: String,
    initial_tile_id: String,
    initial_cliff_type: String,
    initial_cliff_level: i16,
    water_level: i16,
    random_height_seed_enabled: bool,
    random_height_seed: u64,
    loading_screen_asset_ref: String,
    preview_asset_ref: String,
}

#[derive(Debug, Clone)]
pub(super) struct WorldCreationRuntime {
    aster_dir: PathBuf,
    draft: WorldCreationDraft,
    status: String,
    last_error: String,
    create_count: u32,
    last_document_id: String,
    last_summary: String,
    last_terrain_cell_count: usize,
    last_default_tile_count: usize,
    last_aster_seq: u64,
    last_aster_graph_rows: usize,
    placement_count: u32,
    active_record: Option<EditorWorldRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorldCreationEvidence {
    pub(crate) status: String,
    pub(crate) create_count: u32,
    pub(crate) document_id: String,
    pub(crate) summary: String,
    pub(crate) terrain_cell_count: usize,
    pub(crate) default_tile_count: usize,
    pub(crate) store_summary: String,
    pub(crate) aster_summary: String,
    pub(crate) last_error: String,
}

impl Default for WorldCreationDraft {
    fn default() -> Self {
        let spec = EditorWorldCreationSpec::issue1210_default();
        Self::from_spec(&spec)
    }
}

impl WorldCreationDraft {
    fn from_spec(spec: &EditorWorldCreationSpec) -> Self {
        let properties = &spec.properties;
        Self {
            calyx_document_id: spec.calyx_document_id.clone(),
            revision_id: spec.revision_id.clone(),
            display_name: properties.display_name.clone(),
            description: properties.description.clone(),
            width_tiles: properties.width_tiles,
            height_tiles: properties.height_tiles,
            tileset_id: properties.tileset_id.clone(),
            initial_tile_id: properties.initial_tile_id.clone(),
            initial_cliff_type: properties.initial_cliff_type.clone(),
            initial_cliff_level: properties.initial_cliff_level,
            water_level: properties.water_level,
            random_height_seed_enabled: properties.random_height_seed.is_some(),
            random_height_seed: properties.random_height_seed.unwrap_or(1210),
            loading_screen_asset_ref: properties.loading_screen_asset_ref.clone(),
            preview_asset_ref: properties.preview_asset_ref.clone(),
        }
    }

    fn to_spec(&self) -> EditorWorldCreationSpec {
        EditorWorldCreationSpec {
            calyx_document_id: self.calyx_document_id.clone(),
            revision_id: self.revision_id.clone(),
            properties: EditorWorldProperties {
                display_name: self.display_name.clone(),
                description: self.description.clone(),
                width_tiles: self.width_tiles,
                height_tiles: self.height_tiles,
                tileset_id: self.tileset_id.clone(),
                initial_tile_id: self.initial_tile_id.clone(),
                initial_cliff_type: self.initial_cliff_type.clone(),
                initial_cliff_level: self.initial_cliff_level,
                water_level: self.water_level,
                random_height_seed: self
                    .random_height_seed_enabled
                    .then_some(self.random_height_seed),
                playable_bounds: EditorPlayableBounds {
                    min_tile_x: 0,
                    min_tile_z: 0,
                    max_tile_x_exclusive: self.width_tiles as i32,
                    max_tile_z_exclusive: self.height_tiles as i32,
                },
                loading_screen_asset_ref: self.loading_screen_asset_ref.clone(),
                preview_asset_ref: self.preview_asset_ref.clone(),
            },
        }
    }

    fn reset_defaults(&mut self) {
        *self = Self::default();
    }
}

impl WorldCreationRuntime {
    pub(super) fn from_env() -> Result<Self, EngineError> {
        Self::new(aster_dir_from_env()?)
    }

    #[cfg(test)]
    pub(super) fn new(aster_dir: PathBuf) -> Result<Self, EngineError> {
        Ok(Self {
            aster_dir,
            draft: WorldCreationDraft::default(),
            status: "idle".to_string(),
            last_error: String::new(),
            create_count: 0,
            last_document_id: "none".to_string(),
            last_summary: "none".to_string(),
            last_terrain_cell_count: 0,
            last_default_tile_count: 0,
            last_aster_seq: 0,
            last_aster_graph_rows: 0,
            placement_count: 0,
            active_record: None,
        })
    }

    #[cfg(not(test))]
    fn new(aster_dir: PathBuf) -> Result<Self, EngineError> {
        Ok(Self {
            aster_dir,
            draft: WorldCreationDraft::default(),
            status: "idle".to_string(),
            last_error: String::new(),
            create_count: 0,
            last_document_id: "none".to_string(),
            last_summary: "none".to_string(),
            last_terrain_cell_count: 0,
            last_default_tile_count: 0,
            last_aster_seq: 0,
            last_aster_graph_rows: 0,
            placement_count: 0,
            active_record: None,
        })
    }

    fn create_from_draft(&mut self) -> Result<(), EngineError> {
        let spec = self.draft.to_spec();
        let record = create_editor_world_record(spec)?;
        let readback = write_world_record(&self.aster_dir, &record)?;
        let parsed = parse_world_record(&readback.bytes)?;
        self.record_success(&parsed, readback);
        Ok(())
    }

    fn record_success(&mut self, record: &EditorWorldRecord, readback: WorldAsterReadback) {
        self.status = format!("created:{}", record.document_fingerprint);
        self.last_error.clear();
        self.create_count = self.create_count.saturating_add(1);
        self.last_document_id = record.spec.calyx_document_id.clone();
        self.last_summary = record.summary.clone();
        self.last_terrain_cell_count = record.terrain_cell_count;
        self.last_default_tile_count = record.default_tile_count;
        self.last_aster_seq = readback.seq;
        self.last_aster_graph_rows = readback.graph_rows;
        self.active_record = Some(record.clone());
        info!(
            document_id = %self.last_document_id,
            seq = readback.seq,
            graph_rows = readback.graph_rows,
            terrain_cells = record.terrain_cell_count,
            summary = %record.summary,
            "editor world created and read back from Aster"
        );
    }

    fn record_error(&mut self, error: &EngineError) {
        self.status = format!("error:{}", error.code());
        self.last_error = error.to_string();
    }

    fn place_content_browser_assets(
        &mut self,
        assets: Vec<ContentBrowserAssetRef>,
        drop_world: Vec3,
    ) -> Result<AssetPlacementReport, EngineError> {
        let record =
            self.active_record
                .as_mut()
                .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                    field: "editor.world_creation.asset_placement.active_world".to_string(),
                    reason: "create or load a world before dropping content-browser assets"
                        .to_string(),
                })?;
        let request = litd_editor::asset_placement::AssetDropRequest {
            request_id: format!("issue1223_drop_{}", self.placement_count.saturating_add(1)),
            assets,
            drop_world,
            surface_valid: true,
            snap_grid: Some(1.0),
            clamp_to_playable_bounds: true,
            multi_asset_spacing: 2.0,
        };
        let report = place_assets_in_world_record(record, &request)?;
        let readback = write_world_record(&self.aster_dir, record)?;
        let parsed = parse_world_record(&readback.bytes)?;
        self.status = format!("placed:{}", parsed.document_fingerprint);
        self.last_error.clear();
        self.placement_count = self.placement_count.saturating_add(1);
        self.last_document_id = parsed.spec.calyx_document_id.clone();
        self.last_summary = parsed.summary.clone();
        self.last_terrain_cell_count = parsed.terrain_cell_count;
        self.last_default_tile_count = parsed.default_tile_count;
        self.last_aster_seq = readback.seq;
        self.last_aster_graph_rows = readback.graph_rows;
        self.active_record = Some(parsed);
        Ok(report)
    }

    fn evidence(&self) -> WorldCreationEvidence {
        WorldCreationEvidence {
            status: self.status.clone(),
            create_count: self.create_count,
            document_id: self.last_document_id.clone(),
            summary: self.last_summary.clone(),
            terrain_cell_count: self.last_terrain_cell_count,
            default_tile_count: self.last_default_tile_count,
            store_summary: format!("aster={}", self.aster_dir.display()),
            aster_summary: format!(
                "dir={};seq={};graph_rows={}",
                self.aster_dir.display(),
                self.last_aster_seq,
                self.last_aster_graph_rows
            ),
            last_error: self.last_error.clone(),
        }
    }
}

impl EditorShellRuntime {
    pub(crate) fn create_world_from_draft_command(&mut self) -> Result<(), EngineError> {
        match self.world_creation.create_from_draft() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.world_creation.record_error(&error);
                Err(error)
            }
        }
    }

    pub(crate) fn world_creation_evidence(&self) -> WorldCreationEvidence {
        self.world_creation.evidence()
    }

    pub(crate) fn place_content_browser_assets_command(
        &mut self,
        assets: Vec<ContentBrowserAssetRef>,
        drop_world: Vec3,
    ) -> Result<AssetPlacementReport, EngineError> {
        match self
            .world_creation
            .place_content_browser_assets(assets, drop_world)
        {
            Ok(report) => Ok(report),
            Err(error) => {
                self.world_creation.record_error(&error);
                Err(error)
            }
        }
    }

    pub(crate) fn world_creation_last_error_message(&self) -> Option<&str> {
        (!self.world_creation.last_error.trim().is_empty())
            .then_some(self.world_creation.last_error.as_str())
    }

    pub(super) fn world_creation_controls(&mut self, ui: &mut egui::Ui) {
        let compact = panel_width(ui) < 260.0;
        ui.label(RichText::new("World").strong().color(GOLD));
        text_field(
            ui,
            "Name",
            &mut self.world_creation.draft.display_name,
            compact,
        );
        numeric_pair(
            ui,
            "Tiles",
            &mut self.world_creation.draft.width_tiles,
            &mut self.world_creation.draft.height_tiles,
        );
        text_field(
            ui,
            "Tileset",
            &mut self.world_creation.draft.tileset_id,
            compact,
        );
        text_field(
            ui,
            "Tile",
            &mut self.world_creation.draft.initial_tile_id,
            compact,
        );
        level_pair(
            ui,
            "Level",
            &mut self.world_creation.draft.initial_cliff_level,
            &mut self.world_creation.draft.water_level,
        );
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.world_creation.draft.random_height_seed_enabled,
                "Seed",
            );
            ui.add(
                egui::DragValue::new(&mut self.world_creation.draft.random_height_seed)
                    .speed(1)
                    .range(0..=u64::MAX),
            );
        });
        text_field(
            ui,
            "Loading",
            &mut self.world_creation.draft.loading_screen_asset_ref,
            compact,
        );
        ui.horizontal(|ui| {
            if ui.button("Create World").clicked()
                && let Err(err) = self.create_world_from_draft_command()
            {
                tracing::error!(error = %err, "world creation failed");
            }
            if ui.button("Reset").clicked() {
                self.world_creation.draft.reset_defaults();
            }
        });
        ui.label(RichText::new(self.world_creation.status.as_str()).color(
            if self.world_creation.status.starts_with("error:") {
                ERROR
            } else {
                TEXT_DIM
            },
        ));
    }
}

fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String, compact: bool) {
    if compact {
        ui.label(RichText::new(label).color(TEXT_DIM));
        ui.text_edit_singleline(value);
    } else {
        ui.horizontal(|ui| {
            ui.set_min_width(84.0);
            ui.label(RichText::new(label).color(TEXT_DIM));
            ui.text_edit_singleline(value);
        });
    }
}

fn numeric_pair(ui: &mut egui::Ui, label: &str, width: &mut u32, height: &mut u32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(TEXT_DIM));
        ui.add(
            egui::DragValue::new(width)
                .speed(1)
                .range(MIN_WORLD_DIMENSION_TILES..=MAX_WORLD_DIMENSION_TILES),
        );
        ui.add(
            egui::DragValue::new(height)
                .speed(1)
                .range(MIN_WORLD_DIMENSION_TILES..=MAX_WORLD_DIMENSION_TILES),
        );
    });
}

fn level_pair(ui: &mut egui::Ui, label: &str, cliff: &mut i16, water: &mut i16) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(TEXT_DIM));
        ui.add(
            egui::DragValue::new(cliff)
                .speed(1)
                .range(MIN_WORLD_LEVEL..=MAX_WORLD_LEVEL),
        );
        ui.add(
            egui::DragValue::new(water)
                .speed(1)
                .range(MIN_WORLD_LEVEL..=MAX_WORLD_LEVEL),
        );
    });
}

#[cfg(test)]
#[path = "main_editor_world_creation_tests.rs"]
mod tests;
