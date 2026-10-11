use std::{fs, path::Path};

use litd_engine::math::{Transform, Vec3};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    fsv_support,
    history::{EditorDocument, EditorObject},
    mcp_project_tools::{
        MCP_LITDWORLD_SOURCE_OF_TRUTH, NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction,
        call_mcp_project_tool, inspect_litdworld_archive, read_litdworld_project,
    },
};

#[test]
fn issue1316_litdworld_paths_are_verified_archives() {
    let (run, root) =
        fsv_support::artifact_dir("issue1316_litdworld_archive", "manual/workspace").unwrap();
    let state_path = root.join("project-state.json");
    let project_path = root.join("ashen-reach.litdworld");
    let document = fixture_document();
    let expected_document_fingerprint = crate::history::stable_fingerprint(&document).unwrap();

    let before = file_state(&state_path, &project_path);
    println!("issue1316 happy before {}", before);
    let saved = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        lifecycle_args(
            &state_path,
            &root,
            &project_path,
            ProjectLifecycleAction::SaveSnapshot,
            Some(document.clone()),
            "issue1316-save",
        ),
    )
    .unwrap();
    let opened = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        lifecycle_args(
            &state_path,
            &root,
            &project_path,
            ProjectLifecycleAction::Open,
            None,
            "issue1316-open",
        ),
    )
    .unwrap();
    let read_project = read_litdworld_project(&project_path).unwrap();
    let archive = inspect_litdworld_archive(&project_path).unwrap();
    let after = file_state(&state_path, &project_path);
    println!(
        "issue1316 happy after {} project_source={}",
        after, read_project.source_of_truth
    );

    assert_eq!(read_project.source_of_truth, MCP_LITDWORLD_SOURCE_OF_TRUTH);
    assert_eq!(
        read_project.document_fingerprint,
        expected_document_fingerprint
    );
    assert_eq!(archive.zip_magic, "PK0304");
    assert_eq!(archive.zip_entries, vec!["manifest.json", "project.json"]);
    assert_eq!(archive.manifest.schema, "litd.worldarchive.v1");
    assert_eq!(archive.manifest.archive_format, "zip");
    assert_eq!(archive.manifest.compression, "deflate");
    assert_eq!(
        archive.manifest.document_fingerprint,
        expected_document_fingerprint
    );
    assert!(!archive.manifest.engine_version.trim().is_empty());
    assert!(!archive.manifest.min_engine_version.trim().is_empty());
    assert_eq!(saved.state_readback.project_count, 1);
    assert_eq!(opened.state_readback.project_count, 1);
    assert!(project_path.is_file());
    assert!(!project_path.is_dir());

    let plain_json_edge = run_edge(
        &root,
        "plain-json-dot-litdworld",
        |path| fs::write(path, b"{\"schema\":\"legacy-litdworld-json\"}").unwrap(),
        "ZIP",
    );
    let missing_project_edge = run_edge(
        &root,
        "missing-litdworld-archive",
        |_path| {},
        "archive.read",
    );

    let report = json!({
        "issue": 1316,
        "source_of_truth": "physical .litdworld ZIP archive plus Calyx Aster MCP project state readback",
        "happy_path": {
            "before": before,
            "after": after,
            "project_path": project_path,
            "project_path_is_file": project_path.is_file(),
            "zip_magic": archive.zip_magic,
            "zip_entries": archive.zip_entries,
            "manifest": archive.manifest,
            "project_source_of_truth": read_project.source_of_truth,
            "saved_project_count": saved.state_readback.project_count,
            "opened_project_count": opened.state_readback.project_count,
            "document_fingerprint": read_project.document_fingerprint,
        },
        "edges": [
            plain_json_edge,
            missing_project_edge,
        ],
    });
    let readback = run.write_report(&report).unwrap();
    println!(
        "issue1316 manual_fsv_report path={} sha256={} manifest={}",
        readback.path.display(),
        readback.sha256,
        readback.latest_manifest_path.display()
    );
    assert_eq!(readback.json["happy_path"]["project_path_is_file"], true);
    assert_eq!(readback.json["happy_path"]["zip_magic"], "PK0304");
    assert_eq!(readback.manifest["report_sha256"], readback.sha256);
}

fn run_edge(
    root: &Path,
    label: &str,
    write_file: impl FnOnce(&Path),
    expected_error: &str,
) -> Value {
    let edge_dir = root.join(label);
    fs::create_dir_all(&edge_dir).unwrap();
    let project_path = edge_dir.join(format!("{label}.litdworld"));
    let state_path = edge_dir.join("project-state.json");
    write_file(&project_path);
    let before = file_state(&state_path, &project_path);
    let error = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        lifecycle_args(
            &state_path,
            &edge_dir,
            &project_path,
            ProjectLifecycleAction::Open,
            None,
            &format!("issue1316-{label}"),
        ),
    )
    .unwrap_err()
    .to_string();
    let after = file_state(&state_path, &project_path);
    println!("issue1316 edge={label} before={before} after={after} error={error}");
    assert!(error.contains(expected_error), "{error}");
    assert_eq!(before, after);
    json!({
        "label": label,
        "before": before,
        "after": after,
        "error": error,
        "expected_error": expected_error,
    })
}

fn lifecycle_args(
    state_path: &Path,
    root: &Path,
    project_path: &Path,
    action: ProjectLifecycleAction,
    document: Option<EditorDocument>,
    request_id: &str,
) -> Value {
    json!({
        "request_id": request_id,
        "requested_by": "litd-editor-issue1316-test",
        "state_path": state_path,
        "state_id": "issue1316-project-state",
        "ledger_path": root.join(format!("{request_id}-ledger.json")),
        "ledger_id": format!("{request_id}-ledger"),
        "action": action,
        "project_id": "project.ashen_reach_issue1316",
        "title": "Ashen Reach Issue 1316",
        "version": "1.0.0",
        "tags": ["dark_fantasy", "archive", "local_first"],
        "project_path": project_path,
        "snapshot_label": "archive-roundtrip",
        "document": document,
    })
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1316-project", "issue1316-rev-1").unwrap();
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
                "unit_archive_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(4.0, 0.0, -2.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    document
}

fn file_state(state_path: &Path, project_path: &Path) -> Value {
    let project_bytes = fs::read(project_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": state_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "project_exists": project_path.exists(),
        "project_bytes": project_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "project_sha256": project_bytes.as_ref().map(|bytes| sha256_hex(bytes)).unwrap_or_default(),
        "first_four_hex": project_bytes.as_ref().map(|bytes| {
            bytes.iter().take(4).map(|byte| format!("{byte:02x}")).collect::<String>()
        }).unwrap_or_default(),
    })
}
