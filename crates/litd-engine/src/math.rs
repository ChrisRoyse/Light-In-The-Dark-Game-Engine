//! Math foundation: glam-backed presentation math plus deterministic fixed-point sim math.

use crate::EngineError;
use serde::{Deserialize, Serialize};

pub type Vec2 = glam::Vec2;
pub type Vec3 = glam::Vec3;
pub type Vec4 = glam::Vec4;
pub type Quat = glam::Quat;
pub type Mat4 = glam::Mat4;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    pub fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    pub fn transform_point3(&self, point: Vec3) -> Vec3 {
        self.matrix().transform_point3(point)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Result<Self, EngineError> {
        if min.x > max.x || min.y > max.y || min.z > max.z {
            return Err(EngineError::InvalidAabb {
                min: min.to_array(),
                max: max.to_array(),
            });
        }
        Ok(Self { min, max })
    }

    pub fn from_center_half_extents(center: Vec3, half_extents: Vec3) -> Result<Self, EngineError> {
        Self::new(center - half_extents, center + half_extents)
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn half_extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    pub fn contains_point(&self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    pub fn union(&self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Fixed(i64);

impl Fixed {
    pub const FRACTIONAL_BITS: u32 = 32;
    pub const ONE_RAW: i64 = 1_i64 << Self::FRACTIONAL_BITS;
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(Self::ONE_RAW);

    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> i64 {
        self.0
    }

    pub const fn from_i32(value: i32) -> Self {
        Self((value as i64) << Self::FRACTIONAL_BITS)
    }

    pub fn from_ratio(numerator: i64, denominator: i64) -> Result<Self, EngineError> {
        if denominator == 0 {
            return Err(EngineError::FixedPointDivideByZero);
        }
        fixed_from_i128(
            ((numerator as i128) << Self::FRACTIONAL_BITS) / denominator as i128,
            "from_ratio",
        )
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64 / Self::ONE_RAW as f64
    }

    pub fn checked_add(self, rhs: Self) -> Result<Self, EngineError> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(EngineError::FixedPointOverflow { operation: "add" })
    }

    pub fn checked_sub(self, rhs: Self) -> Result<Self, EngineError> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or(EngineError::FixedPointOverflow { operation: "sub" })
    }

    pub fn checked_mul(self, rhs: Self) -> Result<Self, EngineError> {
        fixed_from_i128(
            ((self.0 as i128) * (rhs.0 as i128)) >> Self::FRACTIONAL_BITS,
            "mul",
        )
    }

    pub fn checked_div(self, rhs: Self) -> Result<Self, EngineError> {
        if rhs.0 == 0 {
            return Err(EngineError::FixedPointDivideByZero);
        }
        fixed_from_i128(
            ((self.0 as i128) << Self::FRACTIONAL_BITS) / rhs.0 as i128,
            "div",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixedVec3 {
    pub x: Fixed,
    pub y: Fixed,
    pub z: Fixed,
}

impl FixedVec3 {
    pub const ZERO: Self = Self {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        z: Fixed::ZERO,
    };

    pub const fn new(x: Fixed, y: Fixed, z: Fixed) -> Self {
        Self { x, y, z }
    }

    pub const fn from_i32s(x: i32, y: i32, z: i32) -> Self {
        Self {
            x: Fixed::from_i32(x),
            y: Fixed::from_i32(y),
            z: Fixed::from_i32(z),
        }
    }

    pub fn checked_add(self, rhs: Self) -> Result<Self, EngineError> {
        Ok(Self {
            x: self.x.checked_add(rhs.x)?,
            y: self.y.checked_add(rhs.y)?,
            z: self.z.checked_add(rhs.z)?,
        })
    }

    pub fn checked_sub(self, rhs: Self) -> Result<Self, EngineError> {
        Ok(Self {
            x: self.x.checked_sub(rhs.x)?,
            y: self.y.checked_sub(rhs.y)?,
            z: self.z.checked_sub(rhs.z)?,
        })
    }

    pub fn checked_mul_scalar(self, rhs: Fixed) -> Result<Self, EngineError> {
        Ok(Self {
            x: self.x.checked_mul(rhs)?,
            y: self.y.checked_mul(rhs)?,
            z: self.z.checked_mul(rhs)?,
        })
    }

    pub fn dot(self, rhs: Self) -> Result<Fixed, EngineError> {
        self.x
            .checked_mul(rhs.x)?
            .checked_add(self.y.checked_mul(rhs.y)?)?
            .checked_add(self.z.checked_mul(rhs.z)?)
    }
}

fn fixed_from_i128(value: i128, operation: &'static str) -> Result<Fixed, EngineError> {
    if value < i64::MIN as i128 || value > i64::MAX as i128 {
        return Err(EngineError::FixedPointOverflow { operation });
    }
    Ok(Fixed(value as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_translates_points() {
        let transform = Transform::from_translation(Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(
            transform.transform_point3(Vec3::new(1.0, 2.0, 3.0)),
            Vec3::new(4.0, 6.0, 8.0)
        );
    }

    #[test]
    fn aabb_contains_and_unions_points() {
        let first = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::ONE).unwrap();
        let second = Aabb::from_center_half_extents(Vec3::new(3.0, 0.0, 0.0), Vec3::ONE).unwrap();
        let union = first.union(second);
        assert!(union.contains_point(Vec3::new(4.0, 0.0, 0.0)));
        assert_eq!(union.min, Vec3::new(-1.0, -1.0, -1.0));
        assert_eq!(union.max, Vec3::new(4.0, 1.0, 1.0));
    }

    #[test]
    fn invalid_aabb_fails_loud() {
        match Aabb::new(Vec3::new(2.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)) {
            Err(EngineError::InvalidAabb { min, max }) => {
                assert_eq!(min, [2.0, 0.0, 0.0]);
                assert_eq!(max, [1.0, 0.0, 0.0]);
            }
            other => panic!("expected InvalidAabb, got {other:?}"),
        }
    }

    #[test]
    fn fixed_point_operations_are_exact_for_binary_fractions() {
        let one_and_half = Fixed::from_ratio(3, 2).unwrap();
        let product = one_and_half.checked_mul(Fixed::from_i32(2)).unwrap();
        assert_eq!(product.raw(), Fixed::from_i32(3).raw());

        let quotient = Fixed::from_i32(3).checked_div(Fixed::from_i32(2)).unwrap();
        assert_eq!(quotient.raw(), one_and_half.raw());
    }

    #[test]
    fn fixed_vec3_dot_is_deterministic() {
        let left = FixedVec3::new(
            Fixed::from_ratio(3, 2).unwrap(),
            Fixed::from_i32(-2),
            Fixed::from_i32(4),
        );
        let right = FixedVec3::new(
            Fixed::from_i32(2),
            Fixed::from_i32(3),
            Fixed::from_ratio(1, 2).unwrap(),
        );
        assert_eq!(left.dot(right).unwrap().raw(), Fixed::from_i32(-1).raw());
    }

    #[test]
    fn fixed_point_divide_by_zero_fails_loud() {
        match Fixed::from_ratio(1, 0) {
            Err(EngineError::FixedPointDivideByZero) => {}
            other => panic!("expected FixedPointDivideByZero, got {other:?}"),
        }
    }
}
