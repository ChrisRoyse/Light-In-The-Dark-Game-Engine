//! Authored world-environment surfaces (issues #808 #814 #815 #816).
//!
//! Four editor-authored, RTS-oriented environment concerns share one persist +
//! lowering spine: each authoring value is validated with typed errors, encoded
//! into a deterministic INTEGER-only `EngineContentRecord` payload (the lowering
//! valve forbids floats/nulls, `calyx_lowering.rs`), persisted through the real
//! `EngineCalyxContentStore::put_content` path (so #1791 `pending_measurement`
//! applies with no ORT present), and lowered into a frozen fingerprinted sim
//! package that the renderer consumes via `world_environment.wgsl`.
//!
//! - #808 water (deep/shallow) + map boundary + environment skybox -> [`water`]
//! - #814 lighting + day/night cycle (probe-GI baked states, #1742)  -> [`lighting`]
//! - #815 weather effects + global fog / visibility distance         -> [`weather`]
//! - #816 minimap generation + loading screen + map preview          -> [`minimap`]
//!
//! Doctrine: no side stores (#1348) -- the Aster vault + lowered artifact are the
//! only persistence; authoring never spills a JSON/RON file. Colours are 0..=255
//! channels; lengths are fixed-point milli-units (`MILLI`) so every field lowers
//! as an integer with no precision loss.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource};

pub mod lighting;
pub mod minimap;
pub mod record;
pub mod uniform;
pub mod water;
pub mod weather;

pub use lighting::{
    BakedGiStateRef, DayNightCyclePolicy, LightingEnvironmentAuthoring, TimeOfDayKeyframe,
};
pub use minimap::{LoadingScreen, MinimapAuthoring, MinimapSource, PreviewImageRef};
pub use uniform::{EnvironmentUniform, WORLD_ENVIRONMENT_WGSL, environment_uniform_from_package};
pub use water::{BoundaryKind, MapBoundary, SkyboxPreset, WaterEnvironmentAuthoring};
pub use weather::{GlobalFog, PrecipitationKind, WeatherEnvironmentAuthoring};

/// Fixed-point scale for authored lengths/densities: value * 1000 == milli-units.
/// Keeps every lowered field an integer (the valve forbids floats).
pub const MILLI: u32 = 1000;

/// The authoring surface schema tag stamped into every environment payload.
pub const WORLD_ENVIRONMENT_SCHEMA: &str = "litd.engine.world_environment.v1";

/// A packed 8-bit-per-channel colour. Explicit values (encode, do not embed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Encode as an integer JSON array `[r,g,b,a]` (lowerable, order-stable).
    pub fn to_payload(self) -> Value {
        json!([self.r, self.g, self.b, self.a])
    }

    /// Packed `0xRRGGBBAA` -- the exact 32-bit word the GPU uniform receives.
    pub fn packed_rgba(self) -> u32 {
        (u32::from(self.r) << 24)
            | (u32::from(self.g) << 16)
            | (u32::from(self.b) << 8)
            | u32::from(self.a)
    }
}

/// A packed 8-bit RGB colour (alpha implied 255).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_payload(self) -> Value {
        json!([self.r, self.g, self.b])
    }

    pub fn packed_rgb(self) -> u32 {
        (u32::from(self.r) << 16) | (u32::from(self.g) << 8) | u32::from(self.b)
    }
}

/// Typed, fail-loud errors for every authoring surface. `code()` returns a stable
/// diagnostic code (`LITD_ENV_*`) for logs and the MCP surface.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorldEnvironmentError {
    #[error("world-environment field {field} is invalid: {reason} [{code}]", code = self.code())]
    Invalid { field: String, reason: String },
    #[error("world-environment codec failed for {field}: {reason} [LITD_ENV_CODEC]")]
    Codec { field: String, reason: String },
}

impl WorldEnvironmentError {
    /// Stable diagnostic code, derived from the offending field's top segment.
    pub fn code(&self) -> String {
        match self {
            Self::Codec { .. } => "LITD_ENV_CODEC".to_string(),
            Self::Invalid { field, .. } => {
                let segment = field.split('.').next().unwrap_or("field");
                format!("LITD_ENV_{}", segment.to_ascii_uppercase())
            }
        }
    }
}

pub(crate) fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, WorldEnvironmentError> {
    Err(WorldEnvironmentError::Invalid {
        field: field.into(),
        reason: reason.into(),
    })
}

pub(crate) fn require_non_empty(field: &str, value: &str) -> Result<(), WorldEnvironmentError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.chars().any(char::is_control) {
        return invalid(field, "must not contain control characters");
    }
    Ok(())
}

/// Shared record builder: every authoring surface lowers to one Terrain-kind
/// content record with an editor:// provenance and a deterministic source hash
/// over its own canonical payload bytes.
pub(crate) fn build_content_record(
    id: &str,
    display_name: &str,
    author: &str,
    editor_path: &str,
    payload: Value,
) -> Result<EngineContentRecord, WorldEnvironmentError> {
    let canonical = serde_json::to_vec(&payload).map_err(|error| WorldEnvironmentError::Codec {
        field: "payload".to_string(),
        reason: error.to_string(),
    })?;
    Ok(EngineContentRecord {
        kind: EngineContentKind::Terrain,
        id: id.to_string(),
        display_name: display_name.to_string(),
        payload,
        source: EngineContentSource {
            path: editor_path.to_string(),
            author: author.to_string(),
            source_hash: litd_hash::sha256_hex(&canonical),
        },
        anchors: Vec::new(),
    })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
