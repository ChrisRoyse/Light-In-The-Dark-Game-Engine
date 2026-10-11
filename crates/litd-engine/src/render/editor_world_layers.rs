use std::ops::Range;

use crate::EngineError;

use super::world_error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorWorldSceneLayerKind {
    Terrain,
    Foliage,
    Doodads,
    Units,
    Water,
    Blight,
    Shadows,
    Lighting,
    Weather,
    FogEffects,
    CameraBounds,
    PathingGround,
    PathingNaval,
    Regions,
    CameraObjects,
    InfoDisplay,
}

impl EditorWorldSceneLayerKind {
    fn label(self) -> &'static str {
        match self {
            Self::Terrain => "terrain",
            Self::Foliage => "foliage",
            Self::Doodads => "doodads",
            Self::Units => "units",
            Self::Water => "water",
            Self::Blight => "blight",
            Self::Shadows => "shadows",
            Self::Lighting => "lighting",
            Self::Weather => "weather",
            Self::FogEffects => "fog_effects",
            Self::CameraBounds => "camera_bounds",
            Self::PathingGround => "pathing_ground",
            Self::PathingNaval => "pathing_naval",
            Self::Regions => "regions",
            Self::CameraObjects => "camera_objects",
            Self::InfoDisplay => "info_display",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorWorldSceneLayer {
    pub kind: EditorWorldSceneLayerKind,
    pub start_index: u32,
    pub index_count: u32,
}

impl EditorWorldSceneLayer {
    pub fn new(kind: EditorWorldSceneLayerKind, start_index: u32, index_count: u32) -> Self {
        Self {
            kind,
            start_index,
            index_count,
        }
    }

    pub fn end_index(self) -> u32 {
        self.start_index.saturating_add(self.index_count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorWorldLayerVisibility {
    pub terrain: bool,
    pub foliage: bool,
    pub doodads: bool,
    pub units: bool,
    pub water: bool,
    pub blight: bool,
    pub shadows: bool,
    pub lighting: bool,
    pub weather: bool,
    pub sky: bool,
    pub fog_effects: bool,
    pub grid: bool,
    pub camera_bounds: bool,
    pub pathing_ground: bool,
    pub pathing_naval: bool,
    pub regions: bool,
    pub camera_objects: bool,
    pub info_display: bool,
    pub letterbox_mode: bool,
    pub lock_to_game_camera: bool,
}

impl Default for EditorWorldLayerVisibility {
    fn default() -> Self {
        Self {
            terrain: true,
            foliage: true,
            doodads: true,
            units: true,
            water: true,
            blight: true,
            shadows: true,
            lighting: true,
            weather: true,
            sky: true,
            fog_effects: true,
            grid: false,
            camera_bounds: true,
            pathing_ground: true,
            pathing_naval: true,
            regions: true,
            camera_objects: true,
            info_display: true,
            letterbox_mode: false,
            lock_to_game_camera: false,
        }
    }
}

impl EditorWorldLayerVisibility {
    pub fn summary(self) -> String {
        format!(
            "terrain={};foliage={};doodads={};units={};water={};blight={};shadows={};lighting={};weather={};sky={};fog_effects={};grid={};camera_bounds={};pathing_ground={};pathing_naval={};regions={};camera_objects={};info_display={};letterbox_mode={};lock_to_game_camera={}",
            self.terrain,
            self.foliage,
            self.doodads,
            self.units,
            self.water,
            self.blight,
            self.shadows,
            self.lighting,
            self.weather,
            self.sky,
            self.fog_effects,
            self.grid,
            self.camera_bounds,
            self.pathing_ground,
            self.pathing_naval,
            self.regions,
            self.camera_objects,
            self.info_display,
            self.letterbox_mode,
            self.lock_to_game_camera
        )
    }

    pub fn is_visible(self, kind: EditorWorldSceneLayerKind) -> bool {
        match kind {
            EditorWorldSceneLayerKind::Terrain => self.terrain,
            EditorWorldSceneLayerKind::Foliage => self.foliage,
            EditorWorldSceneLayerKind::Doodads => self.doodads,
            EditorWorldSceneLayerKind::Units => self.units,
            EditorWorldSceneLayerKind::Water => self.water,
            EditorWorldSceneLayerKind::Blight => self.blight,
            EditorWorldSceneLayerKind::Shadows => self.shadows,
            EditorWorldSceneLayerKind::Lighting => self.lighting,
            EditorWorldSceneLayerKind::Weather => self.weather,
            EditorWorldSceneLayerKind::FogEffects => self.fog_effects,
            EditorWorldSceneLayerKind::CameraBounds => self.camera_bounds,
            EditorWorldSceneLayerKind::PathingGround => self.pathing_ground,
            EditorWorldSceneLayerKind::PathingNaval => self.pathing_naval,
            EditorWorldSceneLayerKind::Regions => self.regions,
            EditorWorldSceneLayerKind::CameraObjects => self.camera_objects,
            EditorWorldSceneLayerKind::InfoDisplay => self.info_display,
        }
    }
}

pub(super) fn validate_scene_layers(
    layers: &[EditorWorldSceneLayer],
    total_index_count: u32,
) -> Result<(), EngineError> {
    let mut previous_end = 0_u32;
    for layer in layers {
        if layer.index_count == 0 {
            return world_error(
                "reference_scene.layers",
                "layer index_count must be non-zero",
            );
        }
        if layer.start_index % 3 != 0 || layer.index_count % 3 != 0 {
            return world_error(
                "reference_scene.layers",
                format!(
                    "{} layer range must align to triangle indices",
                    layer.kind.label()
                ),
            );
        }
        let end = layer
            .start_index
            .checked_add(layer.index_count)
            .ok_or_else(|| EngineError::EditorWorldRendererInvalidConfig {
                field: "reference_scene.layers".to_string(),
                reason: format!("{} layer range overflowed u32", layer.kind.label()),
            })?;
        if end > total_index_count {
            return world_error(
                "reference_scene.layers",
                format!(
                    "{} layer range {}..{} exceeds scene index_count {}",
                    layer.kind.label(),
                    layer.start_index,
                    end,
                    total_index_count
                ),
            );
        }
        if layer.start_index < previous_end {
            return world_error(
                "reference_scene.layers",
                format!(
                    "{} layer overlaps a previous scene layer",
                    layer.kind.label()
                ),
            );
        }
        previous_end = end;
    }
    Ok(())
}

pub(super) fn visible_index_ranges(
    total_index_count: u32,
    layers: &[EditorWorldSceneLayer],
    visibility: EditorWorldLayerVisibility,
) -> Vec<Range<u32>> {
    if total_index_count == 0 {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut cursor = 0_u32;
    for layer in layers {
        if visibility.is_visible(layer.kind) {
            continue;
        }
        if cursor < layer.start_index {
            ranges.push(cursor..layer.start_index);
        }
        cursor = cursor.max(layer.end_index());
    }
    if cursor < total_index_count {
        ranges.push(cursor..total_index_count);
    }
    ranges
}

pub(super) fn hidden_index_count(
    layers: &[EditorWorldSceneLayer],
    visibility: EditorWorldLayerVisibility,
) -> u32 {
    layers
        .iter()
        .filter(|layer| !visibility.is_visible(layer.kind))
        .map(|layer| layer.index_count)
        .sum()
}

pub(super) fn hidden_layer_count(
    layers: &[EditorWorldSceneLayer],
    visibility: EditorWorldLayerVisibility,
) -> u32 {
    layers
        .iter()
        .filter(|layer| !visibility.is_visible(layer.kind))
        .count() as u32
}
