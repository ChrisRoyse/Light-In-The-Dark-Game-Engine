use litd_hash::sha256_hex;
use std::{fs, path::Path, path::PathBuf};

use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_project_tools::{
        NEW_OPEN_SAVE_TOOL, PUBLISH_LITDGAME_TOOL, ProjectLifecycleAction, call_mcp_project_tool,
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
fn issue1156_publish_litdgame_source_of_truth_fsv() {
    let root = test_root("unit");
    clean_dir(&root);
    let state_path = root.join("project-state.json");
    let project_path = root.join("ashen-reach.litdworld.json");
    let package_path = root.join("ashen-reach.litdgame.json");
    let catalog_path = root.join("publish-catalog.json");
    let thumbnail_path = root.join("thumbnail.png");
    fs::write(&thumbnail_path, PNG_1X1_RGBA).unwrap();

    let before = publish_summary(&state_path, &package_path, &catalog_path, &thumbnail_path);
    println!(
        "issue1156 publish before {}",
        serde_json::to_string(&before).unwrap()
    );

    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        save_args(&state_path, &root, &project_path, fixture_document()),
    )
    .unwrap();
    let published = call_mcp_project_tool(
        PUBLISH_LITDGAME_TOOL,
        publish_args(PublishArgsArgs {
            state_path: &state_path,
            root: &root,
            project_path: &project_path,
            package_path: &package_path,
            catalog_path: &catalog_path,
            thumbnail_path: &thumbnail_path,
            version: "1.2.3",
            max_package_bytes: 262_144,
        }),
    )
    .unwrap();
    assert_eq!(published.state_readback.package_count, 1);
    assert_eq!(published.state_readback.published_count, 1);

    let state_readback = read_mcp_project_state(&state_path).unwrap();
    let catalog_readback = read_publish_catalog(&catalog_path).unwrap();
    let package_bytes = fs::read(&package_path).unwrap();
    let thumbnail_bytes = fs::read(&thumbnail_path).unwrap();
    let package_sha256 = sha256_hex(&package_bytes);
    let thumbnail_sha256 = sha256_hex(&thumbnail_bytes);
    let package = &state_readback.state.packages[0];
    let entry = &state_readback.state.published_games[0];
    let catalog_entry = &catalog_readback.catalog.entries[0];

    assert_eq!(package.package_sha256, package_sha256);
    assert_eq!(entry.package_sha256, package_sha256);
    assert_eq!(
        entry.content_address,
        format!("litdgame.sha256.{package_sha256}")
    );
    assert_eq!(entry.thumbnail_sha256, thumbnail_sha256);
    assert_eq!(entry.thumbnail_media_type, "image/png");
    assert_eq!(entry.min_spec_tags, vec!["ryzen5", "gtx1660", "ram16gb"]);
    assert_eq!(catalog_entry, entry);
    assert!(package.runtime_lock_verified);

    let after = publish_summary(&state_path, &package_path, &catalog_path, &thumbnail_path);
    println!(
        "issue1156 publish after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1156_publish_edges_fail_closed_without_partial_catalog_state() {
    let root = test_root("edges");
    clean_dir(&root);
    let state_path = root.join("project-state.json");
    let project_path = root.join("ashen-reach.litdworld.json");
    let good_thumbnail = root.join("good-thumbnail.png");
    fs::write(&good_thumbnail, PNG_1X1_RGBA).unwrap();

    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        save_args(&state_path, &root, &project_path, fixture_document()),
    )
    .unwrap();

    let empty_version_package = root.join("empty-version.litdgame.json");
    let empty_version_catalog = root.join("empty-version-catalog.json");
    let empty_before = publish_summary(
        &state_path,
        &empty_version_package,
        &empty_version_catalog,
        &good_thumbnail,
    );
    let empty_error = call_mcp_project_tool(
        PUBLISH_LITDGAME_TOOL,
        publish_args(PublishArgsArgs {
            state_path: &state_path,
            root: &root,
            project_path: &project_path,
            package_path: &empty_version_package,
            catalog_path: &empty_version_catalog,
            thumbnail_path: &good_thumbnail,
            version: "",
            max_package_bytes: 262_144,
        }),
    )
    .unwrap_err()
    .to_string();
    let empty_after = publish_summary(
        &state_path,
        &empty_version_package,
        &empty_version_catalog,
        &good_thumbnail,
    );
    assert_eq!(empty_before, empty_after);
    println!(
        "issue1156 edge empty_version before={} after={} error={}",
        empty_before, empty_after, empty_error
    );

    let missing_thumb = root.join("missing-thumbnail.png");
    let missing_package = root.join("missing-thumb.litdgame.json");
    let missing_catalog = root.join("missing-thumb-catalog.json");
    let missing_before = publish_summary(
        &state_path,
        &missing_package,
        &missing_catalog,
        &missing_thumb,
    );
    let missing_error = call_mcp_project_tool(
        PUBLISH_LITDGAME_TOOL,
        publish_args(PublishArgsArgs {
            state_path: &state_path,
            root: &root,
            project_path: &project_path,
            package_path: &missing_package,
            catalog_path: &missing_catalog,
            thumbnail_path: &missing_thumb,
            version: "1.2.3",
            max_package_bytes: 262_144,
        }),
    )
    .unwrap_err()
    .to_string();
    let missing_after = publish_summary(
        &state_path,
        &missing_package,
        &missing_catalog,
        &missing_thumb,
    );
    assert_eq!(missing_before, missing_after);
    println!(
        "issue1156 edge missing_thumbnail before={} after={} error={}",
        missing_before, missing_after, missing_error
    );

    let bad_thumbnail = root.join("bad-thumbnail.png");
    fs::write(&bad_thumbnail, b"not-a-real-png").unwrap();
    let bad_package = root.join("bad-thumb.litdgame.json");
    let bad_catalog = root.join("bad-thumb-catalog.json");
    let bad_before = publish_summary(&state_path, &bad_package, &bad_catalog, &bad_thumbnail);
    let bad_error = call_mcp_project_tool(
        PUBLISH_LITDGAME_TOOL,
        publish_args(PublishArgsArgs {
            state_path: &state_path,
            root: &root,
            project_path: &project_path,
            package_path: &bad_package,
            catalog_path: &bad_catalog,
            thumbnail_path: &bad_thumbnail,
            version: "1.2.3",
            max_package_bytes: 262_144,
        }),
    )
    .unwrap_err()
    .to_string();
    let bad_after = publish_summary(&state_path, &bad_package, &bad_catalog, &bad_thumbnail);
    assert_eq!(bad_before, bad_after);
    println!(
        "issue1156 edge bad_thumbnail before={} after={} error={}",
        bad_before, bad_after, bad_error
    );

    let tiny_package = root.join("tiny.litdgame.json");
    let tiny_catalog = root.join("tiny-catalog.json");
    let tiny_before = publish_summary(&state_path, &tiny_package, &tiny_catalog, &good_thumbnail);
    let tiny_error = call_mcp_project_tool(
        PUBLISH_LITDGAME_TOOL,
        publish_args(PublishArgsArgs {
            state_path: &state_path,
            root: &root,
            project_path: &project_path,
            package_path: &tiny_package,
            catalog_path: &tiny_catalog,
            thumbnail_path: &good_thumbnail,
            version: "1.2.3",
            max_package_bytes: 256,
        }),
    )
    .unwrap_err()
    .to_string();
    let tiny_after = publish_summary(&state_path, &tiny_package, &tiny_catalog, &good_thumbnail);
    assert_eq!(tiny_before, tiny_after);
    println!(
        "issue1156 edge tiny_package before={} after={} error={}",
        tiny_before, tiny_after, tiny_error
    );
}

fn save_args(
    state_path: &Path,
    root: &Path,
    project_path: &Path,
    document: EditorDocument,
) -> Value {
    json!({
        "request_id": "issue1156-save",
        "requested_by": "litd-editor-issue1156-test",
        "state_path": state_path,
        "state_id": "issue1156-project-state",
        "ledger_path": root.join("project-ledger.json"),
        "ledger_id": "issue1156-project-ledger",
        "action": ProjectLifecycleAction::SaveSnapshot,
        "project_id": "project.ashen_reach_issue1156",
        "title": "Ashen Reach Issue 1156",
        "version": "1.2.3",
        "tags": ["dark_fantasy", "publish", "local_first"],
        "project_path": project_path,
        "snapshot_label": "publish-ready",
        "document": document
    })
}

struct PublishArgsArgs<'a> {
    state_path: &'a Path,
    root: &'a Path,
    project_path: &'a Path,
    package_path: &'a Path,
    catalog_path: &'a Path,
    thumbnail_path: &'a Path,
    version: &'a str,
    max_package_bytes: usize,
}

fn publish_args(args: PublishArgsArgs<'_>) -> Value {
    let PublishArgsArgs {
        state_path,
        root,
        project_path,
        package_path,
        catalog_path,
        thumbnail_path,
        version,
        max_package_bytes,
    } = args;
    json!({
        "request_id": format!("issue1156-publish-{version}-{max_package_bytes}"),
        "requested_by": "litd-editor-issue1156-test",
        "state_path": state_path,
        "state_id": "issue1156-project-state",
        "ledger_path": root.join(format!("publish-ledger-{version}-{max_package_bytes}.json")),
        "ledger_id": format!("issue1156-publish-ledger-{version}-{max_package_bytes}"),
        "project_path": project_path,
        "package_path": package_path,
        "catalog_path": catalog_path,
        "share_id": "issue1156-share",
        "created_by": "agent.project.publisher",
        "host_port": 19560,
        "max_package_bytes": max_package_bytes,
        "title": "Ashen Reach Published",
        "version": version,
        "summary": "Known deterministic publish fixture",
        "tags": ["dark_fantasy", "singleplayer", "local_first"],
        "thumbnail_path": thumbnail_path,
        "min_spec_tags": ["ryzen5", "gtx1660", "ram16gb"]
    })
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1156-project", "issue1156-rev-1").unwrap();
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
                "unit_publish_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(4.0, 0.0, -6.0)),
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
    assign_vault_model_to_unit(&mut document, "unit_publish_hero", &record).unwrap();
    document
}

fn publish_summary(
    state_path: &Path,
    package_path: &Path,
    catalog_path: &Path,
    thumbnail_path: &Path,
) -> Value {
    let state = read_mcp_project_state(state_path).ok();
    let catalog = read_publish_catalog(catalog_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": state_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "package_file_exists": package_path.exists(),
        "package_file_bytes": package_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "catalog_file_exists": catalog_path.exists(),
        "catalog_file_bytes": catalog_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "thumbnail_file_exists": thumbnail_path.exists(),
        "thumbnail_file_bytes": thumbnail_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "package_count": state.as_ref().map(|readback| readback.package_count).unwrap_or(0),
        "published_count": state.as_ref().map(|readback| readback.published_count).unwrap_or(0),
        "catalog_entry_count": catalog.as_ref().map(|readback| readback.catalog.entry_count).unwrap_or(0)
    })
}

fn test_root(label: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1156_publish_litdgame", label)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
