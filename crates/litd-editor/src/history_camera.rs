//! #813: constructors + validation for `EditorNamedCamera` (child module of `history`).
//!
//! Named cameras reuse the play-in-editor rig/pose model (`PlayCameraConfig`) and add an
//! identity (`id`/`name`), a `kind` (top-down RTS default vs authored cinematic), and a
//! `look_at` world target. The target is validated against the map's playable bounds so an
//! off-map cinematic shot fails loud rather than framing empty space -- a camera parked
//! exactly on the last playable edge tile is accepted (inclusive edge).

use litd_engine::{EngineError, math::Vec3};

use super::{EditorCameraKind, EditorNamedCamera, invalid_history};
use crate::play::{PlayCameraConfig, PlayCameraRig};
use crate::world_creation::EditorWorldProperties;

impl EditorNamedCamera {
    /// Top-down RTS camera, the default rig, framing `look_at`.
    pub fn top_down(id: impl Into<String>, name: impl Into<String>, look_at: Vec3) -> Self {
        Self::new(
            id,
            name,
            EditorCameraKind::TopDown,
            PlayCameraConfig::preset(PlayCameraRig::TopDown),
            look_at,
        )
    }

    /// Authored cinematic camera with an explicit rig config.
    pub fn cinematic(
        id: impl Into<String>,
        name: impl Into<String>,
        config: PlayCameraConfig,
        look_at: Vec3,
    ) -> Self {
        Self::new(id, name, EditorCameraKind::Cinematic, config, look_at)
    }

    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        kind: EditorCameraKind,
        config: PlayCameraConfig,
        look_at: Vec3,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            config,
            look_at,
            properties: std::collections::BTreeMap::new(),
        }
    }

    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    pub(super) fn validate(&self) -> Result<(), EngineError> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return invalid_history("named_camera", "id and name must be non-empty");
        }
        if !self.look_at.is_finite() {
            return invalid_history("named_camera.look_at", "look_at must be finite");
        }
        self.config.validate()
    }
}

/// #813 edge case: the camera's `look_at` tile must fall inside the world's playable bounds.
/// The map edge is inclusive (the last playable tile passes); one tile past it fails loud.
pub(super) fn validate_named_camera_bounds(
    properties: &EditorWorldProperties,
    camera: &EditorNamedCamera,
) -> Result<(), EngineError> {
    let bounds = &properties.playable_bounds;
    let tile_x = camera.look_at.x.floor() as i32;
    let tile_z = camera.look_at.z.floor() as i32;
    if tile_x < bounds.min_tile_x
        || tile_x >= bounds.max_tile_x_exclusive
        || tile_z < bounds.min_tile_z
        || tile_z >= bounds.max_tile_z_exclusive
    {
        return invalid_history(
            "named_camera.bounds",
            format!(
                "camera {} look_at world ({:.3},{:.3}) maps to tile ({tile_x},{tile_z}) outside playable bounds [{}..{}, {}..{}]",
                camera.id,
                camera.look_at.x,
                camera.look_at.z,
                bounds.min_tile_x,
                bounds.max_tile_x_exclusive,
                bounds.min_tile_z,
                bounds.max_tile_z_exclusive,
            ),
        );
    }
    Ok(())
}
