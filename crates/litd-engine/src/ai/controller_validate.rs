use std::collections::BTreeSet;

use crate::{
    EngineError,
    nav::{BakedNavMesh, NavCellCoord},
    sim_order::SimOrderSourceKind,
};

use super::{
    AI_CONTROLLER_RUN_SCHEMA, AiControllerAction, AiControllerActionBindings,
    AiControllerAgentInput, AiControllerDecision, AiControllerRunReport,
    controller_hash::compute_controller_run_fnv1a64, verify_behavior_tick_integrity,
};

pub fn validate_controller_inputs(
    navmesh: &BakedNavMesh,
    perception: &super::PerceptionRunReport,
    agents: &[AiControllerAgentInput],
) -> Result<(), EngineError> {
    if agents.is_empty() {
        return invalid_config(
            "controller.agents",
            "must contain at least one controller agent",
        );
    }
    let mut ids = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for input in agents {
        validate_id("controller.agent.id", &input.id)?;
        validate_bindings(&input.action_bindings)?;
        if !ids.insert(input.id.as_str()) {
            return invalid_config("controller.agent.id", format!("duplicate id {}", input.id));
        }
        if !sources.insert(input.source_id) {
            return invalid_config(
                "controller.agent.source_id",
                format!("duplicate source_id {}", input.source_id),
            );
        }
        if input.path_max_expansions == 0 {
            return invalid_config(
                "controller.path_max_expansions",
                "must be greater than zero",
            );
        }
        if input.attack_damage_fp <= 0 {
            return invalid_config("controller.attack_damage_fp", "must be positive");
        }
        validate_nav_cell(navmesh, input.nav_cell, &input.id)?;
        let sensed = perception
            .agents
            .iter()
            .find(|agent| agent.agent_id == input.id)
            .ok_or_else(|| EngineError::AiInvalidConfig {
                field: format!("controller.agents.{}.perception", input.id),
                reason: "matching perception agent is required".to_string(),
            })?;
        if sensed.body_id != input.body_id {
            return invalid_config(
                format!("controller.agents.{}.body_id", input.id),
                "must match perception body_id",
            );
        }
    }
    Ok(())
}

pub fn verify_ai_controller_run_integrity(
    report: &AiControllerRunReport,
) -> Result<(), EngineError> {
    if report.schema != AI_CONTROLLER_RUN_SCHEMA {
        return invalid_config("controller.schema", "unsupported controller schema");
    }
    if report.agent_count != report.decisions.len() || report.decisions.is_empty() {
        return invalid_config("controller.agent_count", "must match non-empty decisions");
    }
    if report.order_count != report.orders.len() {
        return invalid_config("controller.order_count", "must match emitted orders");
    }
    verify_order_contract(report)?;
    for decision in &report.decisions {
        verify_behavior_tick_integrity(&decision.behavior_tick)?;
        if decision.order_count != decision.order_fingerprints.len() {
            return invalid_config(
                "controller.decision.order_count",
                "must match decision order fingerprints",
            );
        }
        verify_decision_action(decision)?;
    }
    let actual = compute_controller_run_fnv1a64(report);
    if report.fnv1a64 != actual {
        return Err(EngineError::AiReadbackMismatch {
            field: "controller_run.fnv1a64".to_string(),
            expected: report.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

fn verify_order_contract(report: &AiControllerRunReport) -> Result<(), EngineError> {
    let execute_tick = report
        .tick
        .checked_add(1)
        .ok_or(EngineError::SimTickOverflow)?;
    for order in &report.orders {
        if order.execute_tick != execute_tick {
            return invalid_config(
                "controller.orders.execute_tick",
                "must target the next tick",
            );
        }
        if order.source.kind != SimOrderSourceKind::LoweredAgent {
            return invalid_config("controller.orders.source", "must be lowered_agent");
        }
    }
    Ok(())
}

fn verify_decision_action(decision: &AiControllerDecision) -> Result<(), EngineError> {
    match decision.action {
        AiControllerAction::Attack => {
            if decision.target_body_id.is_none() || decision.order_count != 2 {
                return invalid_config(
                    "controller.decision.attack",
                    "must target and emit stop+attack",
                );
            }
        }
        AiControllerAction::Chase => {
            if decision.target_body_id.is_none()
                || decision.path_fnv1a64.is_none()
                || decision.avoidance_fnv1a64.is_none()
                || decision.order_count != 1
            {
                return invalid_config(
                    "controller.decision.chase",
                    "must target, path, resolve avoidance, and emit velocity",
                );
            }
        }
        AiControllerAction::Patrol | AiControllerAction::Idle => {
            if decision.order_count != 0 {
                return invalid_config("controller.decision.idle", "must not emit orders");
            }
        }
    }
    Ok(())
}

fn validate_bindings(bindings: &AiControllerActionBindings) -> Result<(), EngineError> {
    for (field, value) in [
        ("controller.bindings.attack", &bindings.attack),
        ("controller.bindings.chase", &bindings.chase),
        ("controller.bindings.patrol", &bindings.patrol),
    ] {
        validate_id(field, value)?;
    }
    let mut unique = BTreeSet::new();
    for value in [&bindings.attack, &bindings.chase, &bindings.patrol] {
        if !unique.insert(value.as_str()) {
            return invalid_config("controller.bindings", "bindings must be unique");
        }
    }
    Ok(())
}

fn validate_nav_cell(
    navmesh: &BakedNavMesh,
    coord: NavCellCoord,
    agent_id: &str,
) -> Result<(), EngineError> {
    let Some(cell) = navmesh
        .cells
        .iter()
        .find(|cell| cell.x == coord.x && cell.z == coord.z)
    else {
        return invalid_config(
            format!("controller.agents.{agent_id}.nav_cell"),
            "cell must exist in navmesh",
        );
    };
    if !cell.walkable {
        return invalid_config(
            format!("controller.agents.{agent_id}.nav_cell"),
            "cell must be walkable",
        );
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_config(field, "must be non-empty");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return invalid_config(field, "must contain only stable ASCII id characters");
    }
    Ok(())
}

pub(super) fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::AiInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}
