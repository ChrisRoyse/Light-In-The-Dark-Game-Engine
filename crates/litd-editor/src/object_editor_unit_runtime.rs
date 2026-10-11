//! Unit runtime round-trip helpers (#732): revert a persisted unit from a lowered TOML,
//! load a lowered unit into the deterministic sim through the canonical Calyx content store,
//! and live-validate a unit object for lowering. Split out of `object_editor.rs` to hold the
//! <500-line ratchet (#1466/#800 sec.6); re-exported so every `object_editor::*` path is
//! unchanged.

use std::{collections::BTreeMap, path::Path};

use litd_engine::{
    EngineError,
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_lowering::EngineLoweringValve,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    math::{Fixed, FixedVec3},
    sim::{FixedTickSim, SimConfig},
};
use serde_json::json;

use crate::history::EditorObject;
use crate::mcp_object_tools::{APPLY_OBJECT_DELTA_TOOL, ObjectPropertyDelta, call_mcp_object_tool};

use super::validation::{
    object_editor_error, push_abilities_property, push_model_property, push_required,
    push_required_property, push_u32_property,
};
use super::{
    ObjectEditorLiveValidation, ObjectEditorSimReadback, UNIT_HIT_POINTS_MAX, UNIT_HIT_POINTS_MIN,
    UNIT_MOVE_SPEED_MAX, UNIT_MOVE_SPEED_MIN, verify_lowered_unit_toml,
};

pub fn revert_unit_from_toml(
    document_path: impl AsRef<Path>,
    ledger_path: impl AsRef<Path>,
    ledger_id: &str,
    request_id: &str,
    requested_by: &str,
    canonical_toml: &str,
    expected_fingerprint: &str,
) -> Result<crate::mcp_object_tools::McpObjectToolCallOutput, EngineError> {
    let lowered = verify_lowered_unit_toml(canonical_toml, expected_fingerprint)?;
    let mut upsert_properties = BTreeMap::new();
    upsert_properties.insert(
        "stats.hit_points".to_string(),
        lowered.unit.hit_points.to_string(),
    );
    upsert_properties.insert(
        "stats.move_speed".to_string(),
        lowered.unit.move_speed.to_string(),
    );
    upsert_properties.insert("art.model".to_string(), lowered.unit.model.clone());
    let mut remove_properties = Vec::new();
    if lowered.unit.abilities.is_empty() {
        remove_properties.push("abilities.normal".to_string());
    } else {
        upsert_properties.insert(
            "abilities.normal".to_string(),
            lowered.unit.abilities.join(","),
        );
    }
    call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path.as_ref(),
            "ledger_path": ledger_path.as_ref(),
            "ledger_id": ledger_id,
            "request_id": request_id,
            "requested_by": requested_by,
            "initial_document": null,
            "object_id": lowered.unit.object_id,
            "delta": ObjectPropertyDelta {
                set_kind: None,
                upsert_properties,
                remove_properties,
            }
        }),
    )
}

pub fn load_lowered_unit_toml_into_sim(
    canonical_toml: &str,
    expected_fingerprint: &str,
    runtime_vault_dir: impl AsRef<Path>,
) -> Result<ObjectEditorSimReadback, EngineError> {
    let lowered = verify_lowered_unit_toml(canonical_toml, expected_fingerprint)?;
    let runtime_vault_dir = runtime_vault_dir.as_ref();
    let db = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(runtime_vault_dir))
        .map_err(|error| object_editor_error("runtime.db", error.to_string()))?;
    let store = EngineCalyxContentStore::new(db);
    let move_speed_fp = Fixed::from_i32(lowered.unit.move_speed as i32).raw();
    let content = store
        .put_content(&EngineContentRecord {
            kind: EngineContentKind::Unit,
            id: lowered.unit.object_id.clone(),
            display_name: lowered.unit.display_name.clone(),
            payload: json!({
                "unit_id": lowered.unit.object_id,
                "rawcode": lowered.unit.rawcode,
                "hit_points": lowered.unit.hit_points,
                "move_speed": lowered.unit.move_speed,
                "move_speed_fp": move_speed_fp,
                "model": lowered.unit.model,
                "abilities": lowered.unit.abilities,
            }),
            source: EngineContentSource {
                path: format!("object-editor://lowered-unit/{}", lowered.unit.object_id),
                author: lowered.provenance.lowering_version.clone(),
                source_hash: expected_fingerprint.to_string(),
            },
            anchors: Vec::new(),
        })
        .map_err(|error| object_editor_error("runtime.content", error.to_string()))?;
    let package = EngineLoweringValve::lower_content_for_sim(&content)
        .map_err(|error| object_editor_error("runtime.lower", error.to_string()))?;
    let lowered_readback = EngineLoweringValve::verify_lowered_package(&package)
        .map_err(|error| object_editor_error("runtime.verify", error.to_string()))?;
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(64)?, 732)?;
    let body = sim.spawn_body(
        FixedVec3::ZERO,
        FixedVec3::new(
            Fixed::from_i32(lowered.unit.move_speed as i32),
            Fixed::ZERO,
            Fixed::ZERO,
        ),
        FixedVec3::ZERO,
    )?;
    let body_state = sim.body_state(body).ok_or_else(|| {
        object_editor_error("runtime.sim_body", "spawned unit body missing from sim")
    })?;
    Ok(ObjectEditorSimReadback {
        runtime_vault_dir: runtime_vault_dir.display().to_string(),
        unit_object_id: lowered.unit.object_id,
        move_speed: lowered.unit.move_speed,
        move_speed_fp,
        abilities: lowered.unit.abilities,
        content_cx_id: content.cx_id,
        content_input_sha256: content.input_sha256,
        lowered_package: package,
        lowered_readback,
        sim_body_id: body.index(),
        sim_body_count: sim.body_count(),
        sim_velocity_x_raw: body_state.velocity.x.raw(),
        sim_state_hash: sim.state_hash(),
    })
}

pub fn validate_unit_object_for_lowering(object: &EditorObject) -> ObjectEditorLiveValidation {
    let mut fields = Vec::new();
    push_required(
        &mut fields,
        "object.kind",
        &object.kind,
        object.kind == "Unit",
    );
    push_required_property(&mut fields, object, "object.rawcode");
    push_required_property(&mut fields, object, "object.display_name");
    push_u32_property(
        &mut fields,
        object,
        "stats.hit_points",
        UNIT_HIT_POINTS_MIN,
        UNIT_HIT_POINTS_MAX,
    );
    push_u32_property(
        &mut fields,
        object,
        "stats.move_speed",
        UNIT_MOVE_SPEED_MIN,
        UNIT_MOVE_SPEED_MAX,
    );
    push_model_property(&mut fields, object, "art.model");
    push_abilities_property(&mut fields, object, "abilities.normal");
    let valid = fields.iter().all(|field| field.valid);
    ObjectEditorLiveValidation {
        object_id: object.id.clone(),
        valid,
        fields,
    }
}
