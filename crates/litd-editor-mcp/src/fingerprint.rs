use serde::Serialize;

use crate::EditorMcpError;

pub fn stable_fingerprint<T: Serialize>(value: &T) -> Result<String, EditorMcpError> {
    litd_hash::stable_fingerprint(value).map_err(|error| EditorMcpError::SerializationFailed {
        format: "ron",
        reason: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use litd_engine::game_package::{
        LITD_LOCK_IN_POLICY, LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimePackageAssetRef,
        RuntimePackageEndpoint, RuntimePackageHostMode, compute_runtime_manifest_fingerprint,
        load_runtime_locked_package,
    };
    use serde_json::json;
    use std::fs;

    use super::stable_fingerprint;

    #[test]
    fn issue1314_editor_mcp_and_engine_package_fingerprints_match_on_real_package() {
        let fsv_run = litd_engine::fsv_artifacts::begin_issue_run("issue1314_hash_dedup")
            .expect("create issue1314 FSV run");
        let root = fsv_run.run_dir();
        let package_path = root.join("issue1314-shared-hash.litdgame.json");
        let tampered_path = root.join("issue1314-tampered.litdgame.json");
        let empty_path = root.join("issue1314-empty.litdgame.json");
        let wrong_extension_path = root.join("issue1314-wrong-extension.json");

        let share_id = "issue1314hashdedup";
        let created_by = "agent.issue1314";
        let endpoint = RuntimePackageEndpoint {
            mode: RuntimePackageHostMode::Loopback,
            host: "127.0.0.1".to_string(),
            port: 13140,
            path: format!("/share/{share_id}"),
        };
        let assets = vec![RuntimePackageAssetRef {
            product_id: "litd.asset.issue1314.shared_hash.mesh".to_string(),
            source_relative_path: "assets/issue1314/shared-hash.mesh".to_string(),
            source_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            source_bytes: 4096,
        }];
        let document_fingerprint = "1314docf1314docf";
        let play_build_fingerprint = "1314play1314play";
        let manifest_input = (
            share_id,
            created_by,
            LITD_RUNTIME_ID,
            &endpoint,
            document_fingerprint,
            play_build_fingerprint,
            &assets,
        );

        let mcp = stable_fingerprint(&manifest_input).expect("mcp fingerprint");
        let engine = compute_runtime_manifest_fingerprint(
            share_id,
            created_by,
            LITD_RUNTIME_ID,
            &endpoint,
            document_fingerprint,
            play_build_fingerprint,
            &assets,
        )
        .expect("engine fingerprint");

        assert_eq!(mcp, engine);

        let package = package_value(
            share_id,
            created_by,
            &endpoint,
            &assets,
            document_fingerprint,
            play_build_fingerprint,
            &mcp,
        );
        let package_bytes = serde_json::to_vec_pretty(&package).expect("encode package");
        fs::write(&package_path, &package_bytes).expect("write package");
        println!(
            "issue1314 happy before_load path={} bytes={} mcp={} engine={}",
            package_path.display(),
            package_bytes.len(),
            mcp,
            engine
        );

        let readback =
            load_runtime_locked_package(&package_path, "issue1314-fsv").expect("engine load");
        let disk_bytes = fs::read(&package_path).expect("read package after load");
        assert_eq!(disk_bytes, package_bytes);
        assert_eq!(readback.manifest_fingerprint, mcp);
        assert_eq!(readback.package_bytes, package_bytes.len());
        assert!(readback.manifest_integrity_verified);
        assert!(readback.runtime_lock_verified);
        assert!(readback.playable_on_litd_runtime);

        let mut tampered = package.clone();
        tampered["manifest_fingerprint"] = json!("0000000000000000");
        fs::write(
            &tampered_path,
            serde_json::to_vec_pretty(&tampered).expect("encode tampered"),
        )
        .expect("write tampered package");
        let tampered_before = fs::read(&tampered_path).expect("read tampered before");
        let tampered_error = load_runtime_locked_package(&tampered_path, "issue1314-fsv")
            .expect_err("tampered manifest rejected")
            .to_string();
        let tampered_after = fs::read(&tampered_path).expect("read tampered after");
        assert_eq!(tampered_before, tampered_after);
        assert!(tampered_error.contains("manifest_fingerprint"));

        fs::write(&empty_path, []).expect("write empty package");
        let empty_before = fs::read(&empty_path).expect("read empty before");
        let empty_error = load_runtime_locked_package(&empty_path, "issue1314-fsv")
            .expect_err("empty package rejected")
            .to_string();
        let empty_after = fs::read(&empty_path).expect("read empty after");
        assert_eq!(empty_before, empty_after);
        assert!(empty_error.contains("package.bytes"));

        fs::write(&wrong_extension_path, &package_bytes).expect("write wrong extension package");
        let wrong_extension_before_exists = wrong_extension_path.exists();
        let wrong_extension_error =
            load_runtime_locked_package(&wrong_extension_path, "issue1314-fsv")
                .expect_err("wrong extension rejected")
                .to_string();
        assert!(wrong_extension_path.exists());
        assert!(wrong_extension_error.contains("package_path.extension"));

        let report_path = root.join("manual-fsv-report.json");
        let report = json!({
            "issue": 1314,
            "source_of_truth": {
                "package_path": package_path,
                "report_path": report_path,
                "hash_definition_site": "crates/litd-hash/src/lib.rs"
            },
            "happy_path": {
                "before": {
                    "package_bytes_written": package_bytes.len(),
                    "mcp_manifest_fingerprint": mcp,
                    "engine_manifest_fingerprint": engine
                },
                "after": {
                    "engine_readback_manifest_fingerprint": readback.manifest_fingerprint,
                    "engine_readback_package_sha256": readback.package_sha256,
                    "engine_readback_package_bytes": readback.package_bytes,
                    "manifest_integrity_verified": readback.manifest_integrity_verified,
                    "runtime_lock_verified": readback.runtime_lock_verified,
                    "playable_on_litd_runtime": readback.playable_on_litd_runtime
                }
            },
            "edge_cases": [
                {
                    "name": "tampered_manifest_fingerprint",
                    "before": {"bytes": tampered_before.len()},
                    "after": {"bytes": tampered_after.len(), "state_unchanged": tampered_before == tampered_after, "error": tampered_error}
                },
                {
                    "name": "empty_package",
                    "before": {"bytes": empty_before.len()},
                    "after": {"bytes": empty_after.len(), "state_unchanged": empty_before == empty_after, "error": empty_error}
                },
                {
                    "name": "wrong_extension",
                    "before": {"exists": wrong_extension_before_exists},
                    "after": {"exists": wrong_extension_path.exists(), "error": wrong_extension_error}
                }
            ]
        });
        let report_readback = fsv_run
            .write_json_file(&report_path, &report)
            .expect("write report");
        assert_eq!(report_readback.json["issue"], json!(1314));
        assert!(!report_readback.bytes.is_empty());
        println!(
            "issue1314 happy after_load package={} runtime={} manifest_ok={} sha={} bytes={} report={}",
            readback.package_id,
            readback.runtime_id,
            readback.manifest_integrity_verified,
            readback.package_sha256,
            readback.package_bytes,
            report_readback.path.display()
        );
    }

    fn package_value(
        share_id: &str,
        created_by: &str,
        endpoint: &RuntimePackageEndpoint,
        assets: &[RuntimePackageAssetRef],
        document_fingerprint: &str,
        play_build_fingerprint: &str,
        manifest_fingerprint: &str,
    ) -> serde_json::Value {
        json!({
            "schema": LITD_QUICK_SHARE_SCHEMA,
            "share_id": share_id,
            "package_id": format!("litd.quick_share.{share_id}"),
            "created_by": created_by,
            "runtime_id": LITD_RUNTIME_ID,
            "lock_in_policy": LITD_LOCK_IN_POLICY,
            "endpoint": endpoint,
            "host_uri": endpoint.uri(),
            "document": {
                "schema": "litd.lowered_document.v1",
                "document_id": "calyx://world/issue1314-shared-hash"
            },
            "document_fingerprint": document_fingerprint,
            "play_build_fingerprint": play_build_fingerprint,
            "asset_count": assets.len(),
            "assets": assets,
            "manifest_fingerprint": manifest_fingerprint
        })
    }
}
