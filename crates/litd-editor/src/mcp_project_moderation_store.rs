use std::path::Path;

use litd_engine::EngineError;

use super::super::mcp_project_aster_store::{
    AsterJsonSpec, read_json_record, read_json_record_optional, write_json_record,
};
use super::super::mcp_project_tools_validation::validate_path;
use super::mcp_project_moderation_types::{
    MODERATION_SAFETY_STATE_SCHEMA, ModerationSafetyState, ModerationSafetyStateReadback,
};
use super::mcp_project_moderation_validation::{moderation_error, validate_moderation_state};

const MODERATION_SOURCE_OF_TRUTH: &str = "Calyx Aster moderation safety state";
const MODERATION_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "moderation_safety_state",
    collection: "litd_project_moderation_safety",
    row: b"mcp_project/moderation_safety/current",
    column: b"moderation_safety_state_json",
    format: "litd-moderation-safety-state-json",
};

pub fn load_or_create_moderation_state(
    path: impl AsRef<Path>,
    state_id: &str,
    social_state_path: &str,
    catalog_path: &str,
    multiplayer_state_path: &str,
) -> Result<ModerationSafetyState, EngineError> {
    let path = path.as_ref();
    validate_path("moderation_state_path", path)?;
    if let Some(readback) =
        read_json_record_optional(path, &MODERATION_SPEC, validate_moderation_state)?
    {
        let state = readback.value;
        if state.state_id != state_id
            || state.social_state_path != social_state_path
            || state.catalog_path != catalog_path
            || state.multiplayer_state_path != multiplayer_state_path
        {
            return Err(moderation_error(
                "state.identity",
                "moderation state identity or source path mismatch",
            ));
        }
        Ok(state)
    } else {
        let state = ModerationSafetyState {
            schema: MODERATION_SAFETY_STATE_SCHEMA.to_string(),
            issue: 1163,
            state_id: state_id.to_string(),
            source_of_truth: MODERATION_SOURCE_OF_TRUTH.to_string(),
            social_state_path: social_state_path.to_string(),
            catalog_path: catalog_path.to_string(),
            multiplayer_state_path: multiplayer_state_path.to_string(),
            report_count: 0,
            role_assignment_count: 0,
            admin_action_count: 0,
            appeal_count: 0,
            access_rule_count: 0,
            connection_decision_count: 0,
            host_control_count: 0,
            roleplay_policy_count: 0,
            roleplay_evaluation_count: 0,
            open_report_count: 0,
            active_restriction_count: 0,
            allow_rule_count: 0,
            ban_rule_count: 0,
            active_roleplay_policy_count: 0,
            reward_verdict_count: 0,
            punishment_verdict_count: 0,
            reports: Vec::new(),
            role_assignments: Vec::new(),
            admin_actions: Vec::new(),
            appeals: Vec::new(),
            access_rules: Vec::new(),
            connection_decisions: Vec::new(),
            host_controls: Vec::new(),
            roleplay_policies: Vec::new(),
            roleplay_evaluations: Vec::new(),
        };
        validate_moderation_state(&state)?;
        Ok(state)
    }
}

pub fn read_moderation_safety_state(
    path: impl AsRef<Path>,
) -> Result<ModerationSafetyStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("moderation_state_path", path)?;
    let readback = read_json_record(path, &MODERATION_SPEC, validate_moderation_state)?;
    Ok(ModerationSafetyStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}

pub fn write_moderation_safety_state(
    path: impl AsRef<Path>,
    state: &ModerationSafetyState,
) -> Result<ModerationSafetyStateReadback, EngineError> {
    validate_moderation_state(state)?;
    let path = path.as_ref();
    validate_path("moderation_state_path", path)?;
    let readback = write_json_record(path, &MODERATION_SPEC, state, validate_moderation_state)?;
    Ok(ModerationSafetyStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}
