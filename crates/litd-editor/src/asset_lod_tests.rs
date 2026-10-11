use std::{fs, path::PathBuf};

use crate::{
    asset_lod::{
        AssetLodSettings, build_asset_lod_product, read_asset_lod_product,
        validate_asset_lod_product, write_asset_lod_product,
    },
    asset_products::{AssetProductKind, product_from_vault_record},
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
};

const MOUNTAINSIDE_GLTF: &str = "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf";
const DUNGEON_TEXTURE: &str = "sources/kaykit/game-kit/KayKit-Dungeon-Remastered-1.0/addons/kaykit_dungeon_remastered/Assets/texture/dungeon_texture.png";

#[test]
fn real_gltf_generates_lod_index_products_and_manifest_readback() {
    let product = product_from_vault_record(&real_record(MOUNTAINSIDE_GLTF)).unwrap();
    let root = test_root("happy");
    let lod_dir = root.join("lod");
    let manifest_path = root.join("asset-lod-product.json");
    fs::create_dir_all(&lod_dir).unwrap();

    let lod = build_asset_lod_product(&product, AssetLodSettings::default(), &lod_dir).unwrap();
    let manifest_bytes = write_asset_lod_product(&manifest_path, &lod).unwrap();
    let readback = read_asset_lod_product(&manifest_path).unwrap();

    assert_eq!(readback, lod);
    assert_eq!(readback.levels.len(), 3);
    assert_eq!(
        readback.levels[0].output_index_count,
        readback.source_index_count
    );
    assert!(readback.levels[1].output_index_count < readback.levels[0].output_index_count);
    assert!(readback.levels[2].output_index_count < readback.levels[1].output_index_count);
    for level in &readback.levels {
        let bytes = fs::read(&level.index_buffer_path).unwrap();
        assert_eq!(bytes.len() as u64, level.index_buffer_bytes);
        assert_eq!(level.output_triangle_count, level.output_index_count / 3);
    }
    println!(
        "issue1102 happy before_source_indices={} after_levels={} lod0={} lod1={} lod2={} manifest_bytes={}",
        readback.source_index_count,
        readback.levels.len(),
        readback.levels[0].output_index_count,
        readback.levels[1].output_index_count,
        readback.levels[2].output_index_count,
        manifest_bytes.len()
    );
}

#[test]
fn texture_product_is_rejected_without_writing_lod_files() {
    let product = product_from_vault_record(&real_record(DUNGEON_TEXTURE)).unwrap();
    assert_eq!(product.product_kind, AssetProductKind::Texture);
    let root = test_root("texture");
    let before_exists = root.exists();
    let error = build_asset_lod_product(&product, AssetLodSettings::default(), &root)
        .unwrap_err()
        .to_string();
    let after_count = count_entries_if_exists(&root);

    assert!(error.contains("editor.asset_lod.product.kind"));
    assert_eq!(after_count, 0);
    println!(
        "issue1102 edge_texture before_exists={} after_file_count={} error={}",
        before_exists, after_count, error
    );
}

#[test]
fn invalid_ratio_settings_fail_before_writing() {
    let product = product_from_vault_record(&real_record(MOUNTAINSIDE_GLTF)).unwrap();
    let root = test_root("bad-ratio");
    let settings = AssetLodSettings {
        target_index_ratios: vec![1.0, 1.0],
        ..AssetLodSettings::default()
    };
    let before_exists = root.exists();
    let error = build_asset_lod_product(&product, settings, &root)
        .unwrap_err()
        .to_string();
    let after_exists = root.exists();

    assert!(error.contains("editor.asset_lod.settings.ratios"));
    assert_eq!(before_exists, after_exists);
    println!(
        "issue1102 edge_bad_ratio before_exists={} after_exists={} error={}",
        before_exists, after_exists, error
    );
}

#[test]
fn corrupted_index_buffer_fails_manifest_validation() {
    let product = product_from_vault_record(&real_record(MOUNTAINSIDE_GLTF)).unwrap();
    let root = test_root("corrupt");
    let lod = build_asset_lod_product(&product, AssetLodSettings::default(), &root).unwrap();
    let corrupt_path = PathBuf::from(&lod.levels[1].index_buffer_path);
    let before_len = fs::metadata(&corrupt_path).unwrap().len();
    fs::write(&corrupt_path, b"broken").unwrap();
    let after_len = fs::metadata(&corrupt_path).unwrap().len();
    let error = validate_asset_lod_product(&lod).unwrap_err().to_string();

    assert!(error.contains("editor.asset_lod.level.index_buffer"));
    println!(
        "issue1102 edge_corrupt before_len={} after_len={} error={}",
        before_len, after_len, error
    );
}

fn real_record(relative_path: &str) -> AssetVaultRecord {
    AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "{}",
                crate::asset_vault_fixture::missing_fixture(relative_path)
            )
        })
}

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1102", label)
}

fn count_entries_if_exists(path: &std::path::Path) -> usize {
    if !path.exists() {
        return 0;
    }
    fs::read_dir(path).unwrap().count()
}
