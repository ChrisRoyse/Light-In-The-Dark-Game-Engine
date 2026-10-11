//! CDLOD per-frame LOD selection: a recursive descent of the quadtree that,
//! for a camera position + optional frustum, emits the set of uniform grid tiles
//! to draw. Mirrors Strugar's `LODSelect`: each level owns a distance-sphere; a
//! node within its level's sphere but beyond the finer sphere is drawn whole,
//! otherwise the descent refines into children. Adds a vertical-error early-out
//! (flat ground stops refining) and streaming coarse-fallback resolution.

use crate::math::Vec3;
use crate::render::camera::CameraFrustum;

use super::config::TerrainConfig;
use super::morph::{MorphParams, morph_factor};
use super::quadtree::{NodeId, QuadNode, TerrainQuadtree};
use super::streaming::NodeAtlas;

/// Where a selected tile's heightmap data actually came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileResidency {
    /// The requested tile's data is resident in the atlas.
    Resident,
    /// The requested tile is still streaming; a coarser resident ancestor's data
    /// is substituted this frame (hole-free degradation).
    Fallback { data: NodeId },
}

/// One tile to draw: a uniform grid stretched over `node`'s extent, morphed at
/// `lod`. `center_morph` is the geomorph factor at the node centre (0 = full
/// detail, 1 = fully collapsed onto the coarser grid) -- a representative value
/// for FSV; the real per-vertex morph is computed in `cdlod_morph.wgsl`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectedTile {
    pub node: QuadNode,
    pub lod: u32,
    pub morph: MorphParams,
    pub center_morph: f32,
    pub residency: TileResidency,
}

/// Immutable per-frame inputs threaded through the recursion.
struct SelectCtx<'a> {
    tree: &'a TerrainQuadtree,
    config: &'a TerrainConfig,
    camera: Vec3,
    frustum: Option<&'a CameraFrustum>,
    atlas: &'a NodeAtlas,
}

/// Outcome of trying to select a node at a level.
#[derive(PartialEq)]
enum Selected {
    /// This node (or its descendants) produced the coverage for its area.
    Handled,
    /// The node is beyond this level's range; the caller must draw it coarser.
    OutOfRange,
}

/// Run CDLOD selection, appending drawn tiles to `out` and queuing missing-tile
/// stream requests into `atlas`. Returns the number of tiles emitted.
pub fn select_tiles(
    tree: &TerrainQuadtree,
    config: &TerrainConfig,
    camera: Vec3,
    frustum: Option<&CameraFrustum>,
    atlas: &mut NodeAtlas,
    out: &mut Vec<SelectedTile>,
) -> usize {
    let start = out.len();
    let mut requests: Vec<(NodeId, f32)> = Vec::new();
    let ctx = SelectCtx {
        tree,
        config,
        camera,
        frustum,
        atlas,
    };
    let root = tree.root();
    let top = tree.lod_count() - 1;
    // The root always covers everything; force it handled so nothing is missed.
    let handled = select_node(&ctx, root.id, top, out, &mut requests);
    if handled == Selected::OutOfRange {
        // Camera is far outside even the coarsest range: still draw the root so
        // the world never vanishes (matches CDLOD's always-draw-root behaviour).
        emit(&ctx, root, top, out);
    }
    for (id, dist_sq) in requests {
        atlas.request(id, dist_sq);
    }
    out.len() - start
}

fn select_node(
    ctx: &SelectCtx,
    id: NodeId,
    lod: u32,
    out: &mut Vec<SelectedTile>,
    requests: &mut Vec<(NodeId, f32)>,
) -> Selected {
    let Some(node) = ctx.tree.node(id) else {
        return Selected::Handled;
    };
    // Frustum cull: an out-of-view node is fully handled (nothing to draw).
    if let Some(frustum) = ctx.frustum
        && !frustum.contains_aabb(node.aabb)
    {
        return Selected::Handled;
    }
    let range = ctx.config.lod_range(lod);
    if node.distance_sq(ctx.camera) > range * range {
        return Selected::OutOfRange;
    }
    // Finest level, or flat enough that refining buys nothing: draw whole node.
    let flat = node.vertical_error < ctx.config.flat_error_cutoff;
    if lod == 0 || flat {
        emit(ctx, node, lod, out);
        record_request(ctx, node.id, requests);
        return Selected::Handled;
    }
    let finer_range = ctx.config.lod_range(lod - 1);
    if node.distance_sq(ctx.camera) > finer_range * finer_range {
        // Entire node sits beyond the finer level's reach: draw it whole here.
        emit(ctx, node, lod, out);
        record_request(ctx, node.id, requests);
        return Selected::Handled;
    }
    // Refine: children that fall out of the finer range are drawn at THIS level.
    for child_id in node.id.children() {
        if select_node(ctx, child_id, lod - 1, out, requests) == Selected::OutOfRange
            && let Some(child) = ctx.tree.node(child_id)
        {
            emit(ctx, child, lod, out);
            record_request(ctx, child.id, requests);
        }
    }
    Selected::Handled
}

/// Push a tile, resolving streaming residency (coarse fallback) and computing the
/// representative geomorph factor at the node centre.
fn emit(ctx: &SelectCtx, node: QuadNode, lod: u32, out: &mut Vec<SelectedTile>) {
    let morph = ctx.config.morph_params(lod);
    let dist = node.distance_sq(ctx.camera).sqrt();
    let center_morph = morph_factor(morph, dist);
    let (drawn, was_fallback) = ctx.atlas.resident_or_coarser(node.id);
    let residency = if was_fallback {
        TileResidency::Fallback { data: drawn }
    } else {
        TileResidency::Resident
    };
    out.push(SelectedTile {
        node,
        lod,
        morph,
        center_morph,
        residency,
    });
}

fn record_request(ctx: &SelectCtx, id: NodeId, requests: &mut Vec<(NodeId, f32)>) {
    if !ctx.atlas.is_resident(id) {
        let dist_sq = ctx
            .tree
            .node(id)
            .map(|n| n.distance_sq(ctx.camera))
            .unwrap_or(f32::INFINITY);
        requests.push((id, dist_sq));
    }
}
