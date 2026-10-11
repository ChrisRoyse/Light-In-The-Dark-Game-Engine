//! Unit proofs for the CDLOD terrain runtime: geomorph continuity, quadtree
//! bounds, constant vertex budget as the world grows, adaptive-density flat
//! early-out, and streaming coarse fallback.

use crate::math::Vec3;

use super::config::{HeightField, TerrainConfig};
use super::morph::{MorphParams, morph_factor, morph_grid_xz, morph_height};
use super::quadtree::{NodeId, TerrainQuadtree};
use super::runtime::{TerrainRuntime, TileResidency};

/// A rolling-hills heightfield: deterministic, plenty of vertical error so LODs
/// actually refine. `leaf` leaf tiles per side, `span` samples per leaf edge.
fn hilly(leaf: u32, span: u32) -> HeightField {
    let size = leaf * span + 1;
    let mut heights = Vec::with_capacity((size * size) as usize);
    for z in 0..size {
        for x in 0..size {
            let fx = x as f32 * 0.35;
            let fz = z as f32 * 0.35;
            heights.push(5.0 * (fx.sin() + fz.cos()));
        }
    }
    HeightField::new(size, 1.0, heights).expect("valid hilly field")
}

fn flat(leaf: u32, span: u32) -> HeightField {
    let size = leaf * span + 1;
    HeightField::new(size, 1.0, vec![0.0; (size * size) as usize]).expect("valid flat field")
}

fn config(leaf: u32) -> TerrainConfig {
    // grid_dim 9, lod0 hand-off 6 world units, morph starts at 66% of a range,
    // flat cutoff 0.05 world units.
    TerrainConfig::new(leaf, 9, 6.0, 0.66, 0.05).expect("valid config")
}

#[test]
fn morph_factor_spans_zero_to_one_monotonically() {
    let p = MorphParams::new(4.0, 6.0);
    // Edge 1: inside morph start -> no morph.
    assert_eq!(morph_factor(p, 3.9), 0.0);
    assert_eq!(morph_factor(p, 4.0), 0.0);
    // Edge 2: at morph end -> fully morphed.
    assert_eq!(morph_factor(p, 6.0), 1.0);
    assert_eq!(morph_factor(p, 99.0), 1.0);
    // Edge 3: midpoint -> exactly half, and strictly increasing across the band.
    assert!((morph_factor(p, 5.0) - 0.5).abs() < 1e-6);
    let a = morph_factor(p, 4.5);
    let b = morph_factor(p, 5.5);
    assert!(a < b && a > 0.0 && b < 1.0);
}

#[test]
fn geomorph_collapses_odd_vertices_onto_coarser_grid_no_popping() {
    // The no-popping guarantee: at morph == 1 every odd grid vertex lands exactly
    // on its lower even neighbour (== the coarser LOD grid), and even vertices
    // never move. So the tile silhouette becomes identical to the coarse tile
    // that takes over at the hand-off distance -> zero positional discontinuity.
    for grid in 0..9u32 {
        let full = morph_grid_xz(grid, 0.0);
        assert_eq!(full, grid as f32, "no morph must keep full detail");
        let collapsed = morph_grid_xz(grid, 1.0);
        if grid.is_multiple_of(2) {
            assert_eq!(
                collapsed, grid as f32,
                "even vertices are shared, never move"
            );
        } else {
            assert_eq!(
                collapsed,
                (grid - 1) as f32,
                "odd vertex snaps to even neighbour"
            );
        }
    }
    // Height blends to the coarse sample at morph == 1 (C0 seam continuity).
    assert!((morph_height(2.0, 7.0, 0.0) - 2.0).abs() < 1e-6);
    assert!((morph_height(2.0, 7.0, 1.0) - 7.0).abs() < 1e-6);
    assert!((morph_height(2.0, 7.0, 0.5) - 4.5).abs() < 1e-6);
}

#[test]
fn quadtree_bounds_aggregate_bottom_up() {
    let field = hilly(8, 4);
    let cfg = config(8);
    let tree = TerrainQuadtree::build(&field, &cfg).expect("build tree");
    assert_eq!(tree.lod_count(), 4, "8 leaves/side -> lods 0..3");
    assert_eq!(tree.side(0), 8);
    assert_eq!(tree.side(3), 1, "root is a single node");

    // The root bound must contain every leaf bound (min/max monotone up the tree).
    let root = tree.root();
    for x in 0..8 {
        for z in 0..8 {
            let leaf = tree.node(NodeId::new(0, x, z)).expect("leaf exists");
            assert!(leaf.min_height >= root.min_height - 1e-3);
            assert!(leaf.max_height <= root.max_height + 1e-3);
            // Children never rougher than the parent.
            assert!(leaf.vertical_error <= root.vertical_error + 1e-3);
        }
    }
    // Total node count for a complete quadtree of 4 levels: 64+16+4+1 = 85.
    assert_eq!(tree.total_nodes(), 85);
}

#[test]
fn selection_spans_at_least_three_lod_rings() {
    // A 16x16-leaf world (half-extent 32) is wide enough that the camera-to-edge
    // distance crosses at least three LOD range bands (6, 12, 24, ...).
    let field = hilly(16, 4);
    let mut runtime = TerrainRuntime::new(&field, config(16)).expect("runtime");
    // Camera near ground at the world centre so it sits over the finest ring.
    let frame = runtime.select_frame(Vec3::new(0.0, 2.0, 0.0), None);
    assert!(frame.stats.tile_count > 0);
    assert!(
        frame.stats.distinct_lods >= 3,
        "camera should see >=3 LOD rings, got {} (per lod {:?})",
        frame.stats.distinct_lods,
        frame.stats.tiles_per_lod,
    );
    // Vertex budget accounting is exact: tile_count * grid_dim^2.
    assert_eq!(
        frame.stats.total_vertices,
        frame.stats.tile_count as u64 * 81
    );
}

#[test]
fn vertex_budget_stays_bounded_as_world_area_grows() {
    // Same camera, same LOD ranges; only the world grows. CDLOD adds coarse rings
    // (logarithmic) rather than more near-field tiles, so the vertex budget must
    // grow FAR slower than the 16x area increase from 8x8 -> 32x32 leaves.
    let cam = Vec3::new(0.0, 2.0, 0.0);
    let small = TerrainRuntime::new(&hilly(8, 4), config(8))
        .unwrap()
        .select_frame(cam, None)
        .stats;
    let big = TerrainRuntime::new(&hilly(32, 4), config(32))
        .unwrap()
        .select_frame(cam, None)
        .stats;

    // The world area grew 16x; the vertex budget must stay well within 3x (it is
    // ~logarithmic in world size) while the LOD ring count increases.
    assert!(
        big.distinct_lods > small.distinct_lods,
        "bigger world must add coarser LOD rings: {} vs {}",
        big.distinct_lods,
        small.distinct_lods,
    );
    assert!(
        big.total_vertices < small.total_vertices * 3,
        "vertex budget must stay bounded: small={} big={} (area grew 16x)",
        small.total_vertices,
        big.total_vertices,
    );
}

#[test]
fn flat_ground_draws_coarse_via_vertical_error_early_out() {
    // A perfectly flat world has zero vertical error everywhere, so the selector
    // must stop at the root: one coarse tile, not a refined near-field.
    let field = flat(8, 4);
    let mut runtime = TerrainRuntime::new(&field, config(8)).expect("runtime");
    let frame = runtime.select_frame(Vec3::new(0.0, 2.0, 0.0), None);
    assert_eq!(
        frame.stats.tile_count, 1,
        "flat ground should collapse to a single coarse tile"
    );
    assert_eq!(frame.tiles[0].lod, runtime.tree().lod_count() - 1);
}

#[test]
fn streaming_coarse_fallback_keeps_coverage_when_atlas_is_full() {
    // Bounded atlas: only the root is resident. Every finer selected tile must
    // fall back to a coarser resident ancestor -- coverage stays hole-free, and
    // the runtime records the fallback plus a stream request for the real tile.
    // Capacity 2: only the root fits initially (so every finer tile falls back);
    // one extra slot lets us stream a single target in and watch it resolve.
    let field = hilly(8, 4);
    let mut runtime = TerrainRuntime::with_atlas_capacity(&field, config(8), 2).expect("runtime");
    let frame = runtime.select_frame(Vec3::new(0.0, 2.0, 0.0), None);
    assert!(frame.stats.tile_count > 1);
    // With only the root resident, any tile finer than the root is a fallback.
    let non_root: Vec<_> = frame
        .tiles
        .iter()
        .filter(|t| t.lod < runtime.tree().lod_count() - 1)
        .collect();
    assert!(!non_root.is_empty(), "expected some finer tiles");
    for tile in &non_root {
        assert!(
            matches!(tile.residency, TileResidency::Fallback { .. }),
            "finer tile must fall back to a coarser resident ancestor"
        );
    }
    assert!(frame.stats.fallback_count > 0);
    assert!(
        frame.stats.request_count > 0,
        "missing tiles must be requested"
    );

    // After streaming a specific tile in, it stops being a fallback next frame.
    let target = non_root[0].node.id;
    assert!(runtime.stream_in(target));
    let frame2 = runtime.select_frame(Vec3::new(0.0, 2.0, 0.0), None);
    let resolved = frame2
        .tiles
        .iter()
        .find(|t| t.node.id == target)
        .expect("target still selected");
    assert_eq!(resolved.residency, TileResidency::Resident);
}

#[test]
fn config_rejects_non_power_of_two_leaf_count() {
    assert!(TerrainConfig::new(6, 9, 6.0, 0.66, 0.05).is_err());
    assert!(TerrainConfig::new(8, 1, 6.0, 0.66, 0.05).is_err());
    assert!(TerrainConfig::new(8, 9, 6.0, 1.5, 0.05).is_err());
}
