use std::{fs, path::Path};

use litd_editor::{
    mcp_object_tools::{
        APPLY_OBJECT_DELTA_TOOL, COPY_BASE_OBJECT_TOOL, call_mcp_object_tool,
        read_mcp_object_document,
    },
    object_editor::{
        issue732_base_document, load_lowered_unit_toml_into_sim, lower_unit_from_document_path,
        object_editor_base_templates, revert_unit_from_toml, verify_lowered_unit_toml,
        write_lowered_unit_toml,
    },
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_OBJECT_EDITOR_ISSUE732_FSV_ROOT",
        "issue732_object_editor",
        "manual",
    )?;
    reset_root(&root)?;

    let document_path = root.join("object-document-aster");
    let copy_ledger = root.join("copy-ledger-aster");
    let delta_ledger = root.join("delta-ledger-aster");
    let edge_ledger = root.join("edge-ledger-aster");
    let revert_ledger = root.join("revert-ledger-aster");
    let original_toml_path = root.join("original-unit.toml");
    let edited_toml_path = root.join("edited-unit.toml");
    let runtime_vault = root.join("runtime-calyx");
    let new_unit_id = "unit.issue732.guardian_footman";

    let seed = issue732_base_document();
    let templates = object_editor_base_templates(&seed)?;
    let footman = templates
        .iter()
        .find(|template| template.base_object_id == "base.unit.footman")
        .ok_or("missing issue732 footman template")?
        .clone();

    let before = state(&document_path)?;
    let copy = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": copy_ledger,
            "ledger_id": "issue732-copy-ledger",
            "request_id": "issue732-copy",
            "requested_by": "litd-editor-issue732-fsv",
            "initial_document": seed,
            "new_object_id": new_unit_id,
            "rawcode": "u732",
            "base": footman,
            "overrides": {}
        }),
    )?;
    let original = lower_unit_from_document_path(&document_path, new_unit_id)?;
    let original_toml = write_lowered_unit_toml(&original_toml_path, &original)?;

    let delta = call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": delta_ledger,
            "ledger_id": "issue732-delta-ledger",
            "request_id": "issue732-delta",
            "requested_by": "litd-editor-issue732-fsv",
            "initial_document": null,
            "object_id": new_unit_id,
            "delta": {
                "set_kind": null,
                "upsert_properties": {
                    "stats.move_speed": "310",
                    "abilities.normal": "ability.issue732.defend,ability.issue732.guardian_leap"
                },
                "remove_properties": []
            }
        }),
    )?;
    let edited = lower_unit_from_document_path(&document_path, new_unit_id)?;
    let edited_toml = write_lowered_unit_toml(&edited_toml_path, &edited)?;
    let verified = verify_lowered_unit_toml(&edited.canonical_toml, &edited.fingerprint)?;
    let sim = load_lowered_unit_toml_into_sim(
        &edited.canonical_toml,
        &edited.fingerprint,
        &runtime_vault,
    )?;

    let out_of_range = edge_out_of_range(&document_path, &edge_ledger, new_unit_id)?;
    let collision = edge_collision(&document_path, &edge_ledger, new_unit_id)?;
    let revert = revert_unit_from_toml(
        &document_path,
        &revert_ledger,
        "issue732-revert-ledger",
        "issue732-revert",
        "litd-editor-issue732-fsv",
        &original.canonical_toml,
        &original.fingerprint,
    )?;
    let reverted = lower_unit_from_document_path(&document_path, new_unit_id)?;
    let after = state(&document_path)?;
    let saved_toml_has_move_speed_310 = edited_toml.document.unit.move_speed == 310;
    let saved_toml_has_guardian_leap = edited_toml
        .document
        .unit
        .abilities
        .contains(&"ability.issue732.guardian_leap".to_string());
    let lower_unit_valid = edited_toml.document.validation.lower_unit_valid;
    let sim_has_move_speed_310 = sim.move_speed == 310;
    let sim_has_guardian_leap = sim
        .abilities
        .contains(&"ability.issue732.guardian_leap".to_string());

    let report = json!({
        "schema": "litd.editor.issue732.object_editor_fsv.v1",
        "issue": 732,
        "manual_gate": "supporting report; manual FSV must inspect this JSON, the TOML files, Aster document rows, runtime stats, and live editor HWND screenshot",
        "source_of_truth": {
            "object_document_aster": document_path,
            "copy_ledger_aster": copy_ledger,
            "delta_ledger_aster": delta_ledger,
            "revert_ledger_aster": revert_ledger,
            "original_unit_toml": original_toml_path,
            "edited_unit_toml": edited_toml_path,
            "runtime_calyx_vault": runtime_vault
        },
        "states": {
            "before": before,
            "after": after.clone()
        },
        "happy_path": {
            "base_template_count": templates.len(),
            "copy": copy,
            "delta": delta,
            "original_toml": original_toml.clone(),
            "edited_toml": edited_toml.clone(),
            "verified_lower_unit": verified,
            "sim_readback": sim.clone(),
            "acceptance": {
                "saved_toml_has_move_speed_310": saved_toml_has_move_speed_310,
                "saved_toml_has_guardian_leap": saved_toml_has_guardian_leap,
                "lower_unit_valid": lower_unit_valid,
                "sim_has_move_speed_310": sim_has_move_speed_310,
                "sim_has_guardian_leap": sim_has_guardian_leap
            }
        },
        "edges": {
            "out_of_range_rejected": out_of_range,
            "name_collision_handled": collision,
            "revert": {
                "tool_output": revert,
                "restored_original_fingerprint": reverted.fingerprint == original.fingerprint,
                "reverted_unit": verify_lowered_unit_toml(&reverted.canonical_toml, &reverted.fingerprint)?
            }
        },
        "artifact_hashes": {
            "original_toml_sha256": sha256_file(&original_toml_path)?,
            "edited_toml_sha256": sha256_file(&edited_toml_path)?
        }
    });
    let report_path = root.join("issue732-object-editor-fsv-readback.json");
    let readback = fsv_run.write_json_file(&report_path, &report)?;
    println!(
        "ISSUE732_OBJECT_EDITOR_FSV report={} report_sha256={} original_toml_sha256={} edited_toml_sha256={} document_fingerprint={} sim_hash={:016X}",
        readback.path.display(),
        readback.sha256,
        sha256_file(&original_toml_path)?,
        sha256_file(&edited_toml_path)?,
        after["document_fingerprint"].as_str().unwrap_or("missing"),
        sim.sim_state_hash,
    );
    Ok(())
}

fn edge_out_of_range(
    document_path: &Path,
    ledger_path: &Path,
    object_id: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = state(document_path)?;
    let error = call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger_path,
            "ledger_id": "issue732-out-of-range",
            "request_id": "issue732-out-of-range",
            "requested_by": "litd-editor-issue732-fsv",
            "initial_document": null,
            "object_id": object_id,
            "delta": {
                "set_kind": null,
                "upsert_properties": { "stats.move_speed": "9999" },
                "remove_properties": []
            }
        }),
    )
    .unwrap_err()
    .to_string();
    let after = state(document_path)?;
    Ok(json!({
        "before": before,
        "after": after,
        "unchanged": before["document_fingerprint"] == after["document_fingerprint"],
        "error": error
    }))
}

fn edge_collision(
    document_path: &Path,
    ledger_path: &Path,
    object_id: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = state(document_path)?;
    let error = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger_path,
            "ledger_id": "issue732-collision",
            "request_id": "issue732-collision",
            "requested_by": "litd-editor-issue732-fsv",
            "initial_document": null,
            "new_object_id": object_id,
            "rawcode": "u733",
            "base": {
                "base_object_id": "base.unit.footman",
                "kind": "Unit",
                "display_name": "Footman",
                "properties": {
                    "stats.hit_points": "420",
                    "stats.move_speed": "270",
                    "art.model": "units/human/Footman/Footman.mdl"
                }
            },
            "overrides": {}
        }),
    )
    .unwrap_err()
    .to_string();
    let after = state(document_path)?;
    Ok(json!({
        "before": before,
        "after": after,
        "unchanged": before["document_fingerprint"] == after["document_fingerprint"],
        "error": error
    }))
}

fn state(document_path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    if !document_path.exists() {
        return Ok(json!({
            "document_exists": false,
            "object_count": 0,
            "unit_count": 0,
            "ability_count": 0
        }));
    }
    let readback = read_mcp_object_document(document_path)?;
    let unit = readback
        .document
        .objects
        .get("unit.issue732.guardian_footman")
        .map(|object| {
            json!({
                "kind": object.kind,
                "move_speed": object.properties.get("stats.move_speed"),
                "abilities": object.properties.get("abilities.normal")
            })
        });
    Ok(json!({
        "document_exists": true,
        "document_path_is_dir": document_path.is_dir(),
        "object_count": readback.object_count,
        "unit_count": readback.unit_count,
        "ability_count": readback.ability_count,
        "typed_object_rows": readback.typed_object_rows,
        "constellation_rows": readback.constellation_rows,
        "reactive_audit_rows": readback.reactive_audit_rows,
        "reactive_fired_rows": readback.reactive_fired_rows,
        "document_sha256": readback.sha256,
        "document_fingerprint": readback.document_fingerprint,
        "edited_unit": unit
    }))
}

fn reset_root(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    fs::create_dir_all(root)?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
