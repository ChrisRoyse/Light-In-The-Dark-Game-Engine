use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;

use crate::history::stable_fingerprint;
use crate::mcp_project_tools::mcp_project_tools_validation::{confine_path, resolve_project_root};

use super::super::computer_opponents_types::{
    COMPUTER_OPPONENT_ISSUE, COMPUTER_OPPONENT_LEDGER_SCHEMA, ComputerOpponentAuthoringArgs,
    ComputerOpponentDefinition, ComputerOpponentRunLedger, ComputerOpponentRuntimeArgs,
    ComputerOpponentScript,
};

const PRESET_MELEE_GUARDIAN: &str = "melee_guardian";

pub(crate) fn validate_authoring_args(
    args: &ComputerOpponentAuthoringArgs,
) -> Result<(), EngineError> {
    validate_path("state_path", &args.state_path)?;
    validate_path("ledger_dir", &args.ledger_dir)?;
    // Confine tool-supplied paths within the trusted project root before any fs effect
    // (#1518): authoring writes `state_path` and a ledger under `ledger_dir`, both from
    // arguments. Same policy as `confine_delivery_paths`.
    let project_root = resolve_project_root()?;
    confine_path("state_path", &args.state_path, &project_root)?;
    confine_path("ledger_dir", &args.ledger_dir, &project_root)?;
    validate_id("state_id", &args.state_id)?;
    validate_id("requested_by", &args.requested_by)?;
    validate_opponents(&args.opponents)
}

pub(crate) fn validate_runtime_args(args: &ComputerOpponentRuntimeArgs) -> Result<(), EngineError> {
    validate_id("run_id", &args.run_id)?;
    validate_path("state_path", &args.state_path)?;
    validate_path("controller_run_path", &args.controller_run_path)?;
    validate_path("ledger_path", &args.ledger_path)?;
    // Confine tool-supplied paths within the trusted project root before any fs effect
    // (#1518): runtime reads `state_path` and writes `controller_run_path` + `ledger_path`.
    let project_root = resolve_project_root()?;
    confine_path("state_path", &args.state_path, &project_root)?;
    confine_path(
        "controller_run_path",
        &args.controller_run_path,
        &project_root,
    )?;
    confine_path("ledger_path", &args.ledger_path, &project_root)?;
    validate_opponents(&args.opponents)?;
    if args.objects.is_empty() {
        return Err(invalid_opponent(
            "objects",
            "at least one opponent object is required",
        ));
    }
    Ok(())
}

pub(crate) fn validate_opponents(
    opponents: &[ComputerOpponentDefinition],
) -> Result<(), EngineError> {
    if opponents.is_empty() {
        return Err(invalid_opponent(
            "opponents",
            "at least one opponent is required",
        ));
    }
    let mut players = BTreeSet::new();
    let mut agents = BTreeSet::new();
    let mut objects = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for opponent in opponents {
        if !players.insert(opponent.player_id) {
            return Err(invalid_opponent(
                "opponent.player_id",
                "duplicate player id",
            ));
        }
        validate_id("opponent.agent_id", &opponent.agent_id)?;
        validate_id("opponent.object_id", &opponent.object_id)?;
        validate_id("opponent.faction", &opponent.faction)?;
        validate_id("opponent.region_id", &opponent.region_id)?;
        validate_id("opponent.script.script_id", &opponent.script.script_id)?;
        if !agents.insert(opponent.agent_id.as_str())
            || !objects.insert(opponent.object_id.as_str())
            || !sources.insert(opponent.source_id)
        {
            return Err(invalid_opponent(
                "opponents",
                "agent/object/source ids must be unique",
            ));
        }
        validate_script(&opponent.script)?;
    }
    Ok(())
}

pub(crate) fn validate_ledger(ledger: &ComputerOpponentRunLedger) -> Result<(), EngineError> {
    let mut expected = ledger.clone();
    let fingerprint = std::mem::take(&mut expected.ledger_fingerprint);
    if expected.schema != COMPUTER_OPPONENT_LEDGER_SCHEMA
        || expected.issue != COMPUTER_OPPONENT_ISSUE
        || expected.controller_run_path.trim().is_empty()
        || expected.opponent_count == 0
        || expected.decision_count != expected.decisions.len()
        || expected.order_count
            != expected
                .decisions
                .iter()
                .map(|row| row.order_count)
                .sum::<usize>()
        || fingerprint.len() != 16
    {
        return Err(invalid_opponent(
            "ledger",
            "schema, counts, or fingerprint invalid",
        ));
    }
    let actual = stable_fingerprint(&expected)?;
    if actual != fingerprint {
        return Err(invalid_opponent(
            "ledger.fingerprint",
            "fingerprint mismatch",
        ));
    }
    Ok(())
}

pub(crate) fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_opponent(field, "path must be non-empty"));
    }
    Ok(())
}

pub(crate) fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(invalid_opponent(
            field,
            "must use stable ASCII id characters",
        ));
    }
    Ok(())
}

pub(crate) fn invalid_opponent(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::AiInvalidConfig {
        field: format!("computer_opponents.{}", field.into()),
        reason: reason.into(),
    }
}

fn validate_script(script: &ComputerOpponentScript) -> Result<(), EngineError> {
    if script.behavior_preset != PRESET_MELEE_GUARDIAN {
        return Err(invalid_opponent(
            "script.behavior_preset",
            "unsupported behavior preset",
        ));
    }
    for value in [
        &script.prompt,
        &script.model_id,
        &script.personality,
        &script.script_id,
    ] {
        if value.trim().is_empty() {
            return Err(invalid_opponent(
                "script",
                "script text fields must be non-empty",
            ));
        }
    }
    if script.path_max_expansions == 0
        || script.attack_damage_fp <= 0
        || script.sight_range_cells <= 0
        || script.attack_range_cells <= 0
        || script.threat_range_cells <= 0
    {
        return Err(invalid_opponent(
            "script",
            "ranges, path budget, and damage must be positive",
        ));
    }
    Ok(())
}
