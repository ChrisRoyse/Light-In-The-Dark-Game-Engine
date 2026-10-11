use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Aabb, Vec3},
};

use super::{invalid_ui, validate_aabb};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FogCell {
    Hidden,
    Explored,
    Visible,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FogOfWarGrid {
    pub width: u32,
    pub height: u32,
    pub world_bounds: Aabb,
    pub cells: Vec<FogCell>,
}

impl FogOfWarGrid {
    pub fn new(width: u32, height: u32, world_bounds: Aabb) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return invalid_ui("fog.grid", "width and height must be non-zero");
        }
        validate_aabb("fog.world_bounds", world_bounds)?;
        Ok(Self {
            width,
            height,
            world_bounds,
            cells: vec![FogCell::Hidden; (width * height) as usize],
        })
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.width == 0 || self.height == 0 {
            return invalid_ui("fog.grid", "width and height must be non-zero");
        }
        if self.cells.len() != (self.width * self.height) as usize {
            return invalid_ui("fog.cells", "cell count must match width*height");
        }
        validate_aabb("fog.world_bounds", self.world_bounds)
    }

    fn index(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    fn cell_center(&self, x: u32, y: u32) -> Vec3 {
        let size = self.world_bounds.max - self.world_bounds.min;
        let fx = (x as f32 + 0.5) / self.width as f32;
        let fz = (y as f32 + 0.5) / self.height as f32;
        Vec3::new(
            self.world_bounds.min.x + size.x * fx,
            0.0,
            self.world_bounds.min.z + size.z * fz,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VisionSource {
    pub world_position: Vec3,
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FogSummary {
    pub hidden: usize,
    pub explored: usize,
    pub visible: usize,
}

pub fn reveal_fog(
    grid: &FogOfWarGrid,
    sources: &[VisionSource],
) -> Result<FogOfWarGrid, EngineError> {
    grid.validate()?;
    let mut next = grid.clone();
    for cell in &mut next.cells {
        if *cell == FogCell::Visible {
            *cell = FogCell::Explored;
        }
    }
    for source in sources {
        if !source.world_position.is_finite() || !source.radius.is_finite() || source.radius <= 0.0
        {
            return invalid_ui("fog.vision_source", "position and radius must be finite");
        }
        for y in 0..next.height {
            for x in 0..next.width {
                let center = next.cell_center(x, y);
                if distance_xz(center, source.world_position) <= source.radius {
                    let index = next.index(x, y);
                    next.cells[index] = FogCell::Visible;
                }
            }
        }
    }
    Ok(next)
}

pub fn summarize_fog(grid: &FogOfWarGrid) -> FogSummary {
    let mut summary = FogSummary {
        hidden: 0,
        explored: 0,
        visible: 0,
    };
    for cell in &grid.cells {
        match cell {
            FogCell::Hidden => summary.hidden += 1,
            FogCell::Explored => summary.explored += 1,
            FogCell::Visible => summary.visible += 1,
        }
    }
    summary
}

fn distance_xz(a: Vec3, b: Vec3) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    (dx * dx + dz * dz).sqrt()
}
