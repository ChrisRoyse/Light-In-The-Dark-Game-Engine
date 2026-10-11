use super::*;

#[test]
fn default_editor_world_uses_real_stylized_nature_composition() {
    let loaded = crate::asset_vault_fixture::expect_asset_vault(
        load_default_editor_world_asset_scene(),
        "editor.asset_scene.default_composition",
    );
    let report = loaded.report;

    assert_eq!(
        report.product_id,
        "litd.asset.composition.stylized_nature_ed11_v1"
    );
    assert_eq!(
        report.source_relative_path,
        "extracted/quaternius/manual-game-kit/stylizednaturemegakit/Stylized_Nature_MegaKit_Standard"
    );
    assert_eq!(
        report.source_sha256,
        "298f6732b872e4cf7b30e6e7abf9641c7f6dc6b326df37ac089533ed7e3d58c9"
    );
    assert!(report.dependency_count >= 70);
    assert!(report.material_count >= 40);
    assert!(report.texture_count >= 35);
    assert!(report.image_count >= 30);
    assert!(
        report.base_color_texture_source_path.starts_with(
            "generated://texture-atlas/stylized-nature-base-color?tile_size=512&tiles="
        )
    );
    assert_eq!(report.base_color_texture_bytes, 67_108_864);
    let normal_source = &report.normal_texture_source_path;
    assert!(
        normal_source
            .starts_with("generated://texture-atlas/stylized-nature-normal?tile_size=512&tiles=")
    );
    assert!(normal_source.contains("&gltf_normal_tiles="));
    assert!(!normal_source.contains("&gltf_normal_tiles=0&"));
    assert!(normal_source.contains("&manifest_normal_tiles=4"));
    assert_eq!(report.normal_texture_bytes, 67_108_864);
    let mr_source = &report.metallic_roughness_texture_source_path;
    assert!(mr_source.starts_with(
        "generated://texture-atlas/stylized-nature-metallic-roughness?tile_size=512&tiles="
    ));
    assert!(mr_source.contains("&gltf_metallic_roughness_tiles="));
    assert!(!mr_source.contains("&gltf_metallic_roughness_tiles=0&"));
    assert!(mr_source.contains("&manifest_metallic_roughness_tiles=4"));
    assert_eq!(report.metallic_roughness_texture_bytes, 67_108_864);
    assert!(report.vertex_count > 10_000);
    assert_eq!(report.index_count % 3, 0);
    assert_eq!(report.degenerate_triangle_count, 0);
    assert!(report.triangle_count > 75_000);
    assert!(
        report.triangle_count <= 1_048_576,
        "default editor world composition must stay inside the live renderer budget, got {} triangles",
        report.triangle_count
    );
    assert!(
        report.triangle_count >= 1_005_000,
        "foreground Polyhaven cliff, KayKit grave gate dressing, ruin depth clusters, and Kenney skyline towers should be present in the real scene, got {} triangles",
        report.triangle_count
    );
    assert!(report.mesh_primitives >= 75);
    assert!(report.fitted_bounds_min[0] <= -88.0);
    assert!(report.fitted_bounds_max[0] >= 88.0);
    assert!(report.fitted_bounds_min[1] <= -22.0);
    assert!(report.fitted_bounds_max[1] >= 45.0);
    assert!(report.fitted_bounds_min[2] <= -66.0);
    assert!(report.fitted_bounds_max[2] >= 60.0);
    assert_eq!(
        report.world_map.source_kind,
        "composition.stylized_nature_ed11_v1"
    );
    assert_eq!(report.world_map.outline.len(), 64);
    assert_eq!(report.world_map.areas.len(), 32);
    assert_eq!(report.world_map.points.len(), 232);
    assert_eq!(report.world_map.rings.len(), 2);
    let foliage = report
        .foliage
        .as_ref()
        .expect("default ED11 scene needs stored foliage config");
    assert_eq!(foliage.config_id, "litd.foliage.stylized_nature_ed11_v1");
    assert_eq!(foliage.tileset_id, "stylized_nature_ed11_v1");
    assert_eq!(
        foliage.source_relative_path,
        "calyx-aster:tmp/litd-editor-foliage-config-aster"
    );
    // Content-drift guard (#1794 review): pin the derived hash against the frozen golden literal
    // so an accidental foliage-asset edit fails loud here too, not just in the foliage-config test.
    assert_eq!(
        crate::asset_scene_foliage_config::default_foliage_config_source_sha256(),
        crate::asset_scene_foliage_config::DEFAULT_FOLIAGE_CONFIG_GOLDEN_SHA256,
        "foliage asset content drifted from the pinned golden sha256; if this edit was \
         intentional, refresh DEFAULT_FOLIAGE_CONFIG_GOLDEN_SHA256 via `sha256sum \
         crates/litd-editor/assets/foliage/stylized_nature_ed11_v1.json`"
    );
    assert_eq!(
        foliage.source_sha256,
        crate::asset_scene_foliage_config::default_foliage_config_source_sha256()
    );
    assert_eq!(foliage.schema_version, 1);
    assert_eq!(foliage.source_issue, 1271);
    assert_eq!(foliage.prototype_count, 2);
    assert_eq!(foliage.patch_count, 18);
    assert_eq!(foliage.target_clump_count, 214);
    assert_eq!(foliage.placed_clump_count, foliage.target_clump_count);
    assert_eq!(foliage.foliage_index_count, foliage.placed_clump_count * 12);
    assert!(foliage.gpu_instancing_required);
    assert!(
        foliage
            .render_mode_summary
            .contains("instanced_billboard_cross")
    );
    assert_eq!(foliage.cull_range_summary, "70.0..185.0");
    assert!(
        report
            .world_map
            .rings
            .iter()
            .any(|ring| ring.kind == "selection"
                && ring.center_xz[1] >= 30.0
                && ring.radius >= 9.0),
        "ED11 world view needs a visible foreground selection ring; rings={:?}",
        report.world_map.rings
    );
    assert_eq!(report.world_map.feature_count(), 330);
    assert_map_has_area(&report, "polyhaven_ruins_surface");
    assert_map_has_area(&report, "polyhaven_rocky_surface");
    assert_map_has_area(&report, "polyhaven_mossy_surface");
    assert_eq!(count_areas(&report, "ed11_blue_branch_fissure"), 6);
    assert_eq!(count_areas(&report, "ed11_lava_branch_fissure"), 6);
    for kind in [
        "crystal",
        "fire",
        "bones",
        "spire",
        "gothic_arch",
        "gothic_fire",
        "gothic_tower",
        "gothic_wall",
        "polyhaven_fort",
        "polyhaven_cliff",
        "polyhaven_foreground_cliff",
        "ruin_skyline_tower",
        "ruin_skyline_wall",
        "ruin_skyline_banner",
        "graveyard_anchor",
        "graveyard_fence",
        "graveyard_fire",
        "dead_tree",
        "kaykit_gate",
        "kaykit_grave",
        "kaykit_lantern",
        "kaykit_pillar",
        "kenney_skyline_tower",
        "kenney_skyline_roof",
        "kenney_skyline_crystal",
        "kenney_skyline_watchpost",
        "warm_light_pool",
        "cool_light_pool",
        "atmospheric_fog_veil",
        "ed11_depth_fog_card",
        "ed11_foliage_patch",
        "ed11_selection_halo",
    ] {
        assert_map_has_point(&report, kind);
    }
    assert_eq!(
        report
            .world_map
            .points
            .iter()
            .filter(|point| point.kind.starts_with("polyhaven_"))
            .count(),
        8,
        "Poly Haven minimap markers must match the eight real placements without duplicate extension"
    );
    assert_eq!(count_points(&report, "ed11_blue_rift_beacon"), 3);
    assert_eq!(count_points(&report, "ed11_lava_rift_beacon"), 3);
    assert_eq!(count_points(&report, "ed11_depth_fog_card"), 12);
    assert_eq!(count_points(&report, "ed11_foliage_patch"), 18);
    assert_eq!(count_points(&report, "ed11_selection_halo"), 2);
    println!(
        "asset_scene_default_composition source={} sha={} dependencies={} materials={} textures={} images={} vertices={} indices={} triangles={} degenerates={} primitives={} bounds_min={:?} bounds_max={:?} map_kind={} map_features={} foliage_config={} foliage_sha={} foliage_clumps={}/{}",
        report.source_relative_path,
        report.source_sha256,
        report.dependency_count,
        report.material_count,
        report.texture_count,
        report.image_count,
        report.vertex_count,
        report.index_count,
        report.triangle_count,
        report.degenerate_triangle_count,
        report.mesh_primitives,
        report.fitted_bounds_min,
        report.fitted_bounds_max,
        report.world_map.source_kind,
        report.world_map.feature_count(),
        foliage.source_relative_path,
        foliage.source_sha256,
        foliage.placed_clump_count,
        foliage.target_clump_count
    );
}

fn assert_map_has_area(report: &EditorAssetSceneReport, kind: &str) {
    assert!(
        report.world_map.areas.iter().any(|area| area.kind == kind),
        "missing map area kind {kind}"
    );
}

fn assert_map_has_point(report: &EditorAssetSceneReport, kind: &str) {
    assert!(
        report
            .world_map
            .points
            .iter()
            .any(|point| point.kind == kind),
        "missing map point kind {kind}"
    );
}

fn count_points(report: &EditorAssetSceneReport, kind: &str) -> usize {
    report
        .world_map
        .points
        .iter()
        .filter(|point| point.kind == kind)
        .count()
}

fn count_areas(report: &EditorAssetSceneReport, kind: &str) -> usize {
    report
        .world_map
        .areas
        .iter()
        .filter(|area| area.kind == kind)
        .count()
}
