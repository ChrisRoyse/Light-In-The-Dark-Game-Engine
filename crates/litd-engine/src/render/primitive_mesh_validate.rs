use crate::EngineError;

use super::super::{PrimitiveKind, invalid_primitive};

pub(crate) fn validate_kind(kind: &PrimitiveKind) -> Result<(), EngineError> {
    match kind {
        PrimitiveKind::Box3d { size } => {
            for axis in size {
                positive("box.size", *axis)?;
            }
        }
        PrimitiveKind::Sphere {
            radius,
            segments,
            rings,
        } => {
            positive("sphere.radius", *radius)?;
            range_u32("sphere.segments", *segments, 3, 128)?;
            range_u32("sphere.rings", *rings, 2, 64)?;
        }
        PrimitiveKind::Cylinder {
            radius,
            height,
            radial_segments,
            height_segments,
        } => {
            positive("cylinder.radius", *radius)?;
            positive("cylinder.height", *height)?;
            range_u32("cylinder.radial_segments", *radial_segments, 3, 128)?;
            range_u32("cylinder.height_segments", *height_segments, 1, 64)?;
        }
        PrimitiveKind::Torus {
            major_radius,
            minor_radius,
            major_segments,
            minor_segments,
        } => {
            positive("torus.major_radius", *major_radius)?;
            positive("torus.minor_radius", *minor_radius)?;
            if minor_radius >= major_radius {
                return invalid_primitive(
                    "torus.radius",
                    "minor_radius must be smaller than major_radius",
                );
            }
            range_u32("torus.major_segments", *major_segments, 3, 128)?;
            range_u32("torus.minor_segments", *minor_segments, 3, 64)?;
        }
        PrimitiveKind::Tube {
            outer_radius,
            inner_radius,
            height,
            radial_segments,
        } => {
            positive("tube.outer_radius", *outer_radius)?;
            positive("tube.inner_radius", *inner_radius)?;
            positive("tube.height", *height)?;
            if inner_radius >= outer_radius {
                return invalid_primitive(
                    "tube.radius",
                    "inner_radius must be smaller than outer_radius",
                );
            }
            range_u32("tube.radial_segments", *radial_segments, 3, 128)?;
        }
        PrimitiveKind::Plane {
            width,
            depth,
            subdivisions_x,
            subdivisions_z,
        } => {
            positive("plane.width", *width)?;
            positive("plane.depth", *depth)?;
            range_u32("plane.subdivisions_x", *subdivisions_x, 1, 64)?;
            range_u32("plane.subdivisions_z", *subdivisions_z, 1, 64)?;
        }
        PrimitiveKind::Sprite { width, height } => {
            positive("sprite.width", *width)?;
            positive("sprite.height", *height)?;
        }
        PrimitiveKind::Points { points } => {
            if points.is_empty() || points.len() > 4096 {
                return invalid_primitive("points", "point count must be inside 1..=4096");
            }
            validate_points("points", points)?;
        }
        PrimitiveKind::Lines { points } => {
            if points.len() < 2 || points.len() > 4096 || points.len() % 2 != 0 {
                return invalid_primitive(
                    "lines",
                    "line-list point count must be even inside 2..=4096",
                );
            }
            validate_points("lines", points)?;
        }
    }
    Ok(())
}

fn positive(field: &str, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() || value <= 0.0 || value > 10_000.0 {
        return invalid_primitive(field, "value must be finite and inside (0, 10000]");
    }
    Ok(())
}

fn range_u32(field: &str, value: u32, min: u32, max: u32) -> Result<(), EngineError> {
    if !(min..=max).contains(&value) {
        return invalid_primitive(field, format!("value must be inside {min}..={max}"));
    }
    Ok(())
}

fn validate_points(field: &str, points: &[[f32; 3]]) -> Result<(), EngineError> {
    for point in points {
        if !point.iter().all(|value| value.is_finite()) {
            return invalid_primitive(field, "all points must be finite");
        }
    }
    Ok(())
}
