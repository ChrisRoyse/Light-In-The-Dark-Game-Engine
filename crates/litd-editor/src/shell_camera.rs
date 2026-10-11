use litd_engine::{
    EngineError,
    math::{Aabb, Vec2, Vec3},
    render::editor_world::{DEFAULT_CAMERA_PROFILE_ID, default_editor_world_visual_preset},
};
use serde::{Deserialize, Serialize};

use super::{invalid_shell, shell_state::WorldViewCameraState};

pub const ED11_BIRD_EYE_CAMERA_PROFILE_ID: &str = DEFAULT_CAMERA_PROFILE_ID;
pub const WORLD_VIEW_CAMERA_BOOKMARK_SLOT_COUNT: u8 = 4;

const MIN_DISTANCE: f32 = 28.0;
const MAX_DISTANCE: f32 = 320.0;
const MIN_PITCH_DEGREES: f32 = -72.0;
const MAX_PITCH_DEGREES: f32 = -34.0;
const ZOOM_UNITS_PER_SCROLL_PIXEL: f32 = 0.08;
const ORBIT_DEGREES_PER_PIXEL: f32 = 0.10;
const PITCH_DEGREES_PER_PIXEL: f32 = 0.08;
const PAN_WORLD_UNITS_PER_DISTANCE: f32 = 1.0 / 780.0;
const FRAME_SELECTION_MIN_RADIUS: f32 = 8.0;
const FRAME_SELECTION_DISTANCE_MULTIPLIER: f32 = 4.8;
const FRAME_SELECTION_MIN_DISTANCE: f32 = 42.0;
const FRAME_SELECTION_MAX_DISTANCE: f32 = 168.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldViewFrameSelection {
    pub selected_entities: u32,
    pub pivot_world: Vec3,
    pub radius: f32,
}

impl WorldViewFrameSelection {
    pub fn validate(self) -> Result<(), EngineError> {
        if !self.pivot_world.is_finite() || !self.radius.is_finite() || self.radius < 0.0 {
            return invalid_shell(
                "camera.frame_selection",
                "selection pivot and radius must be finite, and radius must be non-negative",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldViewCameraBookmark {
    pub slot: u8,
    pub label: String,
    pub camera: WorldViewCameraState,
}

impl WorldViewCameraBookmark {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_bookmark_slot(self.slot)?;
        if self.label.trim().is_empty() || self.label.chars().count() > 32 {
            return invalid_shell(
                "camera.bookmark.label",
                "bookmark label must be non-empty and at most 32 chars",
            );
        }
        self.camera.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldViewCameraControlInput {
    pub pan_delta_px: Vec2,
    pub orbit_delta_px: Vec2,
    pub zoom_delta_px: f32,
}

impl WorldViewCameraControlInput {
    pub const ZERO: Self = Self {
        pan_delta_px: Vec2::ZERO,
        orbit_delta_px: Vec2::ZERO,
        zoom_delta_px: 0.0,
    };

    pub fn validate(self) -> Result<(), EngineError> {
        if !self.pan_delta_px.is_finite()
            || !self.orbit_delta_px.is_finite()
            || !self.zoom_delta_px.is_finite()
        {
            return invalid_shell("camera.input", "pan, orbit, and zoom deltas must be finite");
        }
        Ok(())
    }

    pub fn has_motion(self) -> bool {
        self.pan_delta_px != Vec2::ZERO
            || self.orbit_delta_px != Vec2::ZERO
            || self.zoom_delta_px != 0.0
    }
}

pub fn ed11_bird_eye_default_camera() -> WorldViewCameraState {
    let preset = default_editor_world_visual_preset();
    WorldViewCameraState {
        target_world: preset.camera_target_world,
        distance: preset.camera_distance,
        yaw_degrees: preset.camera_yaw_degrees,
        pitch_degrees: preset.camera_pitch_degrees,
    }
}

pub fn previous_schema_eight_stock_camera() -> WorldViewCameraState {
    WorldViewCameraState {
        target_world: Vec3::new(0.0, 0.0, -22.0),
        distance: 165.0,
        yaw_degrees: 38.0,
        pitch_degrees: -44.0,
    }
}

pub fn apply_world_view_camera_control(
    camera: WorldViewCameraState,
    world_bounds: Aabb,
    input: WorldViewCameraControlInput,
) -> Result<WorldViewCameraState, EngineError> {
    input.validate()?;
    let mut next = clamp_world_view_camera_to_profile(camera, world_bounds)?;
    if input.pan_delta_px != Vec2::ZERO {
        let yaw = next.yaw_degrees.to_radians();
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let forward = Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let scale = (next.distance * PAN_WORLD_UNITS_PER_DISTANCE).max(0.01);
        next.target_world +=
            (right * -input.pan_delta_px.x + forward * input.pan_delta_px.y) * scale;
    }
    if input.orbit_delta_px != Vec2::ZERO {
        next.yaw_degrees =
            (next.yaw_degrees + input.orbit_delta_px.x * ORBIT_DEGREES_PER_PIXEL).rem_euclid(360.0);
        next.pitch_degrees -= input.orbit_delta_px.y * PITCH_DEGREES_PER_PIXEL;
    }
    if input.zoom_delta_px != 0.0 {
        next.distance -= input.zoom_delta_px * ZOOM_UNITS_PER_SCROLL_PIXEL;
    }
    clamp_world_view_camera_to_profile(next, world_bounds)
}

pub fn set_world_view_camera_target_xz(
    camera: WorldViewCameraState,
    world_bounds: Aabb,
    target_xz: [f32; 2],
) -> Result<WorldViewCameraState, EngineError> {
    if !target_xz[0].is_finite() || !target_xz[1].is_finite() {
        return invalid_shell("camera.target_xz", "target coordinates must be finite");
    }
    let mut next = camera;
    next.target_world.x = target_xz[0];
    next.target_world.z = target_xz[1];
    clamp_world_view_camera_to_profile(next, world_bounds)
}

pub fn frame_world_view_camera_on_selection(
    camera: WorldViewCameraState,
    world_bounds: Aabb,
    selection: WorldViewFrameSelection,
) -> Result<WorldViewCameraState, EngineError> {
    validate_world_bounds(world_bounds)?;
    validate_camera_scalars(camera)?;
    camera.validate()?;
    if selection.selected_entities == 0 {
        return Ok(camera);
    }
    selection.validate()?;
    if !world_bounds.contains_point(selection.pivot_world) {
        return invalid_shell(
            "camera.frame_selection.pivot",
            "selection pivot must be inside the current world bounds",
        );
    }
    let mut next = clamp_world_view_camera_to_profile(camera, world_bounds)?;
    next.target_world = selection.pivot_world;
    next.distance = (selection.radius.max(FRAME_SELECTION_MIN_RADIUS)
        * FRAME_SELECTION_DISTANCE_MULTIPLIER)
        .clamp(FRAME_SELECTION_MIN_DISTANCE, FRAME_SELECTION_MAX_DISTANCE);
    clamp_world_view_camera_to_profile(next, world_bounds)
}

pub fn save_world_view_camera_bookmark(
    bookmarks: &mut Vec<WorldViewCameraBookmark>,
    slot: u8,
    camera: WorldViewCameraState,
) -> Result<(), EngineError> {
    validate_bookmark_slot(slot)?;
    camera.validate()?;
    let bookmark = WorldViewCameraBookmark {
        slot,
        label: format!("View {slot}"),
        camera,
    };
    bookmark.validate()?;
    if let Some(existing) = bookmarks.iter_mut().find(|bookmark| bookmark.slot == slot) {
        *existing = bookmark;
    } else {
        bookmarks.push(bookmark);
        bookmarks.sort_by_key(|bookmark| bookmark.slot);
    }
    validate_world_view_camera_bookmarks(bookmarks)
}

pub fn restore_world_view_camera_bookmark(
    bookmarks: &[WorldViewCameraBookmark],
    slot: u8,
) -> Result<WorldViewCameraState, EngineError> {
    validate_bookmark_slot(slot)?;
    validate_world_view_camera_bookmarks(bookmarks)?;
    bookmarks
        .iter()
        .find(|bookmark| bookmark.slot == slot)
        .map(|bookmark| bookmark.camera)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.shell.camera.bookmark.restore".to_string(),
            reason: format!("bookmark slot {slot} has no saved camera"),
        })
}

pub fn validate_world_view_camera_bookmarks(
    bookmarks: &[WorldViewCameraBookmark],
) -> Result<(), EngineError> {
    let mut seen = [false; WORLD_VIEW_CAMERA_BOOKMARK_SLOT_COUNT as usize];
    for bookmark in bookmarks {
        bookmark.validate()?;
        let index = usize::from(bookmark.slot - 1);
        if seen[index] {
            return invalid_shell(
                "camera.bookmark.slot",
                format!("bookmark slot {} appears more than once", bookmark.slot),
            );
        }
        seen[index] = true;
    }
    Ok(())
}

pub fn world_view_camera_bookmark_summary(bookmarks: &[WorldViewCameraBookmark]) -> String {
    if bookmarks.is_empty() {
        return "none".to_string();
    }
    bookmarks
        .iter()
        .map(|bookmark| {
            format!(
                "{}:{}:{:.1},{:.1},{:.1}/{:.1}/{:.1}/{:.1}",
                bookmark.slot,
                bookmark.label,
                bookmark.camera.target_world.x,
                bookmark.camera.target_world.y,
                bookmark.camera.target_world.z,
                bookmark.camera.distance,
                bookmark.camera.yaw_degrees,
                bookmark.camera.pitch_degrees
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub fn clamp_world_view_camera_to_profile(
    camera: WorldViewCameraState,
    world_bounds: Aabb,
) -> Result<WorldViewCameraState, EngineError> {
    validate_world_bounds(world_bounds)?;
    validate_camera_scalars(camera)?;
    let mut next = camera;
    next.distance = next.distance.clamp(MIN_DISTANCE, MAX_DISTANCE);
    next.pitch_degrees = next
        .pitch_degrees
        .clamp(MIN_PITCH_DEGREES, MAX_PITCH_DEGREES);
    next.target_world = next.target_world.clamp(world_bounds.min, world_bounds.max);
    next.validate()?;
    Ok(next)
}

fn validate_camera_scalars(camera: WorldViewCameraState) -> Result<(), EngineError> {
    if !camera.target_world.is_finite()
        || !camera.distance.is_finite()
        || !camera.yaw_degrees.is_finite()
        || !camera.pitch_degrees.is_finite()
    {
        return invalid_shell(
            "camera",
            "camera target, distance, yaw, and pitch must be finite before profile clamping",
        );
    }
    Ok(())
}

fn validate_world_bounds(bounds: Aabb) -> Result<(), EngineError> {
    if !bounds.min.is_finite()
        || !bounds.max.is_finite()
        || bounds.min.x >= bounds.max.x
        || bounds.min.y >= bounds.max.y
        || bounds.min.z >= bounds.max.z
    {
        return invalid_shell(
            "camera.world_bounds",
            "world bounds must be finite with ordered extents",
        );
    }
    Ok(())
}

fn validate_bookmark_slot(slot: u8) -> Result<(), EngineError> {
    if !(1..=WORLD_VIEW_CAMERA_BOOKMARK_SLOT_COUNT).contains(&slot) {
        return invalid_shell(
            "camera.bookmark.slot",
            format!("slot must be in 1..={WORLD_VIEW_CAMERA_BOOKMARK_SLOT_COUNT}"),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Aabb {
        Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(128.0)).unwrap()
    }

    #[test]
    fn invalid_camera_control_input_fails_loud_before_mutation() {
        let before = ed11_bird_eye_default_camera();
        let input = WorldViewCameraControlInput {
            pan_delta_px: Vec2::new(f32::NAN, 0.0),
            orbit_delta_px: Vec2::ZERO,
            zoom_delta_px: 0.0,
        };

        match apply_world_view_camera_control(before, bounds(), input) {
            Err(EngineError::UiRenderInvalidConfig { field, reason }) => {
                assert_eq!(field, "editor.shell.camera.input");
                assert!(reason.contains("finite"));
            }
            other => panic!("expected camera input failure, got {other:?}"),
        }
        assert_eq!(before, ed11_bird_eye_default_camera());
        println!(
            "edge=invalid_camera_control before={:?} after={:?}",
            before,
            ed11_bird_eye_default_camera()
        );
    }

    #[test]
    fn zero_camera_control_is_explicit_no_motion() {
        let before = ed11_bird_eye_default_camera();
        let after =
            apply_world_view_camera_control(before, bounds(), WorldViewCameraControlInput::ZERO)
                .unwrap();

        assert!(!WorldViewCameraControlInput::ZERO.has_motion());
        assert_eq!(after, before);
        println!("edge=zero_camera_control before={before:?} after={after:?}");
    }

    #[test]
    fn frame_selection_centers_pivot_and_clamps_distance() {
        let before = ed11_bird_eye_default_camera();
        let selection = WorldViewFrameSelection {
            selected_entities: 1,
            pivot_world: Vec3::new(40.0, 0.0, -30.0),
            radius: 10.0,
        };
        let after = frame_world_view_camera_on_selection(before, bounds(), selection).unwrap();

        assert_eq!(after.target_world, selection.pivot_world);
        assert_eq!(after.distance, 48.0);
        println!("frame_selection before={before:?} after={after:?}");
    }

    #[test]
    fn bookmark_restore_missing_slot_fails_loud() {
        let bookmarks = Vec::new();
        let error = restore_world_view_camera_bookmark(&bookmarks, 1)
            .expect_err("missing bookmark must fail loud")
            .to_string();

        assert!(error.contains("editor.shell.camera.bookmark.restore"));
        println!("edge=missing_bookmark_restore error={error}");
    }
}
