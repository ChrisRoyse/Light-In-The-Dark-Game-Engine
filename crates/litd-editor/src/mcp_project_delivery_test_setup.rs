use super::*;

pub(super) fn prepare_published_fixture(label: &str) -> FixturePaths {
    // Confine delivery paths to the working directory (#1487); all fixtures live
    // under target/ which is within it.
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
    call_mcp_project_tool(
        PUBLISH_LITDGAME_TOOL,
        publish_args(&paths, "1.2.3", 262_144),
    )
    .unwrap();
    paths
}

pub(super) fn delivery_args(
    paths: &FixturePaths,
    content_address: &str,
    source_uri: Option<&str>,
) -> Value {
    let mut value = json!({
        "request_id": format!("issue1158-deliver-{content_address}"),
        "requested_by": "litd-editor-issue1158-test",
        "state_path": paths.state_path,
        "state_id": "issue1158-project-state",
        "ledger_path": paths.root.join("delivery-ledger.json"),
        "ledger_id": "issue1158-delivery-ledger",
        "catalog_path": paths.catalog_path,
        "delivery_manifest_path": paths.delivery_manifest_path,
        "delivery_store_id": "issue1158-local-store",
        "blob_root_path": paths.blob_root_path,
        "install_manifest_path": paths.install_manifest_path,
        "install_id": "issue1158-install",
        "install_root_path": paths.install_root_path,
        "content_address": content_address,
        "delivered_by": "agent.delivery.store",
        "installed_by": "agent.delivery.install"
    });
    if let Some(source_uri) = source_uri {
        value["source_uri"] = json!(source_uri);
    }
    value
}

fn save_args(
    state_path: &Path,
    root: &Path,
    project_path: &Path,
    document: EditorDocument,
) -> Value {
    json!({
        "request_id": "issue1158-save",
        "requested_by": "litd-editor-issue1158-test",
        "state_path": state_path,
        "state_id": "issue1158-project-state",
        "ledger_path": root.join("project-ledger.json"),
        "ledger_id": "issue1158-project-ledger",
        "action": ProjectLifecycleAction::SaveSnapshot,
        "project_id": "project.ashen_reach_issue1158",
        "title": "Ashen Reach Issue 1158",
        "version": "1.2.3",
        "tags": ["dark_fantasy", "delivery", "local_first"],
        "project_path": project_path,
        "snapshot_label": "delivery-ready",
        "document": document
    })
}
