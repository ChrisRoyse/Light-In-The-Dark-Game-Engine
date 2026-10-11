//! #846: runtime cameras (apply/pan/rotate/lock; top-down RTS default, optional
//! first-person).
//!
//! BOUNDARY: camera state is PRESENTATION. It is per-client and lives OUTSIDE the
//! deterministic sim (#1312/#85) -- nothing here is folded into a sim state hash,
//! and floats are allowed. Two clients watching one lockstep match may hold
//! entirely different camera state without diverging the simulation. Scripted
//! moves consume the #892 cinematic evaluator's resolved pose; named viewpoints
//! consume the #813 named-camera records (a minimal record surface is defined
//! here until #813's lands -- see PR note).

use serde::{Deserialize, Serialize};

use crate::cinematic_timeline::CinematicCameraPose;

/// A concrete camera pose. Angles in degrees; distance is orbit distance from
/// `target` (0 in first-person). All presentation floats.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraPose {
    pub target: [f32; 3],
    pub distance: f32,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub fov_degrees: f32,
}

impl CameraPose {
    /// The default top-down RTS pose: high pitch looking down at the origin.
    pub const fn top_down_default() -> Self {
        Self {
            target: [0.0, 0.0, 0.0],
            distance: 40.0,
            yaw_degrees: 0.0,
            pitch_degrees: 70.0,
            fov_degrees: 60.0,
        }
    }

    /// Adopt the resolved pose the #892 cinematic evaluator produced.
    pub fn from_cinematic(pose: &CinematicCameraPose) -> Self {
        Self {
            target: [pose.target.x, pose.target.y, pose.target.z],
            distance: pose.distance,
            yaw_degrees: pose.yaw_degrees,
            pitch_degrees: pose.pitch_degrees,
            fov_degrees: pose.fov_degrees,
        }
    }
}

/// Camera projection/behaviour mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CameraMode {
    /// Fixed-pitch orbit above the battlefield (RTS default).
    TopDown,
    /// Eye-level view (distance collapses to 0, low pitch).
    FirstPerson,
    /// #1861: frame a live unit; host rebinds `pose.target` from unit position
    /// each presentation frame while `follow_unit` is `Some`.
    Follow,
}

/// A named viewpoint record (#813 seam). The editor authors named cameras and
/// lowers them into records the runtime can `apply` by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedCameraRecord {
    pub name: String,
    pub pose: CameraPose,
    pub mode: CameraMode,
}

/// A runtime camera op applied per-client. `Pan`/`Rotate`/`Zoom` are ignored
/// while the camera is locked (e.g. during a scripted cinematic move).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CameraOp {
    /// Snap directly to a pose.
    Apply(CameraPose),
    /// Translate `target` by a world delta.
    Pan([f32; 3]),
    /// Add to yaw/pitch (degrees).
    Rotate {
        yaw_degrees: f32,
        pitch_degrees: f32,
    },
    /// Add to orbit distance (clamped at 0).
    Zoom(f32),
    /// Switch camera mode.
    SetMode(CameraMode),
    /// Lock (true) or unlock (false) manual control.
    Lock(bool),
    /// Snap to a named viewpoint record (#813).
    ApplyNamed(NamedCameraRecord),
    /// #1861: bind follow mode to a live unit id (scripted escort framing).
    /// Clears on `SetMode` of a non-Follow mode or `ClearFollow`.
    FollowUnit { unit: u32 },
    /// #1861: clear the follow binding and restore top-down RTS mode.
    ClearFollow,
}

/// The per-client camera. NOT sim state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraState {
    pub pose: CameraPose,
    pub mode: CameraMode,
    pub locked: bool,
    /// #1861: unit id the host should frame while mode is `Follow`.
    pub follow_unit: Option<u32>,
}

impl Default for CameraState {
    fn default() -> Self {
        Self::top_down_rts()
    }
}

impl CameraState {
    /// The top-down RTS default a fresh client starts with.
    pub fn top_down_rts() -> Self {
        Self {
            pose: CameraPose::top_down_default(),
            mode: CameraMode::TopDown,
            locked: false,
            follow_unit: None,
        }
    }

    /// Apply one op, honoring the lock. Returns whether the op changed anything
    /// (a locked manual op is a no-op, not an error -- input is simply inert).
    pub fn apply(&mut self, op: &CameraOp) -> bool {
        match op {
            CameraOp::Lock(value) => {
                let changed = self.locked != *value;
                self.locked = *value;
                changed
            }
            // Apply/SetMode/ApplyNamed always take effect (scripted authority).
            CameraOp::Apply(pose) => {
                self.pose = *pose;
                true
            }
            CameraOp::SetMode(mode) => self.set_mode(*mode),
            CameraOp::ApplyNamed(record) => {
                self.pose = record.pose;
                self.set_mode(record.mode);
                true
            }
            CameraOp::FollowUnit { unit } => {
                self.follow_unit = Some(*unit);
                self.mode = CameraMode::Follow;
                true
            }
            CameraOp::ClearFollow => {
                let changed = self.follow_unit.is_some() || self.mode == CameraMode::Follow;
                self.follow_unit = None;
                if self.mode == CameraMode::Follow {
                    self.mode = CameraMode::TopDown;
                }
                changed
            }
            // Manual ops are inert while locked.
            CameraOp::Pan(delta) => {
                if self.locked {
                    return false;
                }
                self.pose.target[0] += delta[0];
                self.pose.target[1] += delta[1];
                self.pose.target[2] += delta[2];
                true
            }
            CameraOp::Rotate {
                yaw_degrees,
                pitch_degrees,
            } => {
                if self.locked {
                    return false;
                }
                self.pose.yaw_degrees = wrap_degrees(self.pose.yaw_degrees + yaw_degrees);
                self.pose.pitch_degrees =
                    (self.pose.pitch_degrees + pitch_degrees).clamp(-89.0, 89.0);
                true
            }
            CameraOp::Zoom(delta) => {
                if self.locked {
                    return false;
                }
                self.pose.distance = (self.pose.distance + delta).max(0.0);
                true
            }
        }
    }

    /// Apply a sequence of ops; returns the number that changed state.
    pub fn apply_all(&mut self, ops: &[CameraOp]) -> usize {
        ops.iter().filter(|op| self.apply(op)).count()
    }

    /// Drive a scripted move: adopt a resolved cinematic pose. Bypasses the lock
    /// because the cinematic runtime is the authority during a scripted move.
    pub fn apply_cinematic_pose(&mut self, pose: &CinematicCameraPose) {
        self.pose = CameraPose::from_cinematic(pose);
    }

    fn set_mode(&mut self, mode: CameraMode) -> bool {
        if self.mode == mode {
            return false;
        }
        self.mode = mode;
        match mode {
            CameraMode::FirstPerson => {
                self.follow_unit = None;
                self.pose.distance = 0.0;
                self.pose.pitch_degrees = self.pose.pitch_degrees.clamp(-89.0, 20.0);
            }
            CameraMode::TopDown => {
                self.follow_unit = None;
                if self.pose.distance <= 0.0 {
                    self.pose.distance = CameraPose::top_down_default().distance;
                }
                self.pose.pitch_degrees = CameraPose::top_down_default().pitch_degrees;
            }
            CameraMode::Follow => {
                // Binding is set by FollowUnit; SetMode(Follow) alone leaves the
                // existing follow target (or None until FollowUnit is applied).
            }
        }
        true
    }

    /// Host helper: rebind `pose.target` to a world-space unit position while
    /// follow mode is active. No-op when not following. Presentation-only.
    pub fn rebind_follow_target(&mut self, world_xyz: [f32; 3]) {
        if self.mode == CameraMode::Follow && self.follow_unit.is_some() {
            self.pose.target = world_xyz;
        }
    }
}

/// Wrap an angle into `[0, 360)`.
fn wrap_degrees(value: f32) -> f32 {
    let wrapped = value % 360.0;
    if wrapped < 0.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}
