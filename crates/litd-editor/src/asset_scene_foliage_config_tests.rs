use super::*;
use std::fs;

#[test]
fn default_foliage_config_loads_real_tileset_source() {
    let config = load_default_foliage_config().unwrap();

    assert_eq!(config.schema_version, 1);
    assert_eq!(config.config_id, "litd.foliage.stylized_nature_ed11_v1");
    assert_eq!(config.tileset_id, "stylized_nature_ed11_v1");
    assert_eq!(config.source_issue, 1271);
    assert_eq!(
        config.source_relative_path,
        "calyx-aster:tmp/litd-editor-foliage-config-aster"
    );
    // Content-drift guard (#1794 review): the derived helper only proves the Aster round-trip
    // preserved the bytes it was handed; pinning it against the frozen golden literal is what
    // catches an accidental edit to the committed asset. A legitimate edit updates one literal.
    assert_eq!(
        default_foliage_config_source_sha256(),
        DEFAULT_FOLIAGE_CONFIG_GOLDEN_SHA256,
        "foliage asset content drifted from the pinned golden sha256; if this edit was \
         intentional, refresh DEFAULT_FOLIAGE_CONFIG_GOLDEN_SHA256 via `sha256sum \
         crates/litd-editor/assets/foliage/stylized_nature_ed11_v1.json`"
    );
    assert_eq!(config.source_sha256, default_foliage_config_source_sha256());
    assert_eq!(config.prototypes.len(), 2);
    assert_eq!(config.patches.len(), 18);
    assert_eq!(config.target_clump_count(), 214);
    assert!(config.gpu_instancing_required());
    assert_eq!(
        config.render_mode_summary(),
        "ed11_grass_clump:instanced_billboard_cross|ed11_dead_shrub_clump:instanced_billboard_cross"
    );
    assert_eq!(config.cull_range_summary(), "70.0..185.0");
}

#[test]
fn foliage_config_rejects_unknown_patch_prototype() {
    let broken = DEFAULT_CONFIG_JSON.replacen(
        "\"prototype_id\": \"ed11_grass_clump\", \"center_xz\"",
        "\"prototype_id\": \"missing_grass\", \"center_xz\"",
        1,
    );
    let broken_path = workspace_root().join("target").join("tmp").join(format!(
        "litd-foliage-config-broken-{}-aster",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&broken_path);
    litd_engine::calyx_json_store::write_calyx_json_bytes(
        &config_spec(&broken_path),
        broken.as_bytes(),
    )
    .unwrap();

    let error = match load_foliage_config(&broken_path) {
        Ok(_) => panic!("expected unknown patch prototype to fail"),
        Err(error) => error.to_string(),
    };

    assert!(error.contains("editor.asset_scene.foliage_config.patch.prototype_id"));
    assert!(error.contains("missing_grass"));
    fs::remove_dir_all(broken_path).unwrap();
}

#[test]
fn foliage_config_rejects_equal_cull_range() {
    let mut json: serde_json::Value = serde_json::from_str(DEFAULT_CONFIG_JSON).unwrap();
    let first = json["prototypes"][0].as_object_mut().unwrap();
    let cull_start = first["cull_start"].clone();
    first.insert("cull_end".to_string(), cull_start);
    let broken_path = workspace_root().join("target").join("tmp").join(format!(
        "litd-foliage-config-equal-cull-{}-aster",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&broken_path);
    litd_engine::calyx_json_store::write_calyx_json_bytes(
        &config_spec(&broken_path),
        &serde_json::to_vec_pretty(&json).unwrap(),
    )
    .unwrap();

    let error = match load_foliage_config(&broken_path) {
        Ok(_) => panic!("expected equal cull range to fail"),
        Err(error) => error.to_string(),
    };

    assert!(error.contains("editor.asset_scene.foliage_config.prototype.cull"));
    assert!(error.contains("ordered"));
    fs::remove_dir_all(broken_path).unwrap();
}
