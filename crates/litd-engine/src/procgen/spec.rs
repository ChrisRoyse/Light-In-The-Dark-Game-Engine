//! Parametric primitive specifications and their fail-loud validation.
//!
//! Every generator input is validated here before any geometry is emitted, so
//! zero-size primitives, non-finite parameters, and out-of-range subdivision
//! counts are rejected with a structured, actionable error instead of producing
//! a degenerate mesh.

use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::mesh::invalid;

/// Upper bound on any single subdivision axis. Keeps a hostile request from
/// allocating gigabytes: the densest primitive (sphere) is O(segments·rings).
pub const MAX_SUBDIVISIONS: u32 = 512;
/// Smallest accepted extent/radius. Anything at or below this is "zero size".
pub const MIN_EXTENT: f32 = 1.0e-4;

/// A parametric primitive. Winding is counter-clockwise front faces; all
/// generators emit outward normals and UVs in `[0, 1]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrimitiveSpec {
    #[serde(rename = "box")]
    Box { size: [f32; 3] },
    Plane {
        width: f32,
        depth: f32,
        subdivisions_x: u32,
        subdivisions_z: u32,
    },
    Cylinder {
        radius: f32,
        height: f32,
        radial_segments: u32,
        height_segments: u32,
    },
    Cone {
        radius: f32,
        height: f32,
        radial_segments: u32,
    },
    Sphere {
        radius: f32,
        segments: u32,
        rings: u32,
    },
    Torus {
        major_radius: f32,
        minor_radius: f32,
        major_segments: u32,
        minor_segments: u32,
    },
}

impl PrimitiveSpec {
    pub fn kind_id(&self) -> &'static str {
        match self {
            Self::Box { .. } => "box",
            Self::Plane { .. } => "plane",
            Self::Cylinder { .. } => "cylinder",
            Self::Cone { .. } => "cone",
            Self::Sphere { .. } => "sphere",
            Self::Torus { .. } => "torus",
        }
    }

    /// Reject degenerate parameters up front. Returns `Ok(())` only when the
    /// spec will produce a well-formed, non-degenerate mesh.
    pub fn validate(&self) -> Result<(), EngineError> {
        match self {
            Self::Box { size } => {
                extent("box.size.x", size[0])?;
                extent("box.size.y", size[1])?;
                extent("box.size.z", size[2])?;
            }
            Self::Plane {
                width,
                depth,
                subdivisions_x,
                subdivisions_z,
            } => {
                extent("plane.width", *width)?;
                extent("plane.depth", *depth)?;
                subdivisions("plane.subdivisions_x", *subdivisions_x, 1)?;
                subdivisions("plane.subdivisions_z", *subdivisions_z, 1)?;
            }
            Self::Cylinder {
                radius,
                height,
                radial_segments,
                height_segments,
            } => {
                extent("cylinder.radius", *radius)?;
                extent("cylinder.height", *height)?;
                subdivisions("cylinder.radial_segments", *radial_segments, 3)?;
                subdivisions("cylinder.height_segments", *height_segments, 1)?;
            }
            Self::Cone {
                radius,
                height,
                radial_segments,
            } => {
                extent("cone.radius", *radius)?;
                extent("cone.height", *height)?;
                subdivisions("cone.radial_segments", *radial_segments, 3)?;
            }
            Self::Sphere {
                radius,
                segments,
                rings,
            } => {
                extent("sphere.radius", *radius)?;
                subdivisions("sphere.segments", *segments, 3)?;
                subdivisions("sphere.rings", *rings, 2)?;
            }
            Self::Torus {
                major_radius,
                minor_radius,
                major_segments,
                minor_segments,
            } => {
                extent("torus.major_radius", *major_radius)?;
                extent("torus.minor_radius", *minor_radius)?;
                if *minor_radius >= *major_radius {
                    return invalid(
                        "torus.minor_radius",
                        "minor_radius must be strictly smaller than major_radius",
                    );
                }
                subdivisions("torus.major_segments", *major_segments, 3)?;
                subdivisions("torus.minor_segments", *minor_segments, 3)?;
            }
        }
        Ok(())
    }
}

fn extent(field: &str, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() {
        return invalid(field, "value must be finite");
    }
    if value <= MIN_EXTENT {
        return invalid(
            field,
            format!("value {value} is zero-size (must exceed {MIN_EXTENT})"),
        );
    }
    Ok(())
}

fn subdivisions(field: &str, value: u32, minimum: u32) -> Result<(), EngineError> {
    if value < minimum {
        return invalid(
            field,
            format!("subdivision count {value} is below the minimum of {minimum}"),
        );
    }
    if value > MAX_SUBDIVISIONS {
        return invalid(
            field,
            format!("subdivision count {value} exceeds the maximum of {MAX_SUBDIVISIONS}"),
        );
    }
    Ok(())
}
