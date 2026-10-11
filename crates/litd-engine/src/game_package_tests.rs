use std::{fs, path::PathBuf};

use serde_json::json;

use crate::{
    fsv_artifacts::begin_issue_run,
    game_package::{
        LITD_LOCK_IN_POLICY, LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimePackageAssetRef,
        RuntimePackageEndpoint, RuntimePackageHostMode, compute_runtime_manifest_fingerprint,
        compute_runtime_manifest_fingerprint_bound, load_runtime_locked_package,
    },
    game_runtime::RuntimeDatasetsManifest,
};

#[test]
fn issue1165_runtime_locked_package_source_of_truth_fsv() {
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("ashen-reach.litdgame.json");
    let package = package_value("issue1165runtimelock", LITD_RUNTIME_ID).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&package).unwrap()).unwrap();

    println!(
        "issue1165 happy before_load path={} bytes={}",
        path.display(),
        fs::metadata(&path).unwrap().len()
    );
    let readback = load_runtime_locked_package(&path, "litd-game-cli").unwrap();
    let disk_bytes = fs::read(&path).unwrap();

    assert_eq!(readback.runtime_id, LITD_RUNTIME_ID);
    assert_eq!(readback.lock_in_policy, LITD_LOCK_IN_POLICY);
    assert!(readback.runtime_lock_verified);
    assert!(readback.manifest_integrity_verified);
    assert!(readback.playable_on_litd_runtime);
    assert_eq!(readback.assets.len(), 1);
    assert_eq!(
        readback.document["document_id"],
        "calyx://world/issue1165-runtime-lock"
    );
    assert_eq!(readback.package_bytes, disk_bytes.len());
    println!(
        "issue1165 happy after_load package={} runtime={} lock={} manifest_ok={} playable={} sha={} bytes={}",
        readback.package_id,
        readback.runtime_id,
        readback.runtime_lock_verified,
        readback.manifest_integrity_verified,
        readback.playable_on_litd_runtime,
        readback.package_sha256,
        readback.package_bytes
    );
}

#[test]
fn issue1165_runtime_package_edges_fail_closed() {
    let root = test_root("edges");
    fs::create_dir_all(&root).unwrap();

    let foreign_path = root.join("foreign-runtime.litdgame.json");
    let foreign = package_value("issue1165foreignruntime", "foreign.runtime.v1").unwrap();
    fs::write(&foreign_path, serde_json::to_vec_pretty(&foreign).unwrap()).unwrap();
    let before_bytes = fs::read(&foreign_path).unwrap();
    let foreign_error = load_runtime_locked_package(&foreign_path, "litd-game-cli")
        .unwrap_err()
        .to_string();
    let after_bytes = fs::read(&foreign_path).unwrap();
    assert_eq!(before_bytes, after_bytes);
    assert!(foreign_error.contains("runtime_lock"));
    println!(
        "issue1165 edge_foreign before_bytes={} after_bytes={} state_unchanged={} error={}",
        before_bytes.len(),
        after_bytes.len(),
        before_bytes == after_bytes,
        foreign_error
    );

    let tampered_path = root.join("tampered-manifest.litdgame.json");
    let mut tampered = package_value("issue1165tampered", LITD_RUNTIME_ID).unwrap();
    tampered["manifest_fingerprint"] = json!("0000000000000000");
    fs::write(
        &tampered_path,
        serde_json::to_vec_pretty(&tampered).unwrap(),
    )
    .unwrap();
    let before_bytes = fs::read(&tampered_path).unwrap();
    let tampered_error = load_runtime_locked_package(&tampered_path, "litd-game-cli")
        .unwrap_err()
        .to_string();
    let after_bytes = fs::read(&tampered_path).unwrap();
    assert_eq!(before_bytes, after_bytes);
    assert!(tampered_error.contains("manifest_fingerprint"));
    println!(
        "issue1165 edge_tampered before_bytes={} after_bytes={} state_unchanged={} error={}",
        before_bytes.len(),
        after_bytes.len(),
        before_bytes == after_bytes,
        tampered_error
    );

    let wrong_extension_path = root.join("portable-export.json");
    fs::write(
        &wrong_extension_path,
        serde_json::to_vec_pretty(&package_value("issue1165extension", LITD_RUNTIME_ID).unwrap())
            .unwrap(),
    )
    .unwrap();
    let before_exists = wrong_extension_path.exists();
    let extension_error = load_runtime_locked_package(&wrong_extension_path, "litd-game-cli")
        .unwrap_err()
        .to_string();
    assert!(wrong_extension_path.exists());
    assert!(extension_error.contains("package_path.extension"));
    println!(
        "issue1165 edge_extension before_exists={} after_exists={} error={}",
        before_exists,
        wrong_extension_path.exists(),
        extension_error
    );
}

fn package_value(
    share_id: &str,
    runtime_id: &str,
) -> Result<serde_json::Value, crate::EngineError> {
    package_value_with_assets(
        share_id,
        runtime_id,
        vec![RuntimePackageAssetRef {
            product_id: "litd.asset.issue1165.locked.mesh".to_string(),
            source_relative_path: "assets/issue1165/locked.mesh".to_string(),
            source_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            source_bytes: 2048,
        }],
    )
}

fn package_value_with_assets(
    share_id: &str,
    runtime_id: &str,
    assets: Vec<RuntimePackageAssetRef>,
) -> Result<serde_json::Value, crate::EngineError> {
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 11650,
        path: format!("/share/{share_id}"),
    };
    let document_fingerprint = "1165ad0c1165ad0c";
    let play_build_fingerprint = "1165beef1165beef";
    let manifest_fingerprint = compute_runtime_manifest_fingerprint(
        share_id,
        "agent.issue1165",
        runtime_id,
        &endpoint,
        document_fingerprint,
        play_build_fingerprint,
        &assets,
    )?;
    let host_uri = endpoint.uri();
    let asset_count = assets.len();
    Ok(json!({
        "schema": LITD_QUICK_SHARE_SCHEMA,
        "share_id": share_id,
        "package_id": format!("litd.quick_share.{share_id}"),
        "created_by": "agent.issue1165",
        "runtime_id": runtime_id,
        "lock_in_policy": LITD_LOCK_IN_POLICY,
        "endpoint": endpoint,
        "host_uri": host_uri,
        "document": {
            "schema": "litd.lowered_document.v1",
            "document_id": "calyx://world/issue1165-runtime-lock"
        },
        "document_fingerprint": document_fingerprint,
        "play_build_fingerprint": play_build_fingerprint,
        "asset_count": asset_count,
        "assets": assets,
        "manifest_fingerprint": manifest_fingerprint
    }))
}

#[test]
fn issue1464_tampered_asset_bytes_fail_loud_and_matching_assets_verify() {
    let root = test_root("asset-verify");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("with-asset.litdgame.json");

    // A real asset file co-located with the package.
    let asset_bytes = b"issue1464 real asset bytes for package verification".to_vec();
    let asset_rel = "assets/issue1464/real.bin";
    let asset_path = root.join(asset_rel);
    fs::create_dir_all(asset_path.parent().unwrap()).unwrap();
    fs::write(&asset_path, &asset_bytes).unwrap();

    let asset = RuntimePackageAssetRef {
        product_id: "litd.asset.issue1464.real".to_string(),
        source_relative_path: asset_rel.to_string(),
        source_sha256: litd_hash::sha256_hex(&asset_bytes),
        source_bytes: asset_bytes.len() as u64,
    };
    let package =
        package_value_with_assets("issue1464assetverify", LITD_RUNTIME_ID, vec![asset.clone()])
            .unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&package).unwrap()).unwrap();

    // Happy path: the co-located asset hashes to its declared source_sha256.
    let readback = load_runtime_locked_package(&path, "litd-game-cli").unwrap();
    assert!(
        readback.asset_bytes_verified,
        "a matching co-located asset must set asset_bytes_verified true"
    );
    // Source of truth: re-hash the file on disk independently.
    assert_eq!(
        litd_hash::sha256_hex(&fs::read(&asset_path).unwrap()),
        asset.source_sha256
    );

    // Substitute the asset with same-length but different bytes (preserving the
    // byte count so the sha256 check, not the length check, is what fails).
    // Before #1464 load still reported the package integrity-verified and
    // playable; now it must fail loud on the content-hash mismatch.
    let substituted = vec![0xAB_u8; asset_bytes.len()];
    fs::write(&asset_path, &substituted).unwrap();
    let error = load_runtime_locked_package(&path, "litd-game-cli")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("source_sha256") && error.contains("substituted"),
        "substituted asset must fail loud on hash mismatch, got: {error}"
    );
    println!("issue1464 asset_tamper fail_loud error={error}");
}

#[test]
fn issue1513_declared_asset_without_colocated_bytes_is_not_byte_verified() {
    let root = test_root("asset-gate-1513");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("missing-asset.litdgame.json");

    // Declare an asset but DO NOT write its co-located bytes — this models the pre-#1513
    // installed package that ships a manifest only. The load is not an error (the bytes
    // may simply not be co-located), but the package is NOT byte-verified. A self-contained
    // install (#1513) MUST materialize these bytes and refuse to finish if it cannot, so
    // that this readback state can never be trusted as a verified installed package.
    let asset_bytes = b"issue1513 declared-but-unshipped asset payload".to_vec();
    let asset = RuntimePackageAssetRef {
        product_id: "litd.asset.issue1513.gate".to_string(),
        source_relative_path: "assets/issue1513/gate.bin".to_string(),
        source_sha256: litd_hash::sha256_hex(&asset_bytes),
        source_bytes: asset_bytes.len() as u64,
    };
    let package =
        package_value_with_assets("issue1513assetgate", LITD_RUNTIME_ID, vec![asset.clone()])
            .unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&package).unwrap()).unwrap();

    // Source of truth = the readback state: a declared-but-absent asset leaves
    // asset_bytes_verified false.
    let readback = load_runtime_locked_package(&path, "litd-game-cli").unwrap();
    assert!(
        !readback.asset_bytes_verified,
        "a declared asset with no co-located bytes must leave asset_bytes_verified false"
    );

    // Materialize the co-located bytes (what a self-contained install does): the package
    // becomes byte-verified. Independently re-hash the file on disk.
    let asset_path = root.join(&asset.source_relative_path);
    fs::create_dir_all(asset_path.parent().unwrap()).unwrap();
    fs::write(&asset_path, &asset_bytes).unwrap();
    assert_eq!(
        litd_hash::sha256_hex(&fs::read(&asset_path).unwrap()),
        asset.source_sha256
    );
    let readback = load_runtime_locked_package(&path, "litd-game-cli").unwrap();
    assert!(
        readback.asset_bytes_verified,
        "#1513: once every declared asset byte is present and verified the package is self-contained"
    );
    println!(
        "issue1513 gate: absent_bytes asset_bytes_verified=false -> materialized asset_bytes_verified={}",
        readback.asset_bytes_verified
    );
}

/// Build a self-consistent runtime datasets manifest. `seed` and `worker_gold` are the two
/// knobs the test varies to get two DIFFERENT but individually valid manifests.
fn runtime_manifest(seed: u64, worker_gold: u32) -> RuntimeDatasetsManifest {
    use crate::game_runtime::players::{PlayerController, PlayerSlot};
    use crate::game_runtime::{
        AuthoredUnit, EconomyRulesetAuthor, LoweredResourceKind, LoweredStructureType,
        LoweredUnitEconomy, LoweredUpkeepTier, MatchBlueprint, VictoryCondition,
        lower_runtime_constants,
    };

    let economy = EconomyRulesetAuthor::new(4, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 10_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "no".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: "worker".to_string(),
            gold_cost: worker_gold,
            lumber_cost: 0,
            food_cost: 0,
            build_time_secs: 1,
            food_provided: 0,
        })
        .structure(LoweredStructureType {
            struct_id: "hall".to_string(),
            gold_cost: 200,
            lumber_cost: 0,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 20,
            trains: vec!["worker".to_string()],
        })
        .lower()
        .expect("lower economy");
    let slots = vec![
        PlayerSlot {
            id: 0,
            controller: PlayerController::Human,
            force: 0,
            color: 0,
            handicap_pct: 100,
        },
        PlayerSlot {
            id: 1,
            controller: PlayerController::Computer,
            force: 1,
            color: 1,
            handicap_pct: 100,
        },
    ];
    let mut blueprint = MatchBlueprint::new("mini", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    blueprint.author_unit(AuthoredUnit {
        unit: 1,
        owner: 0,
        is_structure: true,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 2,
        owner: 1,
        is_structure: true,
        capabilities: Vec::new(),
    });
    let lowered_match = blueprint.lower().expect("lower match");
    let constants =
        lower_runtime_constants(4, 200, 6_000, 500, 1_000, 16, 16, seed).expect("constants");
    RuntimeDatasetsManifest::assemble(economy, lowered_match, constants).expect("assemble")
}

fn runtime_package_value(
    share_id: &str,
    manifest: &RuntimeDatasetsManifest,
) -> Result<serde_json::Value, crate::EngineError> {
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 8493,
        path: format!("/share/{share_id}"),
    };
    let assets = vec![RuntimePackageAssetRef {
        product_id: "litd.asset.issue1843.rt".to_string(),
        source_relative_path: "assets/issue1843/rt.bin".to_string(),
        source_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_string(),
        source_bytes: 1024,
    }];
    let document_fingerprint = "18431843deadbeef";
    let play_build_fingerprint = "1843beef1843beef";
    // Bind the envelope fingerprint over the runtime datasets fingerprint (#1843/MINOR-1).
    let manifest_fingerprint = compute_runtime_manifest_fingerprint_bound(
        share_id,
        "agent.issue1843",
        LITD_RUNTIME_ID,
        &endpoint,
        document_fingerprint,
        play_build_fingerprint,
        &assets,
        Some(manifest.datasets_fingerprint.as_str()),
    )?;
    Ok(json!({
        "schema": LITD_QUICK_SHARE_SCHEMA,
        "share_id": share_id,
        "package_id": format!("litd.quick_share.{share_id}"),
        "created_by": "agent.issue1843",
        "runtime_id": LITD_RUNTIME_ID,
        "lock_in_policy": LITD_LOCK_IN_POLICY,
        "endpoint": endpoint,
        "host_uri": endpoint.uri(),
        "document": {
            "schema": "litd.lowered_document.v1",
            "document_id": "calyx://world/issue1843-runtime-bind"
        },
        "document_fingerprint": document_fingerprint,
        "play_build_fingerprint": play_build_fingerprint,
        "asset_count": assets.len(),
        "assets": assets,
        "manifest_fingerprint": manifest_fingerprint,
        "runtime": manifest,
    }))
}

#[test]
fn issue1843_substituted_runtime_section_rejected() {
    let root = test_root("runtime-substitution-1843");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("runtime.litdgame.json");

    // Two individually self-consistent runtime manifests with DIFFERENT datasets fingerprints.
    let manifest_a = runtime_manifest(0x1843, 50);
    let manifest_b = runtime_manifest(0x9999, 77);
    assert_ne!(
        manifest_a.datasets_fingerprint, manifest_b.datasets_fingerprint,
        "the alternative must be a genuinely different (yet valid) manifest"
    );

    // Package carries runtime A; the envelope fingerprint is bound to A's datasets fp.
    let mut package = runtime_package_value("issue1843runtimebind", &manifest_a).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&package).unwrap()).unwrap();
    let readback = load_runtime_locked_package(&path, "litd-game-cli").unwrap();
    assert_eq!(
        readback.runtime.as_ref().unwrap().datasets_fingerprint,
        manifest_a.datasets_fingerprint,
        "the happy package loads and carries runtime A"
    );

    // Swap the WHOLE runtime section for the self-consistent alternative B, keeping A's
    // envelope manifest_fingerprint. Before the #1843 binding this loaded silently; now the
    // envelope commits to the runtime datasets fingerprint, so load rejects loud and typed.
    package["runtime"] = serde_json::to_value(&manifest_b).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&package).unwrap()).unwrap();
    let error = load_runtime_locked_package(&path, "litd-game-cli")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("manifest_fingerprint"),
        "a substituted self-consistent runtime section must fail the envelope binding, got: {error}"
    );
    println!("issue1843 substitution rejected error={error}");
}

fn test_root(label: &str) -> PathBuf {
    begin_issue_run("issue1165_game_package")
        .unwrap()
        .artifact_dir(label)
        .unwrap()
}
