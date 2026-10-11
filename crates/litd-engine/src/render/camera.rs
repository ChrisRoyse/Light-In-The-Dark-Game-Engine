//! Top-down/isometric RTS camera controls, matrices, and frustum culling.

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Aabb, Mat4, Vec2, Vec3, Vec4},
    render::batching::{Frustum as BatchFrustum, FrustumPlane as BatchFrustumPlane},
};

#[path = "camera_cinematic.rs"]
mod cinematic;

pub use cinematic::{CinematicCameraPath, CinematicKeyframe};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RtsCameraConfig {
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub fov_y_rad: f32,
    pub near: f32,
    pub far: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub min_pitch_rad: f32,
    pub max_pitch_rad: f32,
    pub pan_speed: f32,
    pub zoom_speed: f32,
    pub rotate_speed_rad: f32,
    pub world_bounds: Aabb,
}

impl RtsCameraConfig {
    pub fn standard(viewport_width: u32, viewport_height: u32, world_bounds: Aabb) -> Self {
        Self {
            viewport_width,
            viewport_height,
            fov_y_rad: 55.0_f32.to_radians(),
            near: 0.1,
            far: 512.0,
            min_distance: 8.0,
            max_distance: 96.0,
            min_pitch_rad: 45.0_f32.to_radians(),
            max_pitch_rad: 80.0_f32.to_radians(),
            pan_speed: 18.0,
            zoom_speed: 24.0,
            rotate_speed_rad: 90.0_f32.to_radians(),
            world_bounds,
        }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.viewport_width == 0 || self.viewport_height == 0 {
            return invalid_camera("viewport", "width and height must be non-zero");
        }
        if !self.fov_y_rad.is_finite() || !(0.1..std::f32::consts::PI).contains(&self.fov_y_rad) {
            return invalid_camera("fov_y_rad", "must be finite and inside 0.1..PI");
        }
        if !self.near.is_finite()
            || !self.far.is_finite()
            || self.near <= 0.0
            || self.near >= self.far
        {
            return invalid_camera(
                "near_far",
                "near must be finite, positive, and less than far",
            );
        }
        if !self.min_distance.is_finite()
            || !self.max_distance.is_finite()
            || self.min_distance <= 0.0
            || self.min_distance > self.max_distance
        {
            return invalid_camera("distance", "min/max distance must be finite and ordered");
        }
        if !self.min_pitch_rad.is_finite()
            || !self.max_pitch_rad.is_finite()
            || self.min_pitch_rad <= 0.0
            || self.min_pitch_rad > self.max_pitch_rad
            || self.max_pitch_rad >= std::f32::consts::FRAC_PI_2
        {
            return invalid_camera("pitch", "pitch bounds must be finite and below 90 degrees");
        }
        if !self.pan_speed.is_finite()
            || !self.zoom_speed.is_finite()
            || !self.rotate_speed_rad.is_finite()
            || self.pan_speed < 0.0
            || self.zoom_speed < 0.0
            || self.rotate_speed_rad < 0.0
        {
            return invalid_camera(
                "controller_speeds",
                "speeds must be finite and non-negative",
            );
        }
        validate_aabb("world_bounds", self.world_bounds)
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.viewport_width as f32 / self.viewport_height as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RtsCameraState {
    pub target: Vec3,
    pub yaw_rad: f32,
    pub pitch_rad: f32,
    pub distance: f32,
}

impl RtsCameraState {
    pub fn isometric(target: Vec3) -> Self {
        Self {
            target,
            yaw_rad: 45.0_f32.to_radians(),
            pitch_rad: 60.0_f32.to_radians(),
            distance: 32.0,
        }
    }

    pub fn validate(&self, config: &RtsCameraConfig) -> Result<(), EngineError> {
        if !self.target.is_finite()
            || !self.yaw_rad.is_finite()
            || !self.pitch_rad.is_finite()
            || !self.distance.is_finite()
        {
            return invalid_camera("state", "target, yaw, pitch, and distance must be finite");
        }
        if self.pitch_rad < config.min_pitch_rad || self.pitch_rad > config.max_pitch_rad {
            return invalid_camera("state.pitch_rad", "pitch is outside configured range");
        }
        if self.distance < config.min_distance || self.distance > config.max_distance {
            return invalid_camera("state.distance", "distance is outside configured range");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CameraControlLocks {
    pub pan: bool,
    pub zoom: bool,
    pub rotate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraControlInput {
    pub pan_axis: Vec2,
    pub zoom_axis: f32,
    pub rotate_axis: f32,
    pub pitch_axis: f32,
}

impl CameraControlInput {
    pub const ZERO: Self = Self {
        pan_axis: Vec2::ZERO,
        zoom_axis: 0.0,
        rotate_axis: 0.0,
        pitch_axis: 0.0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EdgeScrollInput {
    pub cursor_px: Vec2,
    pub margin_px: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RtsCamera {
    pub config: RtsCameraConfig,
    pub state: RtsCameraState,
    pub locks: CameraControlLocks,
}

impl RtsCamera {
    pub fn new(config: RtsCameraConfig, state: RtsCameraState) -> Result<Self, EngineError> {
        config.validate()?;
        let mut camera = Self {
            config,
            state,
            locks: CameraControlLocks::default(),
        };
        camera.clamp_state();
        camera.state.validate(&camera.config)?;
        Ok(camera)
    }

    pub fn apply_input(
        &mut self,
        input: CameraControlInput,
        dt_seconds: f32,
    ) -> Result<(), EngineError> {
        if !input.pan_axis.is_finite()
            || !input.zoom_axis.is_finite()
            || !input.rotate_axis.is_finite()
            || !input.pitch_axis.is_finite()
            || !dt_seconds.is_finite()
            || dt_seconds < 0.0
        {
            return invalid_camera("controller_input", "input axes and dt must be finite");
        }
        if !self.locks.pan {
            let right = Vec3::new(self.state.yaw_rad.cos(), 0.0, -self.state.yaw_rad.sin());
            let forward = Vec3::new(self.state.yaw_rad.sin(), 0.0, self.state.yaw_rad.cos());
            self.state.target += (right * input.pan_axis.x + forward * input.pan_axis.y)
                * self.config.pan_speed
                * dt_seconds;
        }
        if !self.locks.zoom {
            self.state.distance -= input.zoom_axis * self.config.zoom_speed * dt_seconds;
        }
        if !self.locks.rotate {
            self.state.yaw_rad += input.rotate_axis * self.config.rotate_speed_rad * dt_seconds;
            self.state.pitch_rad += input.pitch_axis * self.config.rotate_speed_rad * dt_seconds;
        }
        self.clamp_state();
        self.state.validate(&self.config)
    }

    pub fn edge_scroll_axis(&self, input: EdgeScrollInput) -> Result<Vec2, EngineError> {
        if !input.cursor_px.is_finite() || !input.margin_px.is_finite() || input.margin_px <= 0.0 {
            return invalid_camera(
                "edge_scroll",
                "cursor and margin must be finite and margin must be positive",
            );
        }
        let viewport = Vec2::new(
            self.config.viewport_width as f32,
            self.config.viewport_height as f32,
        );
        if input.margin_px * 2.0 >= viewport.x.min(viewport.y) {
            return invalid_camera(
                "edge_scroll.margin_px",
                "margin must be less than half the smaller viewport dimension",
            );
        }
        if input.cursor_px.cmplt(Vec2::ZERO).any() || input.cursor_px.cmpgt(viewport).any() {
            return invalid_camera(
                "edge_scroll.cursor_px",
                "cursor must be inside the viewport",
            );
        }
        let left = edge_amount(input.margin_px - input.cursor_px.x, input.margin_px);
        let right = edge_amount(
            input.cursor_px.x - (viewport.x - input.margin_px),
            input.margin_px,
        );
        let top = edge_amount(input.margin_px - input.cursor_px.y, input.margin_px);
        let bottom = edge_amount(
            input.cursor_px.y - (viewport.y - input.margin_px),
            input.margin_px,
        );
        Ok(Vec2::new(right - left, top - bottom))
    }

    pub fn apply_edge_scroll(
        &mut self,
        input: EdgeScrollInput,
        dt_seconds: f32,
    ) -> Result<(), EngineError> {
        let pan_axis = self.edge_scroll_axis(input)?;
        self.apply_input(
            CameraControlInput {
                pan_axis,
                ..CameraControlInput::ZERO
            },
            dt_seconds,
        )
    }

    pub fn eye(&self) -> Vec3 {
        self.state.target - self.forward() * self.state.distance
    }

    pub fn forward(&self) -> Vec3 {
        let cos_pitch = self.state.pitch_rad.cos();
        Vec3::new(
            self.state.yaw_rad.sin() * cos_pitch,
            -self.state.pitch_rad.sin(),
            self.state.yaw_rad.cos() * cos_pitch,
        )
        .normalize()
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.state.target, Vec3::Y)
    }

    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(
            self.config.fov_y_rad,
            self.config.aspect_ratio(),
            self.config.near,
            self.config.far,
        )
    }

    pub fn view_projection_matrix(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    pub fn frustum(&self) -> Result<CameraFrustum, EngineError> {
        CameraFrustum::from_view_projection(self.view_projection_matrix())
    }

    fn clamp_state(&mut self) {
        self.state.distance = self
            .state
            .distance
            .clamp(self.config.min_distance, self.config.max_distance);
        self.state.pitch_rad = self
            .state
            .pitch_rad
            .clamp(self.config.min_pitch_rad, self.config.max_pitch_rad);
        self.state.target = self
            .state
            .target
            .clamp(self.config.world_bounds.min, self.config.world_bounds.max);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrustumPlane {
    pub normal: Vec3,
    pub distance: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraFrustum {
    pub planes: [FrustumPlane; 6],
}

impl CameraFrustum {
    pub fn from_view_projection(view_projection: Mat4) -> Result<Self, EngineError> {
        let rows = matrix_rows(view_projection);
        let planes = [
            plane_from_vec4(rows[3] + rows[0])?,
            plane_from_vec4(rows[3] - rows[0])?,
            plane_from_vec4(rows[3] + rows[1])?,
            plane_from_vec4(rows[3] - rows[1])?,
            plane_from_vec4(rows[3] + rows[2])?,
            plane_from_vec4(rows[3] - rows[2])?,
        ];
        Ok(Self { planes })
    }

    pub fn contains_aabb(&self, bounds: Aabb) -> bool {
        self.planes.iter().all(|plane| {
            let positive = Vec3::new(
                if plane.normal.x >= 0.0 {
                    bounds.max.x
                } else {
                    bounds.min.x
                },
                if plane.normal.y >= 0.0 {
                    bounds.max.y
                } else {
                    bounds.min.y
                },
                if plane.normal.z >= 0.0 {
                    bounds.max.z
                } else {
                    bounds.min.z
                },
            );
            plane.normal.dot(positive) + plane.distance >= 0.0
        })
    }

    pub fn visible_indices(&self, bounds: &[Aabb]) -> Vec<usize> {
        bounds
            .iter()
            .enumerate()
            .filter_map(|(index, bounds)| self.contains_aabb(*bounds).then_some(index))
            .collect()
    }

    pub fn to_batching_frustum(&self) -> BatchFrustum {
        BatchFrustum::new(
            self.planes
                .iter()
                .map(|plane| BatchFrustumPlane::new(plane.normal, plane.distance))
                .collect(),
        )
    }
}

fn matrix_rows(matrix: Mat4) -> [Vec4; 4] {
    let x = matrix.x_axis;
    let y = matrix.y_axis;
    let z = matrix.z_axis;
    let w = matrix.w_axis;
    [
        Vec4::new(x.x, y.x, z.x, w.x),
        Vec4::new(x.y, y.y, z.y, w.y),
        Vec4::new(x.z, y.z, z.z, w.z),
        Vec4::new(x.w, y.w, z.w, w.w),
    ]
}

fn plane_from_vec4(value: Vec4) -> Result<FrustumPlane, EngineError> {
    let normal = value.truncate();
    let length = normal.length();
    if !length.is_finite() || length <= f32::EPSILON || !value.w.is_finite() {
        return invalid_camera("frustum.plane", "plane must be finite and non-zero");
    }
    Ok(FrustumPlane {
        normal: normal / length,
        distance: value.w / length,
    })
}

fn validate_aabb(field: &'static str, bounds: Aabb) -> Result<(), EngineError> {
    if !bounds.min.is_finite()
        || !bounds.max.is_finite()
        || bounds.min.x > bounds.max.x
        || bounds.min.y > bounds.max.y
        || bounds.min.z > bounds.max.z
    {
        return invalid_camera(field, "bounds must be finite with min <= max");
    }
    Ok(())
}

fn edge_amount(distance_inside_margin: f32, margin: f32) -> f32 {
    (distance_inside_margin / margin).clamp(0.0, 1.0)
}

fn invalid_camera<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::Rendering(
        litd_engine::error::RenderingError::CameraInvalidConfig {
            field: field.into(),
            reason: reason.into(),
        },
    ))
}

#[cfg(test)]
#[path = "camera_tests.rs"]
mod tests;
