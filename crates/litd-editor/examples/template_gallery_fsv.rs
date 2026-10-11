//! FSV for #901: template gallery -- browse, one-click clone, dissect.
//!
//! Drives the REAL editor MCP tools (`list_templates`, `clone_template`) against
//! Calyx Aster vaults, clones a starter into a FRESH project, mutates the clone, and
//! proves the template is byte-for-byte untouched via an INDEPENDENT on-disk readback
//! (fingerprints). Covers three edge cases (name collision typed error, unknown
//! template typed error, empty clone identity typed error) with before/after state,
//! and a negative control that proves the "template unchanged" fingerprint check has
//! teeth (a direct mutation of a template vault DOES change its fingerprint). Writes a
//! report.json whose sha256 is recomputed independently via certutil.

use std::fs;
use std::path::Path;
use std::process::Command;

use litd_editor::history::EditorRegion;
use litd_editor::mcp_template_tools::{
    CLONE_TEMPLATE_TOOL, LIST_TEMPLATES_TOOL, call_mcp_template_tool, read_document,
    read_document_fingerprint,
};
use litd_editor::mcp_world_tools::write_mcp_world_document;
use litd_editor::template_gallery::{MELEE_SKIRMISH_TEMPLATE, build_template_document};
use litd_engine::EngineError;
use litd_engine::math::Vec3;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Fsv {
    issue: u32,
    template_ids: Vec<String>,
    clone: CloneEvidence,
    edges: Vec<Value>,
    negative_control: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CloneEvidence {
    template_id: String,
    template_fingerprint_before: String,
    clone_fingerprint: String,
    clone_calyx_document_id: String,
    template_fingerprint_after_clone_mutated: String,
    clone_has_mutation: bool,
    template_has_mutation: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_TEMPLATE_GALLERY_FSV_ROOT",
        "issue901_template_gallery",
        "manual",
    )?;
    reset(&root);
    let report_path = root.join("template-gallery-fsv-readback.json");

    // Browse the gallery through the real tool.
    let listed = call_mcp_template_tool(LIST_TEMPLATES_TOOL, json!({}))?;
    let template_ids: Vec<String> = serde_json::from_value(listed.result["templates"].clone())
        .map(|v: Vec<Value>| {
            v.into_iter()
                .map(|t| t["template_id"].as_str().unwrap().to_string())
                .collect()
        })?;
    println!("issue901 gallery templates={template_ids:?}");

    let clone = clone_and_prove_independence(&root)?;
    let edges = vec![
        edge_collision(&root)?,
        edge_unknown_template(&root)?,
        edge_empty_identity(&root)?,
    ];
    let negative_control = negative_control(&root)?;

    let fsv = Fsv {
        issue: 901,
        template_ids,
        clone,
        edges,
        negative_control,
    };
    fs::write(&report_path, serde_json::to_vec_pretty(&fsv)?)?;
    let readback: Fsv = serde_json::from_slice(&fs::read(&report_path)?)?;
    if readback != fsv {
        return Err("report readback mismatch".into());
    }
    let recorded = fsv_run.record_json_file(&report_path)?;
    let certutil = certutil_sha256(&report_path)?;
    if certutil != recorded.sha256 {
        return Err(format!(
            "sha256 mismatch: fsv_run={} certutil={certutil}",
            recorded.sha256
        )
        .into());
    }
    println!(
        "issue901 template_gallery_fsv VERDICT=GREEN report={} report_sha256={} certutil_sha256={}",
        report_path.display(),
        recorded.sha256,
        certutil
    );
    Ok(())
}

fn clone_and_prove_independence(root: &Path) -> Result<CloneEvidence, Box<dyn std::error::Error>> {
    let template_path = root.join("template-melee");
    let clone_path = root.join("clone-melee");
    let ledger = root.join("clone-ledger.json");

    let output = call_mcp_template_tool(
        CLONE_TEMPLATE_TOOL,
        json!({
            "template_id": MELEE_SKIRMISH_TEMPLATE,
            "template_path": template_path,
            "clone_path": clone_path,
            "clone_calyx_document_id": "calyx://project/my-melee",
            "clone_revision_id": "my-melee-rev-1",
            "ledger_path": ledger,
            "ledger_id": "clone-ledger",
            "request_id": "clone-1",
            "requested_by": "issue901-fsv",
        }),
    )?;

    // Independent on-disk readback of the template BEFORE mutating the clone.
    let template_before = read_document_fingerprint(&template_path)?;
    let clone_fingerprint = output.result["clone_fingerprint"]
        .as_str()
        .ok_or("clone_fingerprint missing")?
        .to_string();

    // Mutate the CLONE and persist it.
    let mut clone = read_document(&clone_path)?;
    clone.upsert_region(EditorRegion::new(
        "region.clone_only",
        "Clone Only",
        Vec3::new(4.0, 0.0, 4.0),
        Vec3::new(1.0, 4.0, 1.0),
    ))?;
    write_mcp_world_document(&clone_path, &clone)?;

    // Independent on-disk readback of BOTH after the mutation.
    let template_after = read_document_fingerprint(&template_path)?;
    let clone_doc = read_document(&clone_path)?;
    let template_doc = read_document(&template_path)?;
    let clone_has_mutation = clone_doc.regions.contains_key("region.clone_only");
    let template_has_mutation = template_doc.regions.contains_key("region.clone_only");

    if template_before != template_after || template_has_mutation || !clone_has_mutation {
        return Err(format!(
            "clone independence violated: before={template_before} after={template_after} template_mut={template_has_mutation} clone_mut={clone_has_mutation}"
        )
        .into());
    }
    println!(
        "issue901 clone_independence template_before={template_before} template_after={template_after} clone={clone_fingerprint}"
    );
    Ok(CloneEvidence {
        template_id: MELEE_SKIRMISH_TEMPLATE.to_string(),
        template_fingerprint_before: template_before,
        clone_fingerprint,
        clone_calyx_document_id: "calyx://project/my-melee".to_string(),
        template_fingerprint_after_clone_mutated: template_after,
        clone_has_mutation,
        template_has_mutation,
    })
}

fn edge_collision(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let template_path = root.join("collide-template");
    let clone_path = root.join("collide-clone");
    let ledger = root.join("collide-ledger.json");
    // Pre-create the destination so the clone must refuse it.
    let existing = build_template_document(MELEE_SKIRMISH_TEMPLATE)?;
    write_mcp_world_document(&clone_path, &existing)?;
    let before = read_document_fingerprint(&clone_path)?;
    let error = call_mcp_template_tool(
        CLONE_TEMPLATE_TOOL,
        clone_args(
            &template_path,
            &clone_path,
            &ledger,
            "calyx://project/collide",
            "rev-1",
        ),
    )
    .unwrap_err();
    let after = read_document_fingerprint(&clone_path)?;
    if before != after {
        return Err("collision edge mutated the destination".into());
    }
    println!("issue901 edge name_collision error={error}");
    Ok(json!({
        "name": "name_collision_typed_error",
        "before_fingerprint": before,
        "after_fingerprint": after,
        "error_code": error_code(&error),
    }))
}

fn edge_unknown_template(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let template_path = root.join("unknown-template");
    let clone_path = root.join("unknown-clone");
    let ledger = root.join("unknown-ledger.json");
    let error = call_mcp_template_tool(
        CLONE_TEMPLATE_TOOL,
        json!({
            "template_id": "template.does.not.exist",
            "template_path": template_path,
            "clone_path": clone_path,
            "clone_calyx_document_id": "calyx://project/x",
            "clone_revision_id": "rev-1",
            "ledger_path": ledger,
            "ledger_id": "unknown-ledger",
            "request_id": "unknown-1",
            "requested_by": "issue901-fsv",
        }),
    )
    .unwrap_err();
    println!("issue901 edge unknown_template error={error}");
    Ok(json!({
        "name": "unknown_template_typed_error",
        "clone_created": clone_path.exists(),
        "error_code": error_code(&error),
    }))
}

fn edge_empty_identity(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let template_path = root.join("empty-template");
    let clone_path = root.join("empty-clone");
    let ledger = root.join("empty-ledger.json");
    let error = call_mcp_template_tool(
        CLONE_TEMPLATE_TOOL,
        clone_args(&template_path, &clone_path, &ledger, "  ", "rev-1"),
    )
    .unwrap_err();
    println!("issue901 edge empty_clone_identity error={error}");
    Ok(json!({
        "name": "empty_clone_identity_typed_error",
        "clone_created": clone_path.exists(),
        "error_code": error_code(&error),
    }))
}

/// Run-invariant error identity for the report: the typed error's logical FIELD code
/// (path-free), never the free-form message -- collision messages embed the absolute,
/// run-specific destination vault path, which would make report.json sha256
/// non-reproducible across runs/machines. The full message still prints on-screen.
fn error_code(error: &EngineError) -> String {
    match error {
        EngineError::UiRenderInvalidConfig { field, .. } => field.clone(),
        // No edge here produces another variant; fall back to the path-free variant
        // name (payload dropped) so the report stays deterministic regardless.
        other => format!("{other:?}")
            .split([' ', '{', '('])
            .next()
            .unwrap_or("EngineError")
            .to_string(),
    }
}

fn negative_control(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    // The "template unchanged" proof relies on the fingerprint DETECTING mutations. Prove
    // that teeth: materialize a template vault, fingerprint it, mutate it in place, and show
    // the fingerprint changes. A tautological (always-equal) check would fail this control.
    let vault = root.join("control-template");
    let document = build_template_document(MELEE_SKIRMISH_TEMPLATE)?;
    write_mcp_world_document(&vault, &document)?;
    let before = read_document_fingerprint(&vault)?;
    let mut mutated = read_document(&vault)?;
    mutated.upsert_region(EditorRegion::new(
        "region.sabotage",
        "Sabotage",
        Vec3::new(4.0, 0.0, 4.0),
        Vec3::new(1.0, 4.0, 1.0),
    ))?;
    write_mcp_world_document(&vault, &mutated)?;
    let after = read_document_fingerprint(&vault)?;
    if before == after {
        return Err("negative control failed: fingerprint did not detect a mutation".into());
    }
    Ok(json!({
        "name": "fingerprint_detects_mutation",
        "before_fingerprint": before,
        "after_fingerprint": after,
        "changed": before != after,
    }))
}

fn clone_args(
    template_path: &Path,
    clone_path: &Path,
    ledger: &Path,
    calyx_document_id: &str,
    revision_id: &str,
) -> Value {
    json!({
        "template_id": MELEE_SKIRMISH_TEMPLATE,
        "template_path": template_path,
        "clone_path": clone_path,
        "clone_calyx_document_id": calyx_document_id,
        "clone_revision_id": revision_id,
        "ledger_path": ledger,
        "ledger_id": "edge-ledger",
        "request_id": "edge-1",
        "requested_by": "issue901-fsv",
    })
}

fn certutil_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("certutil")
        .args(["-hashfile", &path.display().to_string(), "SHA256"])
        .output()?;
    if !output.status.success() {
        return Err("certutil failed".into());
    }
    let text = String::from_utf8(output.stdout)?;
    let hash = text
        .lines()
        .map(str::trim)
        .find(|line| line.len() == 64 && line.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or("certutil produced no sha256 line")?;
    Ok(hash.to_lowercase())
}

fn reset(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
