use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::{Aabb, Fixed, FixedVec3, Transform, Vec3},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_MATH_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1081-math-fsv.txt"));

    let transform = Transform::from_translation(Vec3::new(3.0, 4.0, 5.0));
    let transformed_point = transform.transform_point3(Vec3::new(1.0, 2.0, 3.0));
    let bounds = Aabb::from_center_half_extents(transformed_point, Vec3::new(1.0, 2.0, 3.0))?;
    let union = bounds.union(Aabb::new(
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(0.0, 0.0, 0.0),
    )?);

    let fixed_product = Fixed::from_ratio(3, 2)?.checked_mul(Fixed::from_i32(2))?;
    let fixed_dot = FixedVec3::new(
        Fixed::from_ratio(3, 2)?,
        Fixed::from_i32(-2),
        Fixed::from_i32(4),
    )
    .dot(FixedVec3::new(
        Fixed::from_i32(2),
        Fixed::from_i32(3),
        Fixed::from_ratio(1, 2)?,
    ))?;
    let divide_by_zero_code = match Fixed::from_ratio(1, 0) {
        Err(err @ EngineError::FixedPointDivideByZero) => err.code(),
        other => panic!("expected FixedPointDivideByZero, got {other:?}"),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "transform_point={:.3},{:.3},{:.3}\n\
             aabb_min={:.3},{:.3},{:.3}\n\
             aabb_max={:.3},{:.3},{:.3}\n\
             aabb_contains_point={}\n\
             union_min={:.3},{:.3},{:.3}\n\
             union_max={:.3},{:.3},{:.3}\n\
             fixed_product_raw={}\n\
             fixed_product_value={:.6}\n\
             fixed_dot_raw={}\n\
             fixed_dot_value={:.6}\n\
             fixed_divide_by_zero_code={}\n",
            transformed_point.x,
            transformed_point.y,
            transformed_point.z,
            bounds.min.x,
            bounds.min.y,
            bounds.min.z,
            bounds.max.x,
            bounds.max.y,
            bounds.max.z,
            bounds.contains_point(transformed_point),
            union.min.x,
            union.min.y,
            union.min.z,
            union.max.x,
            union.max.y,
            union.max.z,
            fixed_product.raw(),
            fixed_product.to_f64(),
            fixed_dot.raw(),
            fixed_dot.to_f64(),
            divide_by_zero_code,
        ),
    )?;
    println!("wrote math FSV stats to {}", path.display());
    Ok(())
}
