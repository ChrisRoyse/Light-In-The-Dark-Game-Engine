//! Undo/redo, autosave, selection, clipboard, and diff-stable editor documents.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};

use crate::asset_products::{AssetProduct, validate_model_assignment};
use crate::cinematic_authoring::EditorCinematicTimeline;
pub use crate::clipboard::{
    ClipboardPasteOptions, ClipboardPasteReport, ClipboardPayload, ClipboardPlacement,
    ClipboardTransform, copy_selection, paste_clipboard, paste_clipboard_with_options,
};
use crate::editor_state_store::EditorStateStore;
use crate::play::PlayCameraConfig;
use crate::terrain_bulk_ops::{EditorTerrainCell, validate_terrain_cells};
use crate::terrain_pathing::{
    EditorTerrainPathingOverride, EditorTilePathingRule, validate_terrain_pathing,
};
use crate::terrain_variation::{
    TerrainRandomGroup, TerrainTileVariation, validate_random_groups, validate_tile_variations,
};
use crate::world_creation::EditorWorldProperties;

// #1466: per-entity constructors + validation live in `history_entities` (impl blocks on the
// entity structs below; it sees their fields as a child module and calls `invalid_history`).
#[path = "history_entities.rs"]
mod history_entities;
// #813: named-camera constructors + validation live in `history_camera` (child module; sees
// `EditorNamedCamera` fields directly and calls `invalid_history`).
#[path = "history_camera.rs"]
mod history_camera;
// #892: cinematic-timeline document methods (upsert + camera-ref validation) live in
// `history_cinematic` (child module; sees `EditorDocument` fields and `invalid_history`).
#[path = "history_cinematic.rs"]
mod history_cinematic;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorDocument {
    pub calyx_document_id: String,
    pub revision_id: String,
    #[serde(default)]
    pub world_properties: EditorWorldProperties,
    #[serde(default)]
    pub play_camera_config: PlayCameraConfig,
    #[serde(default)]
    pub asset_products: BTreeMap<String, AssetProduct>,
    pub objects: BTreeMap<String, EditorObject>,
    #[serde(default)]
    pub regions: BTreeMap<String, EditorRegion>,
    #[serde(default)]
    pub named_cameras: BTreeMap<String, EditorNamedCamera>,
    /// #892: authored cinematic/cutscene timelines. Each references named cameras by
    /// id; lowering resolves those refs into a frozen runtime timeline.
    #[serde(default)]
    pub cinematic_timelines: BTreeMap<String, EditorCinematicTimeline>,
    #[serde(default)]
    pub terrain_stamps: BTreeMap<String, EditorTerrainStamp>,
    #[serde(default)]
    pub terrain_cells: BTreeMap<String, EditorTerrainCell>,
    #[serde(default)]
    pub terrain_pathing_overrides: BTreeMap<String, EditorTerrainPathingOverride>,
    #[serde(default)]
    pub terrain_tile_pathing_rules: BTreeMap<String, EditorTilePathingRule>,
    #[serde(default)]
    pub terrain_tile_variations: BTreeMap<String, TerrainTileVariation>,
    #[serde(default)]
    pub random_groups: BTreeMap<String, TerrainRandomGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorObject {
    pub id: String,
    pub kind: String,
    pub parent: Option<String>,
    pub transform: Transform,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorRegion {
    pub id: String,
    pub name: String,
    pub center: litd_engine::math::Vec3,
    pub half_extents: litd_engine::math::Vec3,
    pub properties: BTreeMap<String, String>,
}

/// #813: how a named camera frames the world. `TopDown` is the RTS default; `Cinematic`
/// is an authored storytelling shot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EditorCameraKind {
    #[default]
    TopDown,
    Cinematic,
}

/// #813: a placed, named camera. `config` reuses the play-in-editor rig/pose model;
/// `look_at` is the world point the camera frames and is validated against the map's
/// playable bounds (an off-map cinematic target fails loud).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorNamedCamera {
    pub id: String,
    pub name: String,
    pub kind: EditorCameraKind,
    pub config: PlayCameraConfig,
    pub look_at: Vec3,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorTerrainStamp {
    pub id: String,
    pub kind: String,
    pub transform: Transform,
    pub width_tiles: u32,
    pub height_tiles: u32,
    pub height_delta: f32,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionSet {
    pub selected_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub label: String,
    pub calyx_revision_id: String,
    pub before: EditorDocument,
    pub after: EditorDocument,
    #[serde(default)]
    pub before_snapshot_seq: Option<u64>,
    #[serde(default)]
    pub after_snapshot_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UndoRedoStack {
    pub undo: Vec<HistoryEntry>,
    pub redo: Vec<HistoryEntry>,
    pub max_entries: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutosaveRecord {
    pub calyx_document_id: String,
    pub calyx_revision_id: String,
    pub stable_fingerprint: String,
    pub stable_ron: String,
    pub pending_changes: bool,
}

impl EditorDocument {
    pub fn new(
        calyx_document_id: impl Into<String>,
        revision_id: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let document = Self {
            calyx_document_id: calyx_document_id.into(),
            revision_id: revision_id.into(),
            world_properties: EditorWorldProperties::default(),
            play_camera_config: PlayCameraConfig::default(),
            asset_products: BTreeMap::new(),
            objects: BTreeMap::new(),
            regions: BTreeMap::new(),
            named_cameras: BTreeMap::new(),
            cinematic_timelines: BTreeMap::new(),
            terrain_stamps: BTreeMap::new(),
            terrain_cells: BTreeMap::new(),
            terrain_pathing_overrides: BTreeMap::new(),
            terrain_tile_pathing_rules: BTreeMap::new(),
            terrain_tile_variations: BTreeMap::new(),
            random_groups: BTreeMap::new(),
        };
        document.validate()?;
        Ok(document)
    }

    pub fn upsert_object(&mut self, object: EditorObject) -> Result<(), EngineError> {
        object.validate()?;
        self.objects.insert(object.id.clone(), object);
        Ok(())
    }

    pub fn remove_object(&mut self, id: &str) -> Result<EditorObject, EngineError> {
        self.objects.remove(id).ok_or_else(|| {
            invalid_history_error("document.objects", format!("missing object {id}"))
        })
    }

    pub fn upsert_region(&mut self, region: EditorRegion) -> Result<(), EngineError> {
        region.validate()?;
        self.regions.insert(region.id.clone(), region);
        Ok(())
    }

    pub fn upsert_named_camera(&mut self, camera: EditorNamedCamera) -> Result<(), EngineError> {
        camera.validate()?;
        history_camera::validate_named_camera_bounds(&self.world_properties, &camera)?;
        self.named_cameras.insert(camera.id.clone(), camera);
        Ok(())
    }

    pub fn upsert_terrain_stamp(&mut self, stamp: EditorTerrainStamp) -> Result<(), EngineError> {
        stamp.validate()?;
        self.terrain_stamps.insert(stamp.id.clone(), stamp);
        Ok(())
    }

    pub fn upsert_terrain_cell(&mut self, cell: EditorTerrainCell) -> Result<(), EngineError> {
        cell.validate()?;
        self.terrain_cells.insert(cell.id.clone(), cell);
        Ok(())
    }

    pub fn upsert_terrain_tile_variation(
        &mut self,
        tile: TerrainTileVariation,
    ) -> Result<(), EngineError> {
        tile.validate()?;
        self.terrain_tile_variations.insert(tile.id.clone(), tile);
        Ok(())
    }

    pub fn upsert_random_group(&mut self, group: TerrainRandomGroup) -> Result<(), EngineError> {
        group.validate()?;
        self.random_groups.insert(group.id.clone(), group);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.calyx_document_id.trim().is_empty() || self.revision_id.trim().is_empty() {
            return invalid_history(
                "document.calyx",
                "document and revision ids must be non-empty",
            );
        }
        self.world_properties.validate()?;
        self.play_camera_config.validate()?;
        for (product_id, product) in &self.asset_products {
            if product_id != &product.product_id {
                return invalid_history(
                    "document.asset_products",
                    format!("map key {product_id} does not match product_id"),
                );
            }
            product.validate()?;
        }
        for object in self.objects.values() {
            object.validate()?;
            validate_model_assignment(&object.id, &object.properties, &self.asset_products)?;
            if let Some(parent) = &object.parent
                && !self.objects.contains_key(parent)
            {
                return invalid_history(
                    "document.parent",
                    format!("object {} references missing parent {parent}", object.id),
                );
            }
        }
        for (region_id, region) in &self.regions {
            if region_id != &region.id {
                return invalid_history(
                    "document.regions",
                    format!("map key {region_id} does not match region id {}", region.id),
                );
            }
            region.validate()?;
        }
        for (camera_id, camera) in &self.named_cameras {
            if camera_id != &camera.id {
                return invalid_history(
                    "document.named_cameras",
                    format!("map key {camera_id} does not match camera id {}", camera.id),
                );
            }
            camera.validate()?;
            history_camera::validate_named_camera_bounds(&self.world_properties, camera)?;
        }
        history_cinematic::validate_document_timelines(self)?;
        for stamp in self.terrain_stamps.values() {
            stamp.validate()?;
        }
        validate_terrain_cells(&self.terrain_cells, &self.terrain_tile_variations)?;
        validate_terrain_pathing(self)?;
        validate_tile_variations(&self.terrain_tile_variations)?;
        validate_random_groups(&self.random_groups)?;
        Ok(())
    }

    pub fn set_play_camera_config(&mut self, config: PlayCameraConfig) -> Result<(), EngineError> {
        config.validate()?;
        self.play_camera_config = config;
        self.validate()
    }
}

impl UndoRedoStack {
    pub fn new(max_entries: usize) -> Result<Self, EngineError> {
        if max_entries == 0 {
            return invalid_history("history.max_entries", "must be greater than zero");
        }
        Ok(Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_entries,
        })
    }

    pub fn record(
        &mut self,
        label: impl Into<String>,
        before: EditorDocument,
        after: EditorDocument,
    ) -> Result<(), EngineError> {
        before.validate()?;
        after.validate()?;
        let entry = HistoryEntry {
            label: label.into(),
            calyx_revision_id: after.revision_id.clone(),
            before,
            after,
            before_snapshot_seq: None,
            after_snapshot_seq: None,
        };
        self.undo.push(entry);
        self.redo.clear();
        if self.undo.len() > self.max_entries {
            self.undo.remove(0);
        }
        Ok(())
    }

    pub fn record_with_mvcc(
        &mut self,
        store: &EditorStateStore,
        label: impl Into<String>,
        before: EditorDocument,
        after: EditorDocument,
    ) -> Result<(), EngineError> {
        before.validate()?;
        after.validate()?;
        if before.calyx_document_id != after.calyx_document_id {
            return invalid_history("mvcc.document", "before and after document ids must match");
        }
        let before_seq = write_history_snapshot(store, &before)?;
        let after_seq = write_history_snapshot(store, &after)?;
        let entry = HistoryEntry {
            label: label.into(),
            calyx_revision_id: after.revision_id.clone(),
            before,
            after,
            before_snapshot_seq: Some(before_seq),
            after_snapshot_seq: Some(after_seq),
        };
        self.undo.push(entry);
        self.redo.clear();
        if self.undo.len() > self.max_entries {
            self.undo.remove(0);
        }
        Ok(())
    }

    pub fn undo(&mut self, document: &mut EditorDocument) -> Result<Option<String>, EngineError> {
        let Some(entry) = self.undo.pop() else {
            return Ok(None);
        };
        *document = entry.before.clone();
        self.redo.push(entry.clone());
        Ok(Some(entry.label))
    }

    pub fn redo(&mut self, document: &mut EditorDocument) -> Result<Option<String>, EngineError> {
        let Some(entry) = self.redo.pop() else {
            return Ok(None);
        };
        *document = entry.after.clone();
        self.undo.push(entry.clone());
        Ok(Some(entry.label))
    }

    pub fn undo_from_mvcc(
        &mut self,
        store: &EditorStateStore,
        document: &mut EditorDocument,
    ) -> Result<Option<String>, EngineError> {
        let Some(entry) = self.undo.pop() else {
            return Ok(None);
        };
        let seq = entry
            .before_snapshot_seq
            .ok_or_else(|| invalid_history_error("mvcc", "undo entry missing before seq"))?;
        *document = read_history_snapshot(store, &entry.before.calyx_document_id, seq)?;
        self.redo.push(entry.clone());
        Ok(Some(entry.label))
    }

    pub fn redo_from_mvcc(
        &mut self,
        store: &EditorStateStore,
        document: &mut EditorDocument,
    ) -> Result<Option<String>, EngineError> {
        let Some(entry) = self.redo.pop() else {
            return Ok(None);
        };
        let seq = entry
            .after_snapshot_seq
            .ok_or_else(|| invalid_history_error("mvcc", "redo entry missing after seq"))?;
        *document = read_history_snapshot(store, &entry.after.calyx_document_id, seq)?;
        self.undo.push(entry.clone());
        Ok(Some(entry.label))
    }
}

fn write_history_snapshot(
    store: &EditorStateStore,
    document: &EditorDocument,
) -> Result<u64, EngineError> {
    let bytes =
        serde_json::to_vec_pretty(document).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize history snapshot: {error}"),
        })?;
    Ok(store
        .put_history_bytes(&document.calyx_document_id, &bytes)?
        .seq)
}

fn read_history_snapshot(
    store: &EditorStateStore,
    document_id: &str,
    seq: u64,
) -> Result<EditorDocument, EngineError> {
    let bytes = store
        .read_history_bytes_at(document_id, seq)?
        .ok_or_else(|| invalid_history_error("mvcc", "history snapshot missing"))?;
    let document = serde_json::from_slice::<EditorDocument>(&bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse history snapshot: {error}"),
        }
    })?;
    document.validate()?;
    Ok(document)
}

pub fn autosave_record(
    document: &EditorDocument,
    pending_changes: bool,
) -> Result<AutosaveRecord, EngineError> {
    let stable_ron = to_stable_ron(document)?;
    Ok(AutosaveRecord {
        calyx_document_id: document.calyx_document_id.clone(),
        calyx_revision_id: document.revision_id.clone(),
        stable_fingerprint: litd_hash::fnv1a64_hex(stable_ron.as_bytes()),
        stable_ron,
        pending_changes,
    })
}

pub fn to_stable_ron<T: Serialize>(value: &T) -> Result<String, EngineError> {
    litd_hash::to_stable_ron(value).map_err(|error| EngineError::SerializationFailed {
        format: "ron",
        reason: error.to_string(),
    })
}

pub fn stable_fingerprint<T: Serialize>(value: &T) -> Result<String, EngineError> {
    litd_hash::stable_fingerprint(value).map_err(|error| EngineError::SerializationFailed {
        format: "ron",
        reason: error.to_string(),
    })
}

pub(super) fn invalid_history<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_history_error(field, reason))
}

fn invalid_history_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.history.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
