use litd_hash::sha256_hex;
use std::{fs, path::PathBuf};

use litd_editor::{
    asset_lod::{
        AssetLodSettings, build_asset_lod_product, read_asset_lod_product,
        validate_asset_lod_product, write_asset_lod_product,
    },
    asset_products::product_from_vault_record,
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
};
use serde_json::json;

const MOUNTAINSIDE_GLTF: &str = "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf";
const DUNGEON_TEXTURE: &str = "sources/kaykit/game-kit/KayKit-Dungeon-Remastered-1.0/addons/kaykit_dungeon_remastered/Assets/texture/dungeon_texture.png";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1102_FSV_ROOT",
        "issue1102_asset_lod",
        "manual",
    )?;
    let product = product_from_vault_record(&real_record(MOUNTAINSIDE_GLTF)?)?;
    let lod_dir = root.join("lod-buffers");
    let manifest_path = root.join("asset-lod-product.json");
    let artifact_path = root.join("asset-lod-readback.json");

    println!(
        "happy before source={} sha={} output_dir={}",
        product.source_relative_path,
        product.source_sha256,
        lod_dir.display()
    );
    let lod = build_asset_lod_product(&product, AssetLodSettings::default(), &lod_dir)?;
    let manifest_bytes = write_asset_lod_product(&manifest_path, &lod)?;
    let parsed = read_asset_lod_product(&manifest_path)?;
    if parsed != lod {
        return Err("LOD manifest readback mismatch".into());
    }
    println!(
        "happy after levels={} source_triangles={} lod_triangles={:?} manifest_sha={}",
        parsed.levels.len(),
        parsed.source_triangle_count,
        parsed
            .levels
            .iter()
            .map(|level| level.output_triangle_count)
            .collect::<Vec<_>>(),
        sha256_hex(&manifest_bytes)
    );

    let texture_edge = texture_edge(&root)?;
    let ratio_edge = ratio_edge(&product, &root)?;
    let corruption_edge = corruption_edge(&product, &root)?;
    validate_asset_lod_product(&parsed)?;
    let artifact = json!({
        "issue": 1102,
        "source_of_truth": "local-asset-vault physical glTF file plus generated LOD index buffers and manifest JSON read back from disk",
        "manifest_path": manifest_path,
        "manifest_sha256": sha256_hex(&manifest_bytes),
        "happy_path": parsed,
        "edge_cases": [texture_edge, ratio_edge, corruption_edge]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err("artifact readback mismatch".into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1102 asset LOD FSV artifact={} bytes={} sha256={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256
    );
    Ok(())
}

fn texture_edge(root: &std::path::Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let product = product_from_vault_record(&real_record(DUNGEON_TEXTURE)?)?;
    let edge_dir = root.join("texture-edge");
    let before_exists = edge_dir.exists();
    let error = build_asset_lod_product(&product, AssetLodSettings::default(), &edge_dir)
        .expect_err("texture product must fail")
        .to_string();
    let after_count = count_entries_if_exists(&edge_dir)?;
    println!(
        "edge_texture before_exists={} after_file_count={} error={}",
        before_exists, after_count, error
    );
    Ok(json!({
        "name": "texture_product_rejected",
        "before_exists": before_exists,
        "after_file_count": after_count,
        "error": error
    }))
}

fn ratio_edge(
    product: &litd_editor::asset_products::AssetProduct,
    root: &std::path::Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let edge_dir = root.join("bad-ratio-edge");
    let before_exists = edge_dir.exists();
    let settings = AssetLodSettings {
        target_index_ratios: vec![1.0, 1.0],
        ..AssetLodSettings::default()
    };
    let error = build_asset_lod_product(product, settings, &edge_dir)
        .expect_err("duplicate LOD ratios must fail")
        .to_string();
    let after_exists = edge_dir.exists();
    println!(
        "edge_bad_ratio before_exists={} after_exists={} error={}",
        before_exists, after_exists, error
    );
    Ok(json!({
        "name": "invalid_ratio_rejected_before_write",
        "before_exists": before_exists,
        "after_exists": after_exists,
        "error": error
    }))
}

fn corruption_edge(
    product: &litd_editor::asset_products::AssetProduct,
    root: &std::path::Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let edge_dir = root.join("corrupt-edge");
    let lod = build_asset_lod_product(product, AssetLodSettings::default(), &edge_dir)?;
    let corrupt_path = PathBuf::from(&lod.levels[1].index_buffer_path);
    let before_len = fs::metadata(&corrupt_path)?.len();
    fs::write(&corrupt_path, b"broken")?;
    let after_len = fs::metadata(&corrupt_path)?.len();
    let error = validate_asset_lod_product(&lod)
        .expect_err("corrupt index buffer must fail")
        .to_string();
    println!(
        "edge_corrupt before_len={} after_len={} error={}",
        before_len, after_len, error
    );
    Ok(json!({
        "name": "corrupt_index_buffer_rejected",
        "before_len": before_len,
        "after_len": after_len,
        "error": error
    }))
}

fn real_record(relative_path: &str) -> Result<AssetVaultRecord, Box<dyn std::error::Error>> {
    AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .ok_or_else(|| format!("missing real asset vault record {relative_path}").into())
}

fn count_entries_if_exists(path: &std::path::Path) -> Result<usize, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(0);
    }
    Ok(fs::read_dir(path)?.count())
}
