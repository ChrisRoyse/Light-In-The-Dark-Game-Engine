//! Unit tests for the GPU-driven two-phase Hi-Z cull core (#1740). These prove the
//! pure-logic surface (pyramid layout, draw-path selection, CPU two-phase cull) at
//! T1; the GPU compute readback lives in the FSV example harness.

use super::*;
use crate::math::{Aabb, Vec2, Vec3};
use crate::render::batching::{Frustum, RenderInstance, ScreenRect};
use crate::render::gpu_driven_cpu::{build_base_depth, build_pyramid, run_two_phase_cpu};

fn wide_frustum() -> Frustum {
    Frustum::orthographic(
        Aabb::new(Vec3::new(-50.0, -50.0, -50.0), Vec3::new(50.0, 50.0, 50.0)).unwrap(),
    )
    .unwrap()
}

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> ScreenRect {
    ScreenRect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y)).unwrap()
}

fn instance(id: u64, screen: ScreenRect, depth: f32) -> RenderInstance {
    RenderInstance {
        id,
        mesh_id: "mesh".to_string(),
        material_id: "mat".to_string(),
        world_bounds: Aabb::from_center_half_extents(Vec3::new(0.0, 0.0, 0.0), Vec3::splat(0.1))
            .unwrap(),
        screen_rect: screen,
        nearest_depth: depth,
        index_count: 36,
    }
}

#[test]
fn pyramid_dims_pack_every_level_back_to_back() {
    let dims = HzbPyramidDims::new(64).unwrap();
    assert_eq!(dims.base_dim(), 64);
    assert_eq!(dims.level_count(), 7); // 64,32,16,8,4,2,1
    assert_eq!(dims.level_dim(0), 64);
    assert_eq!(dims.level_dim(6), 1);
    assert_eq!(dims.level_offset(0), 0);
    assert_eq!(dims.level_offset(1), 64 * 64);
    // Sum of squares 64..1 = 4096+1024+256+64+16+4+1.
    assert_eq!(dims.total_texels(), 5461);
}

#[test]
fn pyramid_dims_reject_non_power_of_two() {
    assert!(HzbPyramidDims::new(0).is_err());
    assert!(HzbPyramidDims::new(48).is_err());
    assert!(HzbPyramidDims::new(8192).is_err());
    assert!(HzbPyramidDims::new(32).is_ok());
}

#[test]
fn max_reduce_pyramid_takes_the_farthest_child() {
    let dims = HzbPyramidDims::new(2).unwrap();
    // base 2x2 -> one 1x1 parent that must be the MAX (farthest) of the four.
    let base = vec![0.1, 0.4, 0.2, 0.3];
    let pyramid = build_pyramid(&dims, &base).unwrap();
    assert_eq!(pyramid[dims.level_offset(1) as usize], 0.4);
}

#[test]
fn base_depth_seeds_nearest_of_previous_visible() {
    let dims = HzbPyramidDims::new(4).unwrap();
    // Two overlapping prev-visible occluders on the same texel: base takes the nearer.
    let instances = vec![
        instance(1, rect(0.0, 0.0, 0.99, 0.99), 0.6),
        instance(2, rect(0.0, 0.0, 0.99, 0.99), 0.3),
    ];
    let base = build_base_depth(&dims, &instances, &[true, true]).unwrap();
    assert!(base.iter().all(|&d| (d - 0.3).abs() < 1e-6));
    // With nothing previously visible the whole level stays at the far plane.
    let empty = build_base_depth(&dims, &instances, &[false, false]).unwrap();
    assert!(empty.iter().all(|&d| (d - 1.0).abs() < 1e-6));
}

#[test]
fn two_phase_culls_instances_behind_a_previously_visible_wall() {
    // A near wall (depth 0.30) was visible last frame; it primes the pyramid over the
    // left half of the screen. Two instances sit behind it (0.80), one in front (0.10).
    let wall = instance(1, rect(0.0, 0.0, 0.5, 1.0), 0.30);
    let behind_a = instance(2, rect(0.10, 0.20, 0.14, 0.26), 0.80);
    let behind_b = instance(3, rect(0.30, 0.60, 0.34, 0.66), 0.80);
    let in_front = instance(4, rect(0.20, 0.40, 0.24, 0.46), 0.10);
    let instances = vec![wall, behind_a, behind_b, in_front];
    let prev_visible = vec![true, false, false, false];

    let result = run_two_phase_cpu(
        TwoPhaseSettings::default(),
        &wide_frustum(),
        &instances,
        &prev_visible,
    )
    .unwrap();

    assert!(result.is_consistent());
    // The two behind-wall instances (indices 1,2) are culled; wall + front survive.
    assert_eq!(result.visible_indices, vec![0, 3]);
    assert_eq!(result.classes[1], VisibilityClass::Culled);
    assert_eq!(result.classes[2], VisibilityClass::Culled);
    // The wall was visible last frame -> phase 1; the front instance is new -> phase 2.
    assert_eq!(result.classes[0], VisibilityClass::Phase1);
    assert_eq!(result.classes[3], VisibilityClass::Phase2);
    assert_eq!(result.phase1_count, 1);
    assert_eq!(result.phase2_count, 1);
}

#[test]
fn edge_all_visible_when_hzb_disabled() {
    let settings = TwoPhaseSettings {
        hzb_enabled: false,
        ..TwoPhaseSettings::default()
    };
    let wall = instance(1, rect(0.0, 0.0, 0.5, 1.0), 0.30);
    let behind = instance(2, rect(0.10, 0.20, 0.14, 0.26), 0.80);
    let instances = vec![wall, behind];
    let result = run_two_phase_cpu(settings, &wide_frustum(), &instances, &[true, false]).unwrap();
    // Nothing is occluded when Hi-Z is off; both survive (frustum only).
    assert_eq!(result.visible_count, 2);
}

#[test]
fn edge_empty_scene_yields_zero_draws() {
    let result = run_two_phase_cpu(TwoPhaseSettings::default(), &wide_frustum(), &[], &[]).unwrap();
    assert_eq!(result.visible_count, 0);
    assert!(result.visible_indices.is_empty());
    assert!(result.is_consistent());
}

#[test]
fn select_draw_path_is_explicit_never_silent() {
    // GPU requested + supported -> GPU.
    let sel = select_draw_path(DrawPathMode::GpuIndirect, true).unwrap();
    assert_eq!(sel.selected, DrawPathMode::GpuIndirect);
    assert_eq!(sel.reason, DrawPathReason::GpuRequestedAndSupported);
    // CPU requested -> CPU, deliberately.
    let sel = select_draw_path(DrawPathMode::CpuFallback, true).unwrap();
    assert_eq!(sel.selected, DrawPathMode::CpuFallback);
    assert_eq!(sel.reason, DrawPathReason::CpuRequested);
    // GPU requested but unsupported -> HARD ERROR, never a silent CPU fallback.
    assert!(select_draw_path(DrawPathMode::GpuIndirect, false).is_err());
}

#[test]
fn prev_visible_length_mismatch_fails_loud() {
    let dims = HzbPyramidDims::new(8).unwrap();
    let instances = vec![instance(1, rect(0.0, 0.0, 0.2, 0.2), 0.5)];
    assert!(build_base_depth(&dims, &instances, &[]).is_err());
}
