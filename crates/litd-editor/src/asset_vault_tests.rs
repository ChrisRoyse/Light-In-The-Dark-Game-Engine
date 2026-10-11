use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::json;
use sha2::{Digest, Sha256};

use super::*;

#[test]
fn default_catalog_matches_real_local_vault_state() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    crate::asset_vault_fixture::require_full_vault(&catalog);
    let download_record_count = catalog.download_source_counts.values().sum::<usize>();

    assert!(catalog.record_count() >= 10_700);
    assert!(download_record_count >= 6_271);
    assert!(catalog.record_count() - download_record_count >= 4_491);
    assert!(catalog.download_source_count("kaykit") >= 5_295);
    assert!(catalog.download_source_count("quaternius") >= 290);
    assert!(catalog.download_source_count("kenney") >= 14);
    assert!(catalog.usable_record_count() >= 8_000);
    assert!(catalog.records().iter().any(|record| {
        record.status.as_deref() == Some("copied_from_manual_acquisition")
            && record.source == "quaternius"
            && record.absolute_path.exists()
    }));
    assert!(
        catalog
            .download_source_summary_line()
            .contains("kaykit=5295")
    );
    assert!(
        catalog
            .manifest_summary_line()
            .contains("asset_intake_manifest.jsonl")
    );
    let coverage = catalog.coverage_summary().unwrap();
    assert_eq!(coverage.issue1242_p0_status, ISSUE1242_SEMANTIC_STATUS);
    // #1569: assert FUNCTIONAL floors + the all-records-CC0 invariant, NOT an exact
    // point-in-time snapshot. Exact record/asset counts drift on every legitimate intake
    // change and pressured destructive vault rebuilds (the #1569 incident). The real
    // guarantees live elsewhere and stay strict: the required packs are proven present by
    // content-hash (default_editor_world_uses_real_stylized_nature_composition asserts
    // source_sha256 298f6732..., transitively covering the medievalvillage/props/character
    // packs that composition consumes) and the semantic role floors below.
    assert!(catalog.record_type_count("download") >= 15_000);
    assert!(catalog.record_type_count("extraction") >= 8_000);
    assert!(catalog.asset_type_count("audio") >= 216);
    assert!(catalog.asset_type_count("game-kit") >= 20_000);
    assert!(catalog.asset_type_count("material") >= 30);
    assert!(catalog.asset_type_count("model") >= 200);
    assert!(catalog.asset_type_count("surface") >= 450);
    assert!(catalog.asset_type_count("terrain") >= 30);
    // every record must be CC0 whatever the total count (licensing invariant, not a snapshot)
    assert_eq!(
        coverage.licenses,
        format!("CC0-1.0={}", catalog.record_count())
    );
    assert_eq!(coverage.semantic_profile_count, catalog.record_count());
    assert!(coverage.semantic_role_counts.contains("architecture="));
    assert!(coverage.semantic_role_counts.contains("character="));
    assert!(coverage.semantic_role_counts.contains("terrain_surface="));
    assert!(coverage.semantic_role_counts.contains("vfx_light="));
    assert!(coverage.semantic_tag_count >= 16);
    assert!(coverage.download_bytes_on_disk >= 10 * 1024 * 1024 * 1024);
    assert!(coverage.extraction_bytes_on_disk >= 2 * 1024 * 1024 * 1024);
    println!(
        "happy_path records={} downloads={} usable={} sources={} record_types={} asset_types={} licenses={} semantic_roles={} semantic_profiles={} semantic_tags={} download_bytes={} extraction_bytes={} p0_status={}",
        catalog.record_count(),
        download_record_count,
        catalog.usable_record_count(),
        catalog.download_source_summary_line(),
        coverage.record_types,
        coverage.asset_types,
        coverage.licenses,
        coverage.semantic_role_counts,
        coverage.semantic_profile_count,
        coverage.semantic_tag_count,
        coverage.download_bytes_on_disk,
        coverage.extraction_bytes_on_disk,
        coverage.issue1242_p0_status
    );
}

#[test]
fn default_catalog_contains_real_kenney_audio_corpus() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    crate::asset_vault_fixture::require_full_vault(&catalog);
    let expected_paths = [
        "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg",
        "extracted/kenney/audio/interface-sounds/Audio/click_001.ogg",
        "extracted/kenney/audio/rpg-audio/Preview.ogg",
        "extracted/kenney/audio/ui-audio/Audio/click1.ogg",
    ];

    assert!(catalog.asset_type_count("audio") >= 216);
    for relative_path in expected_paths {
        let record = catalog
            .records()
            .iter()
            .find(|record| record.relative_path == relative_path)
            .unwrap_or_else(|| {
                panic!(
                    "{}",
                    crate::asset_vault_fixture::missing_fixture(relative_path)
                )
            });
        assert_eq!(record.source, "kenney");
        assert_eq!(record.asset_type, "audio");
        assert_eq!(record.license, "CC0-1.0");
        assert!(record.is_editor_usable());
        assert!(record.absolute_path.exists());
        assert!(record.bytes_on_disk > 0);
        assert_eq!(record.sha256.len(), 64);
    }
}

#[test]
fn verified_download_status_contract_is_strict() {
    assert!(is_verified_download_status(Some("downloaded")));
    assert!(is_verified_download_status(Some("existing_verified")));
    assert!(is_verified_download_status(Some(
        "copied_from_manual_acquisition"
    )));
    assert!(!is_verified_download_status(None));
    assert!(!is_verified_download_status(Some("manual_unverified")));
}

#[test]
fn default_catalog_tests_use_verified_process_cache() {
    let source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let needle = format!("AssetVaultCatalog::{}(", "load_default");
    let mut offenders = Vec::new();
    for entry in fs::read_dir(&source_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        if text.contains(&needle) {
            offenders.push(path.file_name().unwrap().to_string_lossy().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "use AssetVaultCatalog::load_default_cached() for default real-vault tests; direct cold loads re-hash every asset file: {offenders:?}"
    );
}

#[test]
fn editor_palette_contains_real_kaykit_and_quaternius_files() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    crate::asset_vault_fixture::require_full_vault(&catalog);
    let palette = catalog.editor_palette_records(64);

    assert_eq!(palette.len(), 64);
    assert!(palette.iter().any(|record| record.source == "kaykit"));
    assert!(palette.iter().any(|record| record.source == "quaternius"));
    assert!(palette.iter().all(|record| record.is_editor_usable()));
    assert!(palette.iter().all(|record| record.absolute_path.exists()));
    assert!(palette.iter().any(|record| {
        record
            .relative_path
            .to_ascii_lowercase()
            .contains("dungeon")
    }));
    println!(
        "palette first={} second={} len={}",
        palette[0].relative_path,
        palette[1].relative_path,
        palette.len()
    );
}

#[test]
fn missing_manifest_path_fails_loud() {
    let (_temp_root, path) = temp_manifest_path("missing");
    println!("edge=missing before_exists={}", path.exists());
    let error = AssetVaultCatalog::load_from_manifest(&path, "download")
        .unwrap_err()
        .to_string();

    assert!(error.contains("editor.asset_vault.manifest.path"));
    assert!(error.contains("manifest does not exist"));
    println!(
        "edge=missing after_exists={} error={}",
        path.exists(),
        error
    );
}

#[test]
fn empty_manifest_fails_loud() {
    let (_temp_root, path) = temp_manifest_path("empty");
    println!("edge=empty before_exists={}", path.exists());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "").unwrap();

    let error = AssetVaultCatalog::load_from_manifest(&path, "download")
        .unwrap_err()
        .to_string();

    assert!(error.contains("editor.asset_vault.manifest.records"));
    assert!(error.contains("manifest is empty"));
    println!(
        "edge=empty after_exists={} bytes={} error={}",
        path.exists(),
        fs::metadata(&path).unwrap().len(),
        error
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn bad_json_manifest_fails_loud_with_line_number() {
    let (_temp_root, path) = temp_manifest_path("bad-json");
    println!("edge=bad_json before_exists={}", path.exists());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{not-json}\n").unwrap();

    let error = AssetVaultCatalog::load_from_manifest(&path, "download")
        .unwrap_err()
        .to_string();

    assert!(error.contains("editor.asset_vault.manifest.json"));
    assert!(error.contains(":1 invalid JSONL"));
    println!(
        "edge=bad_json after_exists={} bytes={} error={}",
        path.exists(),
        fs::metadata(&path).unwrap().len(),
        error
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn same_size_corrupted_file_fails_loud_with_sha_readback() {
    let record = small_real_download_record();
    let (_temp_root, manifest, copied_file) =
        copied_record_manifest("same-size-corrupt", &record, None);
    let before_hash = file_sha256(&copied_file);
    let before_len = fs::metadata(&copied_file).unwrap().len();
    println!(
        "edge=same_size_corrupt before_exists={} before_len={} manifest_sha={} actual_sha={}",
        copied_file.exists(),
        before_len,
        record.sha256,
        before_hash
    );
    let mut bytes = fs::read(&copied_file).unwrap();
    bytes[0] ^= 0x5a;
    fs::write(&copied_file, &bytes).unwrap();

    let error = AssetVaultCatalog::load_from_manifest(&manifest, "download")
        .unwrap_err()
        .to_string();
    let after_hash = file_sha256(&copied_file);
    let after_len = fs::metadata(&copied_file).unwrap().len();

    assert_eq!(before_len, after_len);
    assert_ne!(before_hash, after_hash);
    assert!(error.contains("editor.asset_vault.file.sha256"));
    assert!(error.contains(&record.sha256));
    println!(
        "edge=same_size_corrupt after_exists={} after_len={} actual_sha={} error={}",
        copied_file.exists(),
        after_len,
        after_hash,
        error
    );
}

#[test]
fn manifest_sha_mismatch_fails_loud_for_real_copied_file() {
    let record = small_real_download_record();
    let bad_hash = "0".repeat(64);
    let (_temp_root, manifest, copied_file) =
        copied_record_manifest("manifest-sha-mismatch", &record, Some(&bad_hash));
    let actual_hash = file_sha256(&copied_file);
    println!(
        "edge=manifest_sha_mismatch before_exists={} len={} manifest_sha={} actual_sha={}",
        copied_file.exists(),
        fs::metadata(&copied_file).unwrap().len(),
        bad_hash,
        actual_hash
    );

    let error = AssetVaultCatalog::load_from_manifest(&manifest, "download")
        .unwrap_err()
        .to_string();

    assert_eq!(actual_hash, record.sha256);
    assert!(error.contains("editor.asset_vault.file.sha256"));
    assert!(error.contains(&bad_hash));
    println!(
        "edge=manifest_sha_mismatch after_exists={} actual_sha={} error={}",
        copied_file.exists(),
        file_sha256(&copied_file),
        error
    );
}

#[test]
fn manifest_file_missing_fails_loud_after_real_record_copy_removed() {
    let record = small_real_download_record();
    let (_temp_root, manifest, copied_file) =
        copied_record_manifest("missing-copied-file", &record, None);
    println!(
        "edge=missing_file before_exists={} len={}",
        copied_file.exists(),
        fs::metadata(&copied_file).unwrap().len()
    );
    fs::remove_file(&copied_file).unwrap();

    let error = AssetVaultCatalog::load_from_manifest(&manifest, "download")
        .unwrap_err()
        .to_string();

    assert!(!copied_file.exists());
    assert!(error.contains("editor.asset_vault.file.exists"));
    println!(
        "edge=missing_file after_exists={} error={}",
        copied_file.exists(),
        error
    );
}

// #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
fn temp_manifest_path(name: &str) -> (litd_engine::test_temp::TestTempDir, PathBuf) {
    let root = litd_engine::test_temp::TestTempDir::new(&format!("editor-asset-vault-{name}"));
    let path = root.path().join("manifests").join(format!("{name}.jsonl"));
    (root, path)
}

fn small_real_download_record() -> AssetVaultRecord {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    catalog
        .records()
        .iter()
        .find(|record| {
            record.record_type == "download"
                && record.bytes_on_disk > 0
                && record.bytes_on_disk <= 50_000
                && record.absolute_path.exists()
        })
        .expect("real asset vault must contain a small verified download record")
        .clone()
}

fn copied_record_manifest(
    name: &str,
    source_record: &AssetVaultRecord,
    sha_override: Option<&str>,
) -> (litd_engine::test_temp::TestTempDir, PathBuf, PathBuf) {
    // #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
    let root = litd_engine::test_temp::TestTempDir::new(&format!("editor-asset-vault-{name}"));
    let relative_path = format!(
        "sources/hash-edge/{}",
        source_record
            .absolute_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("asset.bin")
    );
    let copied_file = root.path().join(&relative_path);
    fs::create_dir_all(copied_file.parent().unwrap()).unwrap();
    fs::copy(&source_record.absolute_path, &copied_file).unwrap();
    let manifest = root.path().join("manifests").join("download.jsonl");
    fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    let line = json!({
        "record_type": "download",
        "source": &source_record.source,
        "asset_id": &source_record.asset_id,
        "asset_type": &source_record.asset_type,
        "title": &source_record.title,
        "license": &source_record.license,
        "license_url": &source_record.license_url,
        "source_url": &source_record.source_url,
        "relative_path": relative_path,
        "absolute_path": copied_file.display().to_string(),
        "bytes_on_disk": fs::metadata(&copied_file).unwrap().len(),
        "sha256": sha_override.unwrap_or(&source_record.sha256),
        "status": source_record.status.as_deref().unwrap_or("downloaded"),
    });
    fs::write(&manifest, format!("{line}\n")).unwrap();
    (root, manifest, copied_file)
}

fn file_sha256(path: &Path) -> String {
    let bytes = fs::read(path).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}
