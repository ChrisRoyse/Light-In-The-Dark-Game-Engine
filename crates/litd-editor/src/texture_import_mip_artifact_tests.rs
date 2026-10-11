use litd_hash::sha256_hex;
use std::{fs, path::PathBuf};

use super::*;
use crate::asset_vault::{AssetVaultCatalog, AssetVaultRecord};

const REAL_PNG: &str = "extracted/kenney/game-kit/tiny-dungeon/Tiles/tile_0000.png";

#[test]
fn issue1347_texture_import_mip_artifacts_source_truth_fsv() {
    let root = test_dir("issue1347_texture_import_mip_artifacts");
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    let before_exists = root.exists();
    let (texture, artifacts, manifest) = write_issue1347_artifacts(&root.join("happy"));
    let after_exists = root.exists();
    assert!(!before_exists);
    assert!(after_exists);
    assert_imported_mip_chain(&texture);
    assert_eq!(manifest.textures.len(), 1);
    assert_eq!(
        manifest.texture_files.len(),
        1 + texture.receipt.gpu_mip_levels.len()
    );
    let mip_files = manifest
        .texture_files
        .iter()
        .filter(|file| file.kind == "bc7")
        .collect::<Vec<_>>();
    assert_eq!(mip_files.len(), texture.receipt.gpu_mip_levels.len());
    for file in &mip_files {
        let bytes = fs::read(&file.path).unwrap();
        assert_eq!(file.bytes, bytes.len());
        assert_eq!(file.sha256, sha256_hex(&bytes));
    }
    println!(
        "issue1347 happy before_root_exists={} after_root_exists={} manifest={} texture={} mip_levels={} mip_files={} manifest_sha={}",
        before_exists,
        after_exists,
        artifacts.manifest_path,
        texture.receipt.source_relative_path,
        texture.receipt.gpu_mip_levels.len(),
        mip_files.len(),
        artifacts.manifest_sha256
    );

    let (missing_before, missing_after, missing_error) = issue1347_missing_mip_edge(&root);
    let (corrupt_before_sha, corrupt_after_sha, corrupt_error) = issue1347_corrupt_mip_edge(&root);
    let (manifest_before_count, manifest_after_count, manifest_error) =
        issue1347_manifest_missing_mip_edge(&root);

    let report = serde_json::json!({
        "schema_version": 1,
        "source_of_truth": {
            "manifest_path": artifacts.manifest_path,
            "artifact_root": root,
            "texture": texture.receipt.source_relative_path,
        },
        "happy": {
            "before_root_exists": before_exists,
            "after_root_exists": after_exists,
            "mip_levels": texture.receipt.gpu_mip_levels.len(),
            "mip_files": mip_files.len(),
            "aggregate_bytes": texture.receipt.gpu_transcode_bytes,
            "aggregate_sha256": texture.receipt.gpu_transcode_sha256,
        },
        "edges": {
            "missing_mip_file": {
                "before_exists": missing_before,
                "after_exists": missing_after,
                "error_contains_file_read": missing_error.contains("editor.texture_import.artifacts.file.read"),
            },
            "corrupt_mip_file": {
                "before_sha256": corrupt_before_sha,
                "after_sha256": corrupt_after_sha,
                "error_contains_sha256": corrupt_error.contains("editor.texture_import.artifacts.file.sha256"),
            },
            "manifest_missing_mip_entry": {
                "before_texture_files": manifest_before_count,
                "after_texture_files": manifest_after_count,
                "error_contains_mip_file": manifest_error.contains("editor.texture_import.artifacts.mip.file"),
            },
        },
    });
    let report_path = root.join("manual-fsv-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let report_bytes = fs::read(&report_path).unwrap();
    let report_readback: serde_json::Value = serde_json::from_slice(&report_bytes).unwrap();
    assert_eq!(
        report_readback["happy"]["mip_files"],
        report_readback["happy"]["mip_levels"]
    );
    assert_eq!(
        report_readback["edges"]["missing_mip_file"]["error_contains_file_read"],
        true
    );
    assert_eq!(
        report_readback["edges"]["corrupt_mip_file"]["error_contains_sha256"],
        true
    );
    assert_eq!(
        report_readback["edges"]["manifest_missing_mip_entry"]["error_contains_mip_file"],
        true
    );
    println!(
        "issue1347 report={} report_sha={} missing_edge_before={} missing_edge_after={} corrupt_before_sha={} corrupt_after_sha={} manifest_before_count={} manifest_after_count={}",
        report_path.display(),
        sha256_hex(&report_bytes),
        missing_before,
        missing_after,
        corrupt_before_sha,
        corrupt_after_sha,
        manifest_before_count,
        manifest_after_count
    );
}

fn real_record(catalog: &AssetVaultCatalog, relative_path: &str) -> AssetVaultRecord {
    catalog
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

fn test_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("fsv")
        .join(name)
}

fn file_sha256(path: &std::path::Path) -> String {
    sha256_hex(&fs::read(path).unwrap())
}

fn assert_imported_mip_chain(texture: &ProcessedTextureAsset) {
    assert!(!texture.gpu_mip_levels.is_empty());
    assert_eq!(
        texture.receipt.mipmaps_generated,
        texture.gpu_mip_levels.len() > 1
    );
    assert_eq!(
        texture.receipt.gpu_mip_levels.len(),
        texture.gpu_mip_levels.len()
    );
    let mut aggregate = Vec::new();
    for (index, mip) in texture.gpu_mip_levels.iter().enumerate() {
        assert_eq!(mip.receipt.level_index, index as u32);
        assert_eq!(texture.receipt.gpu_mip_levels[index], mip.receipt);
        assert_eq!(mip.receipt.gpu_transcode_format, "BC7_RGBA");
        assert_eq!(mip.receipt.bytes, mip.bytes.len());
        assert_eq!(mip.receipt.sha256, sha256_hex(&mip.bytes));
        aggregate.extend_from_slice(&mip.bytes);
    }
    assert_eq!(texture.receipt.gpu_transcode_bytes, aggregate.len());
    assert_eq!(texture.receipt.gpu_transcode_sha256, sha256_hex(&aggregate));
    assert_eq!(texture.gpu_transcode_bytes, aggregate);
}

fn write_issue1347_artifacts(
    root: &std::path::Path,
) -> (
    ProcessedTextureAsset,
    TextureImportArtifactReceipt,
    TextureImportManifest,
) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let texture =
        import_texture_asset(&real_record(&catalog, REAL_PNG), TextureRole::UiSprite).unwrap();
    let atlas = build_texture_atlas(
        std::slice::from_ref(&texture),
        TextureAtlasSettings {
            tile_size: 64,
            columns: 1,
            max_size: 64,
        },
    )
    .unwrap();
    let artifacts =
        write_texture_import_artifacts(root, std::slice::from_ref(&texture), &atlas).unwrap();
    let manifest = read_texture_import_manifest(&artifacts.manifest_path).unwrap();
    (texture, artifacts, manifest)
}

fn first_mip_file(manifest: &TextureImportManifest, level_index: u32) -> TextureArtifactFile {
    manifest
        .texture_files
        .iter()
        .find(|file| file.kind == "bc7" && file.level_index == Some(level_index))
        .cloned()
        .unwrap_or_else(|| panic!("missing BC7 mip level {level_index} artifact"))
}

fn issue1347_missing_mip_edge(root: &std::path::Path) -> (bool, bool, String) {
    let edge_root = root.join("edge_missing_mip");
    let (_texture, artifacts, manifest) = write_issue1347_artifacts(&edge_root);
    let file = first_mip_file(&manifest, 1);
    let before_exists = std::path::Path::new(&file.path).exists();
    fs::remove_file(&file.path).unwrap();
    let after_exists = std::path::Path::new(&file.path).exists();
    let error = read_texture_import_manifest(&artifacts.manifest_path)
        .unwrap_err()
        .to_string();
    println!(
        "issue1347 edge missing_mip before_exists={} after_exists={} error={}",
        before_exists, after_exists, error
    );
    (before_exists, after_exists, error)
}

fn issue1347_corrupt_mip_edge(root: &std::path::Path) -> (String, String, String) {
    let edge_root = root.join("edge_corrupt_mip");
    let (_texture, artifacts, manifest) = write_issue1347_artifacts(&edge_root);
    let file = first_mip_file(&manifest, 0);
    let before_sha = file_sha256(std::path::Path::new(&file.path));
    let corrupt = vec![0x5a; file.bytes];
    fs::write(&file.path, corrupt).unwrap();
    let after_sha = file_sha256(std::path::Path::new(&file.path));
    assert_ne!(before_sha, after_sha);
    let error = read_texture_import_manifest(&artifacts.manifest_path)
        .unwrap_err()
        .to_string();
    println!(
        "issue1347 edge corrupt_mip before_sha={} after_sha={} error={}",
        before_sha, after_sha, error
    );
    (before_sha, after_sha, error)
}

fn issue1347_manifest_missing_mip_edge(root: &std::path::Path) -> (usize, usize, String) {
    let edge_root = root.join("edge_manifest_missing_mip");
    let (_texture, artifacts, _manifest) = write_issue1347_artifacts(&edge_root);
    let manifest_bytes = fs::read(&artifacts.manifest_path).unwrap();
    let mut manifest: TextureImportManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    let before_count = manifest.texture_files.len();
    let removed = manifest
        .texture_files
        .iter()
        .find(|file| file.kind == "bc7" && file.level_index == Some(1))
        .cloned()
        .unwrap();
    manifest
        .texture_files
        .retain(|file| file.path != removed.path);
    let after_count = manifest.texture_files.len();
    fs::write(
        &artifacts.manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let error = read_texture_import_manifest(&artifacts.manifest_path)
        .unwrap_err()
        .to_string();
    println!(
        "issue1347 edge manifest_missing_mip before_count={} after_count={} error={}",
        before_count, after_count, error
    );
    (before_count, after_count, error)
}
