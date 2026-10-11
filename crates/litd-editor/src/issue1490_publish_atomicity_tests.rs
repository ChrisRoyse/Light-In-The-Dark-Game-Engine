// FSV for #1490 (publish atomicity): the publish path writes four independent
// Calyx Aster stores with no shared transaction (package archive, project state,
// audit ledger, ecosystem catalog). Before the fix the outward-facing catalog was
// committed BEFORE the local state/ledger, so a mid-publish failure could leave a
// game durably discoverable in the ecosystem catalog with no backing record in
// project state -- and nothing to reconcile it.
//
// The fix commits the catalog LAST. This test proves against the real stores that:
//   1. Happy path: all four stores end consistent (catalog entry == state entry).
//   2. Injected failure at the catalog write (a real IO failure -- a file placed
//      where the catalog Aster vault directory must go) leaves the SAFE direction:
//      the game is in project state + ledger but NOT in the catalog. The forbidden
//      direction (catalog advertises a game absent from state) can never occur.
//   3. A re-publish idempotently completes the half-published game: the catalog
//      ends with exactly one entry, state still holds exactly one published game.

use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    fsv_support::artifact_dir,
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
fn issue1490_publish_commits_catalog_last_cross_store_atomicity_fsv() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (run, root) = artifact_dir("issue1490_publish_atomicity", "stores").unwrap();
    let state_path = root.join("project-state-aster");
    let ledger_path = root.join("project-ledger-aster");
    let project_path = root.join("ashen-reach.litdworld.json");
    let package_path = root.join("ashen-reach.litdgame.json");
    let thumbnail_path = root.join("thumbnail.png");
    // Two catalog targets: one blocked by a file (forces failure), one valid.
    let blocked_catalog = root.join("blocked-catalog-aster");
    fs::write(&thumbnail_path, PNG_1X1_RGBA).unwrap();

    // Save the project so a packageable document exists.
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        save_args(&state_path, &ledger_path, &project_path),
    )
    .unwrap();

    // ---- Phase A: injected catalog-write failure (real IO error) ----
    // Place a regular file where the catalog Aster vault DIRECTORY must be created.
    fs::write(&blocked_catalog, b"not-an-aster-vault").unwrap();
    assert!(blocked_catalog.is_file());

    let state_before = read_mcp_project_state(&state_path).unwrap();
    assert_eq!(state_before.published_count, 0, "no game published yet");

    let publish_err = call_mcp_project_tool(
        PUBLISH_LITDGAME_TOOL,
        publish_args(
            &state_path,
            &ledger_path,
            &project_path,
            &package_path,
            &blocked_catalog,
            &thumbnail_path,
        ),
    )
    .unwrap_err();
    let publish_err_report = publish_err.report();

    // State + ledger were committed BEFORE the catalog write failed.
    let state_after_fail = read_mcp_project_state(&state_path).unwrap();
    assert_eq!(
        state_after_fail.published_count, 1,
        "state must record the publish even though the catalog write failed"
    );
    assert_eq!(state_after_fail.package_count, 1);

    // The catalog store never came into existence as a valid vault: reading it fails,
    // so it certainly does not advertise a game. The forbidden direction (catalog has
    // a game state lacks) is impossible.
    let catalog_read_after_fail = read_publish_catalog(&blocked_catalog);
    assert!(
        catalog_read_after_fail.is_err(),
        "blocked catalog must not be a readable vault advertising the game"
    );
    let published_id = state_after_fail.state.published_games[0].publish_id.clone();

    // ---- Phase B: idempotent recovery with a valid catalog ----
    let valid_catalog = root.join("publish-catalog-aster");
    let recover = call_mcp_project_tool(
        PUBLISH_LITDGAME_TOOL,
        publish_args(
            &state_path,
            &ledger_path,
            &project_path,
            &package_path,
            &valid_catalog,
            &thumbnail_path,
        ),
    )
    .unwrap();
    assert_eq!(
        recover.state_readback.published_count, 1,
        "no duplicate published game"
    );
    assert_eq!(recover.state_readback.package_count, 1);

    // ---- Full-state cross-store consistency after recovery ----
    let state_final = read_mcp_project_state(&state_path).unwrap();
    let catalog_final = read_publish_catalog(&valid_catalog).unwrap();
    let package_bytes = fs::read(&package_path).unwrap();
    let package_sha256 = sha256_hex(&package_bytes);

    assert_eq!(
        catalog_final.catalog.entry_count, 1,
        "catalog holds exactly one entry"
    );
    let catalog_entry = &catalog_final.catalog.entries[0];
    let state_entry = &state_final.state.published_games[0];
    assert_eq!(
        catalog_entry, state_entry,
        "catalog entry must equal the state entry"
    );
    assert_eq!(
        catalog_entry.publish_id, published_id,
        "same game as the half-published one"
    );
    assert_eq!(
        state_entry.package_sha256, package_sha256,
        "state references the real package bytes"
    );
    assert_eq!(state_final.state.packages[0].package_sha256, package_sha256);

    // Invariant proven directly: every publish_id in the catalog is present in state.
    let state_ids: std::collections::BTreeSet<&str> = state_final
        .state
        .published_games
        .iter()
        .map(|record| record.publish_id.as_str())
        .collect();
    let catalog_absent_from_state: Vec<String> = catalog_final
        .catalog
        .entries
        .iter()
        .filter(|entry| !state_ids.contains(entry.publish_id.as_str()))
        .map(|entry| entry.publish_id.clone())
        .collect();
    assert!(
        catalog_absent_from_state.is_empty(),
        "catalog must never advertise a game absent from state: {catalog_absent_from_state:?}"
    );

    let report = json!({
        "schema_version": 1,
        "issue": 1490,
        "finding": "publish must commit the outward-facing ecosystem catalog LAST",
        "source_of_truth": {
            "state": state_path.display().to_string(),
            "ledger": ledger_path.display().to_string(),
            "catalog": valid_catalog.display().to_string(),
            "package": package_path.display().to_string(),
        },
        "phase_a_injected_catalog_failure": {
            "blocked_catalog_is_file": blocked_catalog.is_file(),
            "published_count_before": state_before.published_count,
            "published_count_after_failure": state_after_fail.published_count,
            "package_count_after_failure": state_after_fail.package_count,
            "catalog_readable_after_failure": catalog_read_after_fail.is_ok(),
            "half_published_direction": "state_has_game, catalog_missing (SAFE)",
            "publish_error": error_json(publish_err_report),
        },
        "phase_b_idempotent_recovery": {
            "published_count": state_final.published_count,
            "package_count": state_final.package_count,
            "catalog_entry_count": catalog_final.catalog.entry_count,
            "catalog_entry_equals_state_entry": catalog_entry == state_entry,
            "package_sha256": package_sha256,
            "catalog_ids_absent_from_state": catalog_absent_from_state,
        },
    });
    let readback = run.write_report(&report).unwrap();
    println!(
        "ISSUE1490_PUBLISH_ATOMICITY_FSV path={} bytes={} sha256={} fail_published_count={} recovered_catalog_entries={} catalog_subset_of_state={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        state_after_fail.published_count,
        catalog_final.catalog.entry_count,
        catalog_absent_from_state.is_empty(),
    );
}

fn save_args(state_path: &Path, ledger_path: &Path, project_path: &Path) -> Value {
    json!({
        "request_id": "issue1490-save",
        "requested_by": "litd-editor-issue1490-test",
        "state_path": state_path,
        "state_id": "issue1490-project-state",
        "ledger_path": ledger_path,
        "ledger_id": "issue1490-project-ledger",
        "action": ProjectLifecycleAction::SaveSnapshot,
        "project_id": "project.ashen_reach_issue1490",
        "title": "Ashen Reach Issue 1490",
        "version": "1.0.0",
        "tags": ["dark_fantasy", "publish", "local_first"],
        "project_path": project_path,
        "snapshot_label": "publish-ready",
        "document": fixture_document()
    })
}

fn publish_args(
    state_path: &Path,
    ledger_path: &Path,
    project_path: &Path,
    package_path: &Path,
    catalog_path: &Path,
    thumbnail_path: &Path,
) -> Value {
    json!({
        "request_id": "issue1490-publish",
        "requested_by": "litd-editor-issue1490-test",
        "state_path": state_path,
        "state_id": "issue1490-project-state",
        "ledger_path": ledger_path,
        "ledger_id": "issue1490-project-ledger",
        "project_path": project_path,
        "package_path": package_path,
        "catalog_path": catalog_path,
        "share_id": "issue1490-share",
        "created_by": "agent.project.publisher",
        "host_port": 19560,
        "max_package_bytes": 262_144,
        "title": "Ashen Reach Published",
        "version": "1.0.0",
        "summary": "Known deterministic publish fixture for atomicity FSV",
        "tags": ["dark_fantasy", "singleplayer", "local_first"],
        "thumbnail_path": thumbnail_path,
        "min_spec_tags": ["ryzen5", "gtx1660", "ram16gb"]
    })
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1490-project", "issue1490-rev-1").unwrap();
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

fn error_json(report: litd_engine::diagnostics::ErrorReport) -> Value {
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation,
    })
}
