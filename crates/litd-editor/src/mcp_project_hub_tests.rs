use litd_hash::sha256_hex;
use std::{fs, path::Path, path::PathBuf};

use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_project_tools::{
        DELIVERY_LITDGAME_TOOL, HUB_LAUNCHER_TOOL, NEW_OPEN_SAVE_TOOL, PUBLISH_LITDGAME_TOOL,
        ProjectLifecycleAction, call_mcp_project_tool, read_hub_launcher_state,
        read_mcp_project_state, read_publish_catalog,
    },
};
use litd_engine::math::{Transform, Vec3};

const PNG_1X1_RGBA: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9, 251, 3,
    253, 167, 197, 129, 218, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[test]
fn issue1157_hub_launcher_browse_search_launch_fsv() {
    let paths = prepare_delivered_fixture("unit");
    let content_address = published_content_address(&paths.catalog_path);

    let before = hub_summary(&paths);
    println!(
        "issue1157 hub before {}",
        serde_json::to_string(&before).unwrap()
    );

    let readback = call_mcp_project_tool(
        HUB_LAUNCHER_TOOL,
        hub_args(
            &paths,
            Some("Delivery"),
            &["singleplayer"],
            true,
            Some(&content_address),
        ),
    )
    .unwrap();
    assert_eq!(readback.state_readback.published_count, 1);

    let hub = read_hub_launcher_state(&paths.hub_state_path).unwrap();
    let card = &hub.state.cards[0];
    let launch = &hub.state.launches[0];
    let installed_path = PathBuf::from(card.installed_package_path.as_ref().unwrap());
    let installed_bytes = fs::read(&installed_path).unwrap();
    assert_eq!(hub.state.result_count, 1);
    assert_eq!(hub.state.launch_count, 1);
    assert!(card.installed);
    assert!(card.launchable);
    assert_eq!(card.content_address, content_address);
    assert_eq!(
        card.installed_package_sha256.as_deref(),
        Some(card.package_sha256.as_str())
    );
    assert_eq!(sha256_hex(&installed_bytes), card.package_sha256);
    assert_eq!(launch.runtime_report.package_sha256, card.package_sha256);
    assert!(launch.runtime_report.playable_on_litd_runtime);

    let after = hub_summary(&paths);
    println!(
        "issue1157 hub after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1157_hub_launcher_edges_fail_closed_without_state_drift() {
    let paths = prepare_delivered_fixture("edges");
    let content_address = published_content_address(&paths.catalog_path);

    let sort_before = hub_summary(&paths);
    let mut bad_sort_args = hub_args(&paths, Some("Delivery"), &["singleplayer"], true, None);
    bad_sort_args["sort"] = json!("popularity_desc");
    let sort_error = call_mcp_project_tool(HUB_LAUNCHER_TOOL, bad_sort_args)
        .unwrap_err()
        .to_string();
    let sort_after = hub_summary(&paths);
    assert_eq!(sort_before, sort_after);
    println!(
        "issue1157 edge unsupported_sort before={} after={} error={}",
        sort_before, sort_after, sort_error
    );

    let wrong_address = format!("litdgame.sha256.{}", "1".repeat(64));
    let wrong_before = hub_summary(&paths);
    let wrong_error = call_mcp_project_tool(
        HUB_LAUNCHER_TOOL,
        hub_args(
            &paths,
            Some("Delivery"),
            &["singleplayer"],
            true,
            Some(&wrong_address),
        ),
    )
    .unwrap_err()
    .to_string();
    let wrong_after = hub_summary(&paths);
    assert_eq!(wrong_before, wrong_after);
    println!(
        "issue1157 edge wrong_launch_address before={} after={} error={}",
        wrong_before, wrong_after, wrong_error
    );

    let no_result_before = hub_summary(&paths);
    call_mcp_project_tool(
        HUB_LAUNCHER_TOOL,
        hub_args(&paths, Some("NoSuchGame"), &[], false, None),
    )
    .unwrap();
    let no_result_after = hub_summary(&paths);
    assert_eq!(no_result_after["result_count"], json!(0));
    assert_eq!(no_result_after["launch_count"], json!(0));
    println!(
        "issue1157 edge no_results before={} after={}",
        no_result_before, no_result_after
    );

    call_mcp_project_tool(
        HUB_LAUNCHER_TOOL,
        hub_args(
            &paths,
            Some("Delivery"),
            &["singleplayer"],
            true,
            Some(&content_address),
        ),
    )
    .unwrap();
    let hub = read_hub_launcher_state(&paths.hub_state_path).unwrap();
    let installed_path = hub.state.cards[0].installed_package_path.as_ref().unwrap();
    fs::write(installed_path, b"tampered-installed-litdgame").unwrap();
    let tampered_before = hub_summary(&paths);
    let tampered_error = call_mcp_project_tool(
        HUB_LAUNCHER_TOOL,
        hub_args(
            &paths,
            Some("Delivery"),
            &["singleplayer"],
            true,
            Some(&content_address),
        ),
    )
    .unwrap_err()
    .to_string();
    let tampered_after = hub_summary(&paths);
    assert_eq!(tampered_before, tampered_after);
    println!(
        "issue1157 edge tampered_install before={} after={} error={}",
        tampered_before, tampered_after, tampered_error
    );
}

fn prepare_delivered_fixture(label: &str) -> FixturePaths {
    // Confine delivery paths to the workspace root (#1487); target/ is within it.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let root = test_root(label);
    clean_dir(&root);
    let paths = FixturePaths {
        root: root.clone(),
        state_path: root.join("project-state.json"),
        project_path: root.join("ashen-reach.litdworld.json"),
        package_path: root.join("ashen-reach.litdgame.json"),
        catalog_path: root.join("publish-catalog.json"),
        thumbnail_path: root.join("thumbnail.png"),
        delivery_manifest_path: root.join("delivery-store.json"),
        blob_root_path: root.join("content-store"),
        install_manifest_path: root.join("install-manifest.json"),
        install_root_path: root.join("installed"),
        hub_state_path: root.join("hub-launcher-state.json"),
    };
    fs::write(&paths.thumbnail_path, PNG_1X1_RGBA).unwrap();
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        save_args(
            &paths.state_path,
            &paths.root,
            &paths.project_path,
            fixture_document(),
        ),
    )
    .unwrap();
    call_mcp_project_tool(PUBLISH_LITDGAME_TOOL, publish_args(&paths)).unwrap();
    let content_address = published_content_address(&paths.catalog_path);
    call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address),
    )
    .unwrap();
    paths
}

fn hub_args(
    paths: &FixturePaths,
    query: Option<&str>,
    required_tags: &[&str],
    installed_only: bool,
    launch_content_address: Option<&str>,
) -> Value {
    json!({
        "request_id": "issue1157-hub",
        "requested_by": "litd-editor-issue1157-test",
        "state_path": paths.state_path,
        "state_id": "issue1157-project-state",
        "ledger_path": paths.root.join("hub-ledger.json"),
        "ledger_id": "issue1157-hub-ledger",
        "catalog_path": paths.catalog_path,
        "install_manifest_path": paths.install_manifest_path,
        "hub_state_path": paths.hub_state_path,
        "hub_id": "issue1157-local-hub",
        "query": query,
        "required_tags": required_tags,
        "installed_only": installed_only,
        "sort": "title_asc",
        "launch_content_address": launch_content_address,
        "launched_by": "agent.hub.launcher"
    })
}

fn delivery_args(paths: &FixturePaths, content_address: &str) -> Value {
    json!({
        "request_id": "issue1157-deliver",
        "requested_by": "litd-editor-issue1157-test",
        "state_path": paths.state_path,
        "state_id": "issue1157-project-state",
        "ledger_path": paths.root.join("delivery-ledger.json"),
        "ledger_id": "issue1157-delivery-ledger",
        "catalog_path": paths.catalog_path,
        "delivery_manifest_path": paths.delivery_manifest_path,
        "delivery_store_id": "issue1157-local-store",
        "blob_root_path": paths.blob_root_path,
        "install_manifest_path": paths.install_manifest_path,
        "install_id": "issue1157-install",
        "install_root_path": paths.install_root_path,
        "content_address": content_address,
        "delivered_by": "agent.delivery.store",
        "installed_by": "agent.delivery.install"
    })
}

fn save_args(
    state_path: &Path,
    root: &Path,
    project_path: &Path,
    document: EditorDocument,
) -> Value {
    json!({
        "request_id": "issue1157-save",
        "requested_by": "litd-editor-issue1157-test",
        "state_path": state_path,
        "state_id": "issue1157-project-state",
        "ledger_path": root.join("project-ledger.json"),
        "ledger_id": "issue1157-project-ledger",
        "action": ProjectLifecycleAction::SaveSnapshot,
        "project_id": "project.ashen_reach_issue1157",
        "title": "Ashen Reach Issue 1157",
        "version": "1.2.3",
        "tags": ["dark_fantasy", "hub", "local_first"],
        "project_path": project_path,
        "snapshot_label": "hub-ready",
        "document": document
    })
}

fn publish_args(paths: &FixturePaths) -> Value {
    json!({
        "request_id": "issue1157-publish",
        "requested_by": "litd-editor-issue1157-test",
        "state_path": paths.state_path,
        "state_id": "issue1157-project-state",
        "ledger_path": paths.root.join("publish-ledger.json"),
        "ledger_id": "issue1157-publish-ledger",
        "project_path": paths.project_path,
        "package_path": paths.package_path,
        "catalog_path": paths.catalog_path,
        "share_id": "issue1157-share",
        "created_by": "agent.project.publisher",
        "host_port": 19570,
        "max_package_bytes": 262144,
        "title": "Ashen Reach Delivery Hub",
        "version": "1.2.3",
        "summary": "Known deterministic hub launcher fixture",
        "tags": ["dark_fantasy", "singleplayer", "local_first"],
        "thumbnail_path": paths.thumbnail_path,
        "min_spec_tags": ["ryzen5", "gtx1660", "ram16gb"]
    })
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1157-project", "issue1157-rev-1").unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_hub_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(8.0, 0.0, -10.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    let record = AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .unwrap();
    assign_vault_model_to_unit(&mut document, "unit_hub_hero", &record).unwrap();
    document
}

fn hub_summary(paths: &FixturePaths) -> Value {
    let state = read_mcp_project_state(&paths.state_path).ok();
    let hub = read_hub_launcher_state(&paths.hub_state_path).ok();
    let installed_path = hub
        .as_ref()
        .and_then(|readback| readback.state.cards.first())
        .and_then(|card| card.installed_package_path.as_ref())
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.install_root_path.join("missing"));
    json!({
        "state_exists": paths.state_path.exists(),
        "state_update_count": state.as_ref().map(|readback| readback.state.update_count).unwrap_or(0),
        "hub_state_exists": paths.hub_state_path.exists(),
        "hub_state_bytes": paths.hub_state_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "result_count": hub.as_ref().map(|readback| readback.state.result_count).unwrap_or(0),
        "launch_count": hub.as_ref().map(|readback| readback.state.launch_count).unwrap_or(0),
        "installed_exists": installed_path.exists(),
        "installed_bytes": installed_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "installed_sha256": fs::read(&installed_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default()
    })
}

fn published_content_address(catalog_path: &Path) -> String {
    read_publish_catalog(catalog_path).unwrap().catalog.entries[0]
        .content_address
        .clone()
}

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1157_hub_launcher", label)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}

struct FixturePaths {
    root: PathBuf,
    state_path: PathBuf,
    project_path: PathBuf,
    package_path: PathBuf,
    catalog_path: PathBuf,
    thumbnail_path: PathBuf,
    delivery_manifest_path: PathBuf,
    blob_root_path: PathBuf,
    install_manifest_path: PathBuf,
    install_root_path: PathBuf,
    hub_state_path: PathBuf,
}
