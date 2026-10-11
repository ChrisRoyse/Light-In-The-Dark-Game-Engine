//! Implicit CDLOD quadtree built over a [`HeightField`]. Each node stores the
//! bound (min/max height -> AABB) and a vertical error the selector uses for
//! adaptive-density early-out. Level 0 = finest (leaf), root = one whole-world
//! node at level `lod_count-1`.

use crate::EngineError;
use crate::math::{Aabb, Vec3};

use super::config::{HeightField, TerrainConfig};

/// Address of a node: LOD level plus node-grid coordinates at that level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    pub lod: u32,
    pub x: u32,
    pub z: u32,
}

impl NodeId {
    pub fn new(lod: u32, x: u32, z: u32) -> Self {
        Self { lod, x, z }
    }

    /// The four children of this node one LOD finer. Order: (0,0),(1,0),(0,1),(1,1).
    pub fn children(self) -> [NodeId; 4] {
        let cl = self.lod - 1;
        let (bx, bz) = (self.x * 2, self.z * 2);
        [
            NodeId::new(cl, bx, bz),
            NodeId::new(cl, bx + 1, bz),
            NodeId::new(cl, bx, bz + 1),
            NodeId::new(cl, bx + 1, bz + 1),
        ]
    }
}

/// A resolved quadtree node with its world bound and roughness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadNode {
    pub id: NodeId,
    pub aabb: Aabb,
    pub min_height: f32,
    pub max_height: f32,
    /// Vertical error (world units) ~ how much detail is lost drawing this whole
    /// node coarse. Height range is a deterministic, monotone-under-refinement
    /// proxy; children always have error <= parent.
    pub vertical_error: f32,
}

impl QuadNode {
    /// Squared distance from `point` to this node's AABB (0 inside).
    pub fn distance_sq(&self, point: Vec3) -> f32 {
        let c = point.clamp(self.aabb.min, self.aabb.max);
        (point - c).length_squared()
    }
}

/// The whole tree, indexed `[lod][z * side(lod) + x]`.
#[derive(Debug, Clone)]
pub struct TerrainQuadtree {
    levels: Vec<Vec<QuadNode>>,
    leaf_sample_span: u32,
}

impl TerrainQuadtree {
    /// Build the tree. `leaf_sample_span` (samples spanned by one leaf tile edge)
    /// is derived from the field size and the config's leaf count.
    pub fn build(field: &HeightField, config: &TerrainConfig) -> Result<Self, EngineError> {
        let leaf = config.leaf_nodes_per_side;
        if !(field.size - 1).is_multiple_of(leaf) {
            return Err(EngineError::TerrainRuntimeInvalidConfig {
                field: "quadtree.field_size".to_string(),
                reason: format!(
                    "(size-1)={} must be divisible by leaf_nodes_per_side={leaf} so each leaf owns \
                     an integer sample patch",
                    field.size - 1
                ),
            });
        }
        let leaf_sample_span = (field.size - 1) / leaf;

        // Level 0: fill every leaf's bound directly from the samples it covers.
        let mut levels: Vec<Vec<QuadNode>> = Vec::with_capacity(config.lod_count() as usize);
        let mut leaves = Vec::with_capacity((leaf * leaf) as usize);
        for z in 0..leaf {
            for x in 0..leaf {
                leaves.push(build_leaf(field, config, leaf_sample_span, x, z));
            }
        }
        levels.push(leaves);

        // Coarser levels: aggregate four children (bottom-up union).
        let mut side = leaf;
        let mut lod = 1;
        while side > 1 {
            let coarse_side = side / 2;
            let child = &levels[(lod - 1) as usize];
            let mut coarse = Vec::with_capacity((coarse_side * coarse_side) as usize);
            for z in 0..coarse_side {
                for x in 0..coarse_side {
                    coarse.push(build_parent(config, child, side, lod, x, z));
                }
            }
            levels.push(coarse);
            side = coarse_side;
            lod += 1;
        }

        Ok(Self {
            levels,
            leaf_sample_span,
        })
    }

    pub fn lod_count(&self) -> u32 {
        self.levels.len() as u32
    }

    pub fn leaf_sample_span(&self) -> u32 {
        self.leaf_sample_span
    }

    /// Node count on one axis at `lod`.
    pub fn side(&self, lod: u32) -> u32 {
        self.levels[lod as usize].len().isqrt() as u32
    }

    /// The single root (coarsest) node.
    pub fn root(&self) -> QuadNode {
        self.levels[self.levels.len() - 1][0]
    }

    pub fn node(&self, id: NodeId) -> Option<QuadNode> {
        let level = self.levels.get(id.lod as usize)?;
        let side = level.len().isqrt() as u32;
        if id.x >= side || id.z >= side {
            return None;
        }
        Some(level[(id.z * side + id.x) as usize])
    }

    /// Total node count across all levels (FSV stat).
    pub fn total_nodes(&self) -> usize {
        self.levels.iter().map(Vec::len).sum()
    }
}

fn build_leaf(field: &HeightField, config: &TerrainConfig, span: u32, x: u32, z: u32) -> QuadNode {
    let sx0 = x * span;
    let sz0 = z * span;
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for sz in sz0..=sz0 + span {
        for sx in sx0..=sx0 + span {
            let h = field.sample(sx, sz);
            lo = lo.min(h);
            hi = hi.max(h);
        }
    }
    let min = Vec3::new(field.sample_to_world(sx0), lo, field.sample_to_world(sz0));
    let max = Vec3::new(
        field.sample_to_world(sx0 + span),
        hi,
        field.sample_to_world(sz0 + span),
    );
    let aabb = Aabb::new(min, max).expect("leaf bound min<=max by construction");
    let _ = config;
    QuadNode {
        id: NodeId::new(0, x, z),
        aabb,
        min_height: lo,
        max_height: hi,
        vertical_error: hi - lo,
    }
}

fn build_parent(
    config: &TerrainConfig,
    child_level: &[QuadNode],
    child_side: u32,
    lod: u32,
    x: u32,
    z: u32,
) -> QuadNode {
    let idx = |cx: u32, cz: u32| (cz * child_side + cx) as usize;
    let kids = [
        child_level[idx(x * 2, z * 2)],
        child_level[idx(x * 2 + 1, z * 2)],
        child_level[idx(x * 2, z * 2 + 1)],
        child_level[idx(x * 2 + 1, z * 2 + 1)],
    ];
    let mut aabb = kids[0].aabb;
    let mut lo = kids[0].min_height;
    let mut hi = kids[0].max_height;
    for kid in &kids[1..] {
        aabb = aabb.union(kid.aabb);
        lo = lo.min(kid.min_height);
        hi = hi.max(kid.max_height);
    }
    let _ = config;
    QuadNode {
        id: NodeId::new(lod, x, z),
        aabb,
        min_height: lo,
        max_height: hi,
        vertical_error: hi - lo,
    }
}
