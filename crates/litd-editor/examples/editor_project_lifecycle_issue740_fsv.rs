use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, stable_fingerprint},
    mcp_project_tools::{
        LitdWorldArchiveManifest, NEW_OPEN_SAVE_TOOL, PACKAGE_LITDGAME_TOOL,
        ProjectLifecycleAction, READ_PROJECT_TOOL, call_mcp_project_tool,
        inspect_litdworld_archive, read_litdworld_project, read_mcp_project_state,
    },
};
use litd_engine::math::{Transform, Vec3};
use litd_hash::sha256_hex;
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE740_FSV_ROOT",
        "issue740_project_lifecycle_archive",
        "manual",
    )?;
    let paths = Paths::new(root);
    let initial = fixture_document("issue740-rev-1")?;
    let saved = edited_document(initial.clone())?;

    let before = summary(&paths);
    let new_readback = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        lifecycle_args(
            &paths,
            ProjectLifecycleAction::New,
            "issue740-new",
            Some(initial),
        ),
    )?;
    let save_readback = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        lifecycle_args(
            &paths,
            ProjectLifecycleAction::SaveSnapshot,
            "issue740-save",
            Some(saved.clone()),
        ),
    )?;
    let open_readback = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        lifecycle_args(&paths, ProjectLifecycleAction::Open, "issue740-open", None),
    )?;
    let archive = inspect_litdworld_archive(&paths.project)?;
    let reopened = read_litdworld_project(&paths.project)?;
    if reopened.document != saved {
        return Err("opened .litdworld project did not match saved document".into());
    }
    let package_readback = call_mcp_project_tool(
        PACKAGE_LITDGAME_TOOL,
        json!({
            "request_id": "issue740-package",
            "requested_by": "litd-editor-issue740-fsv",
            "state_path": paths.state,
            "state_id": "issue740-project-state",
            "ledger_path": paths.root.join("package-ledger"),
            "ledger_id": "issue740-package-ledger",
            "project_path": paths.project,
            "package_path": paths.package,
            "share_id": "issue740-share",
            "created_by": "agent.project.lifecycle",
            "host_port": 19740,
            "max_package_bytes": 262144
        }),
    )?;
    let state_readback =
        call_mcp_project_tool(READ_PROJECT_TOOL, json!({"state_path": paths.state}))?;
    let after = summary(&paths);

    let expected_fingerprint = stable_fingerprint(&saved)?;
    if archive.manifest.document_fingerprint != expected_fingerprint {
        return Err("archive manifest document fingerprint did not match saved document".into());
    }
    if archive.zip_magic != "PK0304" || archive.zip_entries != ["manifest.json", "project.json"] {
        return Err("archive did not contain the canonical .litdworld entries".into());
    }
    let edges = vec![
        edge_tampered_archive(&paths)?,
        edge_missing_asset_export(&paths)?,
        edge_future_min_engine_version(&paths)?,
    ];

    let project_bytes = fs::read(&paths.project)?;
    let package_bytes = fs::read(&paths.package)?;
    let report = json!({
        "schema": "litd.issue740.project_lifecycle_archive_fsv.v1",
        "issue": 740,
        "source_of_truth": "physical .litdworld ZIP archive, re-imported project document, packaged .litdgame readback, and Calyx Aster project state",
        "paths": paths.to_json(),
        "states": {
            "before": before,
            "after": after
        },
        "happy_path": {
            "new_open_save": new_readback,
            "save_snapshot": save_readback,
            "open": open_readback,
            "read_project": state_readback,
            "package_litdgame": package_readback,
            "archive": archive,
            "reimport": {
                "document_fingerprint": reopened.document_fingerprint,
                "expected_document_fingerprint": expected_fingerprint,
                "document_matches_saved": reopened.document == saved,
                "object_count": reopened.document.objects.len(),
                "unit_count": reopened.document.objects.values().filter(|object| object.kind == "Unit").count()
            },
            "archive_file": {
                "bytes": project_bytes.len(),
                "sha256": sha256_hex(&project_bytes),
                "zip_magic": first_four_hex(&project_bytes)
            },
            "package_file": {
                "bytes": package_bytes.len(),
                "sha256": sha256_hex(&package_bytes)
            }
        },
        "edges": edges
    });
    let readback =
        run.write_json_artifact("manual/issue740-project-lifecycle-fsv.json", &report)?;
    println!(
        "ISSUE740_PROJECT_LIFECYCLE_FSV report={} report_sha256={} litdworld_sha256={} package_sha256={}",
        readback.path.display(),
        readback.sha256,
        sha256_hex(&project_bytes),
        sha256_hex(&package_bytes)
    );
    Ok(())
}

#[derive(Debug, Clone)]
struct Paths {
    root: PathBuf,
    state: PathBuf,
    project: PathBuf,
    package: PathBuf,
}

impl Paths {
    fn new(root: PathBuf) -> Self {
        Self {
            state: root.join("project-state"),
            project: root.join("ashen-reach.litdworld"),
            package: root.join("ashen-reach.litdgame.json"),
            root,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "root": self.root,
            "state": self.state,
            "project": self.project,
            "package": self.package
        })
    }
}

fn lifecycle_args(
    paths: &Paths,
    action: ProjectLifecycleAction,
    request_id: &str,
    document: Option<EditorDocument>,
) -> Value {
    json!({
        "request_id": request_id,
        "requested_by": "litd-editor-issue740-fsv",
        "state_path": paths.state,
        "state_id": "issue740-project-state",
        "ledger_path": paths.root.join(format!("{request_id}-ledger")),
        "ledger_id": format!("{request_id}-ledger"),
        "action": action,
        "project_id": "project.ashen_reach_issue740",
        "title": "Ashen Reach Issue 740",
        "version": "1.0.0",
        "tags": ["dark_fantasy", "archive", "local_first"],
        "project_path": paths.project,
        "snapshot_label": request_id,
        "document": document
    })
}

fn fixture_document(revision_id: &str) -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let mut document = bare_document(revision_id)?;
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .ok_or("fixture asset vault record missing")?;
    assign_vault_model_to_unit(&mut document, "unit_project_hero", &record)?;
    document.validate()?;
    Ok(document)
}

fn bare_document(revision_id: &str) -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let mut document = EditorDocument::new("calyx://world/issue740-project", revision_id)?;
    document.upsert_object(
        EditorObject::new("world", "World", None, Transform::IDENTITY)
            .with_property("name", "Ashen Reach"),
    )?;
    document.upsert_object(
        EditorObject::new(
            "unit_project_hero",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(2.0, 0.0, -4.0)),
        )
        .with_property("faction", "Dawnward"),
    )?;
    document.validate()?;
    Ok(document)
}

fn edited_document(
    mut document: EditorDocument,
) -> Result<EditorDocument, Box<dyn std::error::Error>> {
    document.revision_id = "issue740-rev-2".to_string();
    document
        .objects
        .get_mut("unit_project_hero")
        .ok_or("unit_project_hero missing")?
        .properties
        .insert("role".to_string(), "archive_roundtrip_probe".to_string());
    document.validate()?;
    Ok(document)
}

fn edge_tampered_archive(paths: &Paths) -> Result<Value, Box<dyn std::error::Error>> {
    let edge_path = paths.root.join("tampered-project.litdworld");
    let mut bytes = fs::read(&paths.project)?;
    let midpoint = bytes.len() / 2;
    bytes[midpoint] ^= 0xff;
    fs::write(&edge_path, &bytes)?;
    let error = inspect_litdworld_archive(&edge_path)
        .unwrap_err()
        .to_string();
    Ok(json!({
        "edge": "tampered_file_verify_fails",
        "path": edge_path,
        "sha256": sha256_hex(&bytes),
        "error": error,
        "failed_closed": error.contains("litdworld-archive")
    }))
}

fn edge_missing_asset_export(paths: &Paths) -> Result<Value, Box<dyn std::error::Error>> {
    let edge_paths = Paths {
        root: paths.root.join("missing-asset-edge"),
        state: paths.root.join("missing-asset-edge").join("project-state"),
        project: paths
            .root
            .join("missing-asset-edge")
            .join("missing-asset.litdworld"),
        package: paths
            .root
            .join("missing-asset-edge")
            .join("missing-asset.litdgame.json"),
    };
    fs::create_dir_all(&edge_paths.root)?;
    let missing = bare_document("issue740-missing-asset")?;
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        lifecycle_args(
            &edge_paths,
            ProjectLifecycleAction::SaveSnapshot,
            "issue740-missing-asset-save",
            Some(missing),
        ),
    )?;
    let before = summary(&edge_paths);
    let error = call_mcp_project_tool(
        PACKAGE_LITDGAME_TOOL,
        json!({
            "request_id": "issue740-missing-asset-package",
            "requested_by": "litd-editor-issue740-fsv",
            "state_path": edge_paths.state,
            "state_id": "issue740-project-state",
            "ledger_path": edge_paths.root.join("missing-asset-ledger"),
            "ledger_id": "issue740-missing-asset-ledger",
            "project_path": edge_paths.project,
            "package_path": edge_paths.package,
            "share_id": "issue740-missing-asset",
            "created_by": "agent.project.lifecycle",
            "host_port": 19741,
            "max_package_bytes": 262144
        }),
    )
    .unwrap_err()
    .to_string();
    let after = summary(&edge_paths);
    Ok(json!({
        "edge": "missing_asset_export_error",
        "before": before,
        "after": after,
        "state_unchanged": before == after,
        "error": error,
        "failed_closed": error.contains("quick-share package requires at least one asset product")
    }))
}

fn edge_future_min_engine_version(paths: &Paths) -> Result<Value, Box<dyn std::error::Error>> {
    let edge_path = paths.root.join("future-min-engine.litdworld");
    rewrite_manifest(&paths.project, &edge_path, |manifest| {
        manifest.min_engine_version = "999.0.0".to_string();
    })?;
    let error = inspect_litdworld_archive(&edge_path)
        .unwrap_err()
        .to_string();
    Ok(json!({
        "edge": "version_range_honored_by_loader",
        "path": edge_path,
        "error": error,
        "failed_closed": error.contains("archive requires engine >=")
    }))
}

fn rewrite_manifest(
    source: &Path,
    destination: &Path,
    edit: impl FnOnce(&mut LitdWorldArchiveManifest),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = ZipArchive::new(Cursor::new(fs::read(source)?))?;
    let mut manifest_bytes = Vec::new();
    archive
        .by_name("manifest.json")?
        .read_to_end(&mut manifest_bytes)?;
    let mut project_bytes = Vec::new();
    archive
        .by_name("project.json")?
        .read_to_end(&mut project_bytes)?;
    let mut manifest: LitdWorldArchiveManifest = serde_json::from_slice(&manifest_bytes)?;
    edit(&mut manifest);
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    write_zip(destination, &manifest_bytes, &project_bytes)?;
    Ok(())
}

fn write_zip(
    path: &Path,
    manifest_bytes: &[u8],
    project_bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    writer.start_file("manifest.json", options)?;
    writer.write_all(manifest_bytes)?;
    writer.start_file("project.json", options)?;
    writer.write_all(project_bytes)?;
    fs::write(path, writer.finish()?.into_inner())?;
    Ok(())
}

fn summary(paths: &Paths) -> Value {
    let state = read_mcp_project_state(&paths.state).ok();
    let project_bytes = fs::read(&paths.project).ok();
    let package_bytes = fs::read(&paths.package).ok();
    json!({
        "state_exists": paths.state.exists(),
        "project_exists": paths.project.exists(),
        "project_is_file": paths.project.is_file(),
        "project_bytes": project_bytes.as_ref().map(Vec::len).unwrap_or(0),
        "project_sha256": project_bytes.as_ref().map(|bytes| sha256_hex(bytes)).unwrap_or_default(),
        "project_first_four_hex": project_bytes.as_ref().map(|bytes| first_four_hex(bytes)).unwrap_or_default(),
        "package_exists": paths.package.exists(),
        "package_bytes": package_bytes.as_ref().map(Vec::len).unwrap_or(0),
        "package_sha256": package_bytes.as_ref().map(|bytes| sha256_hex(bytes)).unwrap_or_default(),
        "project_count": state.as_ref().map(|readback| readback.project_count).unwrap_or(0),
        "package_count": state.as_ref().map(|readback| readback.package_count).unwrap_or(0),
    })
}

fn first_four_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}
