use std::collections::{BTreeMap, BTreeSet};

use crate::{EngineError, sim::SimBodyState};

use super::{
    PerceptionAgentInput, PerceptionBlocker, PerceptionEntityInput,
    blackboard::{invalid_config, validate_id},
};

pub(super) fn validate_inputs(
    agents: &[PerceptionAgentInput],
    entities: &[PerceptionEntityInput],
    blockers: &[PerceptionBlocker],
) -> Result<(), EngineError> {
    if agents.is_empty() {
        return invalid_config("agents", "must contain at least one sensing agent");
    }
    if entities.is_empty() {
        return invalid_config("entities", "must contain at least one sensed entity");
    }
    let mut agent_ids = BTreeSet::new();
    for agent in agents {
        validate_id("agents.id", &agent.id)?;
        validate_id("agents.faction", &agent.faction)?;
        if !agent_ids.insert(agent.id.as_str()) {
            return invalid_config("agents.id", format!("duplicate id {}", agent.id));
        }
        validate_ranges(agent)?;
    }
    let mut entity_ids = BTreeSet::new();
    let mut entity_body_ids = BTreeSet::new();
    for entity in entities {
        validate_id("entities.id", &entity.id)?;
        validate_id("entities.faction", &entity.faction)?;
        if !entity_ids.insert(entity.id.as_str()) {
            return invalid_config("entities.id", format!("duplicate id {}", entity.id));
        }
        if !entity_body_ids.insert(entity.body_id.index()) {
            return invalid_config("entities.body_id", "duplicate entity body id");
        }
    }
    let mut blocker_ids = BTreeSet::new();
    for blocker in blockers {
        validate_id("blockers.id", &blocker.id)?;
        if !blocker_ids.insert(blocker.id.as_str()) {
            return invalid_config("blockers.id", format!("duplicate id {}", blocker.id));
        }
        if blocker.min.x > blocker.max.x || blocker.min.z > blocker.max.z {
            return invalid_config("blockers.bounds", "min cell must be <= max cell");
        }
    }
    Ok(())
}

pub(super) fn body_state_index(
    body_states: &[SimBodyState],
) -> Result<BTreeMap<u32, SimBodyState>, EngineError> {
    let mut states = BTreeMap::new();
    for state in body_states {
        if states.insert(state.id.index(), *state).is_some() {
            return invalid_config("body_states.id", "duplicate sim body id in snapshot");
        }
    }
    Ok(states)
}

fn validate_ranges(agent: &PerceptionAgentInput) -> Result<(), EngineError> {
    if agent.sight_range_fp < 0 || agent.attack_range_fp < 0 || agent.threat_range_fp < 0 {
        return invalid_config(
            "agents.ranges",
            "ranges must be non-negative fixed-point values",
        );
    }
    if agent.attack_range_fp > agent.sight_range_fp {
        return invalid_config(
            "agents.attack_range_fp",
            "attack range cannot exceed sight range",
        );
    }
    Ok(())
}
