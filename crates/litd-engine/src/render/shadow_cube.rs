use serde::{Deserialize, Serialize};

use crate::math::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CubeFace {
    PositiveX,
    NegativeX,
    PositiveY,
    NegativeY,
    PositiveZ,
    NegativeZ,
}

impl CubeFace {
    pub const ALL: [Self; 6] = [
        Self::PositiveX,
        Self::NegativeX,
        Self::PositiveY,
        Self::NegativeY,
        Self::PositiveZ,
        Self::NegativeZ,
    ];

    pub(super) fn direction_up(self) -> (Vec3, Vec3) {
        match self {
            Self::PositiveX => (Vec3::X, Vec3::Y),
            Self::NegativeX => (-Vec3::X, Vec3::Y),
            Self::PositiveY => (Vec3::Y, -Vec3::Z),
            Self::NegativeY => (-Vec3::Y, Vec3::Z),
            Self::PositiveZ => (Vec3::Z, Vec3::Y),
            Self::NegativeZ => (-Vec3::Z, Vec3::Y),
        }
    }
}
