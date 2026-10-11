use litd_engine::{EngineError, render::editor_world::EditorWorldLayerVisibility};
use serde::{Deserialize, Serialize};

use super::EditorShellRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldLayerVisibility {
    #[serde(default = "visible_by_default")]
    pub terrain: bool,
    #[serde(default = "visible_by_default")]
    pub foliage: bool,
    #[serde(default = "visible_by_default")]
    pub doodads: bool,
    #[serde(default = "visible_by_default")]
    pub units: bool,
    #[serde(default = "visible_by_default")]
    pub water: bool,
    #[serde(default = "visible_by_default")]
    pub blight: bool,
    #[serde(default = "visible_by_default")]
    pub shadows: bool,
    #[serde(default = "visible_by_default")]
    pub lighting: bool,
    #[serde(default = "visible_by_default")]
    pub weather: bool,
    #[serde(default = "visible_by_default")]
    pub sky: bool,
    #[serde(default = "visible_by_default")]
    pub fog_effects: bool,
    #[serde(default = "grid_visible_by_default")]
    pub grid: bool,
    #[serde(default = "visible_by_default")]
    pub camera_bounds: bool,
    #[serde(default = "visible_by_default")]
    pub pathing_ground: bool,
    #[serde(default = "visible_by_default")]
    pub pathing_naval: bool,
    #[serde(default = "visible_by_default")]
    pub regions: bool,
    #[serde(default = "visible_by_default")]
    pub camera_objects: bool,
    #[serde(default = "visible_by_default")]
    pub info_display: bool,
    #[serde(default)]
    pub letterbox_mode: bool,
    #[serde(default)]
    pub lock_to_game_camera: bool,
}

impl Default for WorldLayerVisibility {
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

impl WorldLayerVisibility {
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

    pub fn to_engine(self) -> EditorWorldLayerVisibility {
        EditorWorldLayerVisibility {
            terrain: self.terrain,
            foliage: self.foliage,
            doodads: self.doodads,
            units: self.units,
            water: self.water,
            blight: self.blight,
            shadows: self.shadows,
            lighting: self.lighting,
            weather: self.weather,
            sky: self.sky,
            fog_effects: self.fog_effects,
            grid: self.grid,
            camera_bounds: self.camera_bounds,
            pathing_ground: self.pathing_ground,
            pathing_naval: self.pathing_naval,
            regions: self.regions,
            camera_objects: self.camera_objects,
            info_display: self.info_display,
            letterbox_mode: self.letterbox_mode,
            lock_to_game_camera: self.lock_to_game_camera,
        }
    }

    pub(crate) fn is_enabled(self, toggle: WorldLayerToggle) -> bool {
        match toggle {
            WorldLayerToggle::Terrain => self.terrain,
            WorldLayerToggle::Foliage => self.foliage,
            WorldLayerToggle::Doodads => self.doodads,
            WorldLayerToggle::Units => self.units,
            WorldLayerToggle::Water => self.water,
            WorldLayerToggle::Blight => self.blight,
            WorldLayerToggle::Shadows => self.shadows,
            WorldLayerToggle::Lighting => self.lighting,
            WorldLayerToggle::Weather => self.weather,
            WorldLayerToggle::Sky => self.sky,
            WorldLayerToggle::FogEffects => self.fog_effects,
            WorldLayerToggle::Grid => self.grid,
            WorldLayerToggle::CameraBounds => self.camera_bounds,
            WorldLayerToggle::PathingGround => self.pathing_ground,
            WorldLayerToggle::PathingNaval => self.pathing_naval,
            WorldLayerToggle::Regions => self.regions,
            WorldLayerToggle::CameraObjects => self.camera_objects,
            WorldLayerToggle::InfoDisplay => self.info_display,
            WorldLayerToggle::LetterboxMode => self.letterbox_mode,
            WorldLayerToggle::LockToGameCamera => self.lock_to_game_camera,
        }
    }

    fn set_enabled(&mut self, toggle: WorldLayerToggle, enabled: bool) {
        match toggle {
            WorldLayerToggle::Terrain => self.terrain = enabled,
            WorldLayerToggle::Foliage => self.foliage = enabled,
            WorldLayerToggle::Doodads => self.doodads = enabled,
            WorldLayerToggle::Units => self.units = enabled,
            WorldLayerToggle::Water => self.water = enabled,
            WorldLayerToggle::Blight => self.blight = enabled,
            WorldLayerToggle::Shadows => self.shadows = enabled,
            WorldLayerToggle::Lighting => self.lighting = enabled,
            WorldLayerToggle::Weather => self.weather = enabled,
            WorldLayerToggle::Sky => self.sky = enabled,
            WorldLayerToggle::FogEffects => self.fog_effects = enabled,
            WorldLayerToggle::Grid => self.grid = enabled,
            WorldLayerToggle::CameraBounds => self.camera_bounds = enabled,
            WorldLayerToggle::PathingGround => self.pathing_ground = enabled,
            WorldLayerToggle::PathingNaval => self.pathing_naval = enabled,
            WorldLayerToggle::Regions => self.regions = enabled,
            WorldLayerToggle::CameraObjects => self.camera_objects = enabled,
            WorldLayerToggle::InfoDisplay => self.info_display = enabled,
            WorldLayerToggle::LetterboxMode => self.letterbox_mode = enabled,
            WorldLayerToggle::LockToGameCamera => self.lock_to_game_camera = enabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorldLayerToggle {
    Terrain,
    Foliage,
    Doodads,
    Units,
    Water,
    Blight,
    Shadows,
    Lighting,
    Weather,
    Sky,
    FogEffects,
    Grid,
    CameraBounds,
    PathingGround,
    PathingNaval,
    Regions,
    CameraObjects,
    InfoDisplay,
    LetterboxMode,
    LockToGameCamera,
}

impl WorldLayerToggle {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Terrain => "Terrain",
            Self::Foliage => "Foliage",
            Self::Doodads => "Doodads",
            Self::Units => "Units",
            Self::Water => "Water",
            Self::Blight => "Blight",
            Self::Shadows => "Shadows",
            Self::Lighting => "Lighting",
            Self::Weather => "Weather",
            Self::Sky => "Sky",
            Self::FogEffects => "Fog Effects",
            Self::Grid => "Grid",
            Self::CameraBounds => "Camera Bounds",
            Self::PathingGround => "Pathing (Ground)",
            Self::PathingNaval => "Pathing (Naval)",
            Self::Regions => "Regions",
            Self::CameraObjects => "Camera Objects",
            Self::InfoDisplay => "Info Display",
            Self::LetterboxMode => "Letterbox Mode",
            Self::LockToGameCamera => "Lock to Game Camera",
        }
    }
}

impl EditorShellRuntime {
    pub fn world_layer_visibility(&self) -> EditorWorldLayerVisibility {
        self.world_layers.to_engine()
    }

    pub(crate) fn set_world_layer_visible(
        &mut self,
        toggle: WorldLayerToggle,
        visible: bool,
    ) -> Result<(), EngineError> {
        if self.world_layers.is_enabled(toggle) == visible {
            return Ok(());
        }
        if toggle == WorldLayerToggle::LockToGameCamera && visible {
            self.snap_to_game_camera()?;
        }
        // Save-then-commit (#1489): persist the target layers first and only flip the
        // in-memory copy once the durable write succeeds, so a failed save leaves the
        // UI checkbox and disk agreeing on the old value instead of silently diverging.
        let mut next_layers = self.world_layers;
        next_layers.set_enabled(toggle, visible);
        self.shell_state
            .save_plan_with_world_layers(&self.plan, next_layers)?;
        self.world_layers = next_layers;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_world_foliage_visible(&mut self, visible: bool) -> Result<(), EngineError> {
        self.set_world_layer_visible(WorldLayerToggle::Foliage, visible)
    }
}

fn visible_by_default() -> bool {
    true
}

fn grid_visible_by_default() -> bool {
    false
}
