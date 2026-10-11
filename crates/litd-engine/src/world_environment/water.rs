//! #808 water (deep/shallow) + map boundary + environment skybox.
//!
//! Bathymetry-driven water: a `shallow_depth` band that fades to transparent at
//! the shore and a deeper `deep_depth` band with a distinct tint, matching the
//! standard RTS/RPG "the deeper you see, the foggier the water" approach. The map
//! boundary describes the authored extent, the camera-bounded playable inset, and
//! how the world edge terminates. A skybox preset names the environment dome.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Rgba8, WorldEnvironmentError, invalid, record::EnvironmentPayload, require_non_empty};

/// How the authored world edge terminates beyond the playable inset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryKind {
    /// A hard cliff/impassable wall at the map edge.
    HardEdge,
    /// Water extends to the edge (island / coastal maps).
    WaterExtent,
    /// The world fades to void/black (skybox-only) at the edge.
    Void,
}

impl BoundaryKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HardEdge => "hard_edge",
            Self::WaterExtent => "water_extent",
            Self::Void => "void",
        }
    }
}

/// The environment skybox preset naming the sky dome + horizon lighting mood.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyboxPreset {
    ClearDay,
    Overcast,
    Dusk,
    Night,
    Underdark,
}

impl SkyboxPreset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClearDay => "clear_day",
            Self::Overcast => "overcast",
            Self::Dusk => "dusk",
            Self::Night => "night",
            Self::Underdark => "underdark",
        }
    }

    /// A stable ordinal so the preset lowers as an integer field too.
    pub const fn ordinal(self) -> u32 {
        match self {
            Self::ClearDay => 0,
            Self::Overcast => 1,
            Self::Dusk => 2,
            Self::Night => 3,
            Self::Underdark => 4,
        }
    }
}

/// Authored map boundary: extent, camera-bounded playable inset, and edge kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapBoundary {
    pub width_tiles: u32,
    pub height_tiles: u32,
    /// Inset (in tiles) from every edge that the camera/units are bounded to.
    pub playable_inset_tiles: u32,
    pub kind: BoundaryKind,
}

impl MapBoundary {
    fn validate(&self) -> Result<(), WorldEnvironmentError> {
        if self.width_tiles == 0 || self.height_tiles == 0 {
            return invalid(
                "boundary.extent",
                "map extent must be non-zero on both axes",
            );
        }
        let min_dim = self.width_tiles.min(self.height_tiles);
        // Both edges are inset, so 2*inset must leave a non-empty playable core.
        if self.playable_inset_tiles.saturating_mul(2) >= min_dim {
            return invalid(
                "boundary.playable_inset_tiles",
                "playable inset consumes the entire map; leave a non-empty core",
            );
        }
        Ok(())
    }
}

/// Authored deep/shallow water body for the whole map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaterBody {
    /// Max depth (milli-metres) still counted as shallow (transparent-at-shore band).
    pub shallow_depth_mm: u32,
    /// Depth (milli-metres) at which water is fully deep-tinted (> shallow).
    pub deep_depth_mm: u32,
    /// Shore blend band width (milli-metres); <= shallow depth.
    pub shore_blend_mm: u32,
    pub shallow_color: Rgba8,
    pub deep_color: Rgba8,
    pub waves_cliff_shores: bool,
    pub waves_rolling_shores: bool,
}

impl WaterBody {
    fn validate(&self) -> Result<(), WorldEnvironmentError> {
        if self.shallow_depth_mm == 0 {
            return invalid("water.shallow_depth_mm", "shallow depth must be positive");
        }
        if self.deep_depth_mm <= self.shallow_depth_mm {
            return invalid(
                "water.deep_depth_mm",
                "deep depth must exceed shallow depth (deep water is strictly deeper)",
            );
        }
        if self.shore_blend_mm > self.shallow_depth_mm {
            return invalid(
                "water.shore_blend_mm",
                "shore blend band cannot exceed the shallow depth",
            );
        }
        if self.shallow_color.a >= self.deep_color.a {
            return invalid(
                "water.deep_color",
                "deep water must be less transparent (higher alpha) than shallow water",
            );
        }
        Ok(())
    }
}

/// The #808 authoring aggregate: water body + boundary + skybox for one map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaterEnvironmentAuthoring {
    pub id: String,
    pub display_name: String,
    pub author: String,
    pub water: WaterBody,
    pub boundary: MapBoundary,
    pub skybox: SkyboxPreset,
}

impl WaterEnvironmentAuthoring {
    pub fn validate(&self) -> Result<(), WorldEnvironmentError> {
        require_non_empty("water.id", &self.id)?;
        require_non_empty("water.display_name", &self.display_name)?;
        require_non_empty("water.author", &self.author)?;
        self.water.validate()?;
        self.boundary.validate()?;
        Ok(())
    }

    /// Deterministic integer-only payload for persist + lowering.
    pub fn to_payload(&self) -> Result<EnvironmentPayload, WorldEnvironmentError> {
        self.validate()?;
        let fields = json!({
            "boundary_edge_kind": self.boundary.kind.as_str(),
            "boundary_height_tiles": self.boundary.height_tiles,
            "boundary_playable_inset_tiles": self.boundary.playable_inset_tiles,
            "boundary_width_tiles": self.boundary.width_tiles,
            "skybox_ordinal": self.skybox.ordinal(),
            "skybox_preset": self.skybox.as_str(),
            "water_deep_color": self.water.deep_color.to_payload(),
            "water_deep_depth_mm": self.water.deep_depth_mm,
            "water_shallow_color": self.water.shallow_color.to_payload(),
            "water_shallow_depth_mm": self.water.shallow_depth_mm,
            "water_shore_blend_mm": self.water.shore_blend_mm,
            "water_waves_cliff_shores": self.water.waves_cliff_shores,
            "water_waves_rolling_shores": self.water.waves_rolling_shores,
        });
        EnvironmentPayload::new("water_boundary_environment", &self.id, fields)
    }
}
