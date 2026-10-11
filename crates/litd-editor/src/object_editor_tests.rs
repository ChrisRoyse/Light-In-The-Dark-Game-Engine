use std::{fs, path::Path};

use litd_engine::math::Fixed;
use serde_json::json;

use super::*;
use crate::mcp_object_tools::{
    APPLY_OBJECT_DELTA_TOOL, COPY_BASE_OBJECT_TOOL, call_mcp_object_tool, read_mcp_object_document,
};

#[test]
fn issue732_duplicate_customize_lower_load_reject_collision_and_revert() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue732_object_editor", "unit").unwrap();
    reset_root(&root);
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
    let footman = object_editor_base_templates(&seed)
        .unwrap()
        .into_iter()
        .find(|template| template.base_object_id == "base.unit.footman")
        .unwrap();
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": copy_ledger,
            "ledger_id": "issue732-copy",
            "request_id": "issue732-copy",
            "requested_by": "litd-editor-issue732-test",
            "initial_document": seed,
            "new_object_id": new_unit_id,
            "rawcode": "u732",
            "base": footman,
            "overrides": {}
        }),
    )
    .unwrap();

    let original = lower_unit_from_document_path(&document_path, new_unit_id).unwrap();
    let original_readback = write_lowered_unit_toml(&original_toml_path, &original).unwrap();
    assert_eq!(original_readback.document.unit.move_speed, 270);
    assert_eq!(
        original_readback.document.unit.abilities,
        vec!["ability.issue732.defend".to_string()]
    );

    call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": delta_ledger,
            "ledger_id": "issue732-delta",
            "request_id": "issue732-delta",
            "requested_by": "litd-editor-issue732-test",
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
    )
    .unwrap();

    let edited = lower_unit_from_document_path(&document_path, new_unit_id).unwrap();
    let edited_readback = write_lowered_unit_toml(&edited_toml_path, &edited).unwrap();
    let verified = verify_lowered_unit_toml(&edited.canonical_toml, &edited.fingerprint).unwrap();
    assert_eq!(verified, edited_readback.document);
    assert_eq!(verified.unit.move_speed, 310);
    assert_eq!(
        verified.unit.abilities,
        vec![
            "ability.issue732.defend".to_string(),
            "ability.issue732.guardian_leap".to_string()
        ]
    );

    let sim = crate::asset_vault_fixture::expect_calyx_substrate(
        load_lowered_unit_toml_into_sim(
            &edited.canonical_toml,
            &edited.fingerprint,
            &runtime_vault,
        ),
        "editor.object_editor.runtime.content",
    );
    assert_eq!(sim.move_speed, 310);
    assert_eq!(sim.sim_velocity_x_raw, Fixed::from_i32(310).raw());
    assert!(
        sim.abilities
            .contains(&"ability.issue732.guardian_leap".to_string())
    );
    assert_eq!(sim.sim_body_count, 1);

    let before_bad_speed = read_mcp_object_document(&document_path)
        .unwrap()
        .document_fingerprint;
    let bad_speed = call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": edge_ledger,
            "ledger_id": "issue732-bad-speed",
            "request_id": "issue732-bad-speed",
            "requested_by": "litd-editor-issue732-test",
            "initial_document": null,
            "object_id": new_unit_id,
            "delta": {
                "set_kind": null,
                "upsert_properties": { "stats.move_speed": "9999" },
                "remove_properties": []
            }
        }),
    )
    .unwrap_err()
    .to_string();
    let after_bad_speed = read_mcp_object_document(&document_path)
        .unwrap()
        .document_fingerprint;
    assert_eq!(before_bad_speed, after_bad_speed);
    assert!(bad_speed.contains("stats.move_speed"));

    let before_collision = read_mcp_object_document(&document_path)
        .unwrap()
        .document_fingerprint;
    let collision = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": edge_ledger,
            "ledger_id": "issue732-collision",
            "request_id": "issue732-collision",
            "requested_by": "litd-editor-issue732-test",
            "initial_document": null,
            "new_object_id": new_unit_id,
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
    let after_collision = read_mcp_object_document(&document_path)
        .unwrap()
        .document_fingerprint;
    assert_eq!(before_collision, after_collision);
    assert!(collision.contains("already exists"));

    revert_unit_from_toml(
        &document_path,
        &revert_ledger,
        "issue732-revert",
        "issue732-revert",
        "litd-editor-issue732-test",
        &original.canonical_toml,
        &original.fingerprint,
    )
    .unwrap();
    let reverted = lower_unit_from_document_path(&document_path, new_unit_id).unwrap();
    assert_eq!(reverted.fingerprint, original.fingerprint);
    assert_eq!(
        verify_lowered_unit_toml(&reverted.canonical_toml, &reverted.fingerprint)
            .unwrap()
            .unit
            .move_speed,
        270
    );
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
