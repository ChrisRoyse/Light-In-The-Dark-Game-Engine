use litd_hash::Fnv1a64;

use crate::{math::FixedVec3, nav::NavCellCoord};

use super::{
    AgentPerception, PerceptionAgent, PerceptionBlocker, PerceptionEntity, PerceptionRunReport,
    PerceptionSnapshot, blackboard::hash_status, blackboard::update_str,
    compute_blackboard_fnv1a64,
};

pub fn compute_snapshot_fnv1a64(snapshot: &PerceptionSnapshot) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &snapshot.schema);
    hasher.update_u64_le(snapshot.tick);
    hasher.update_u64_le(snapshot.sim_state_hash);
    hasher.update_u64_le(snapshot.agents.len() as u64);
    for agent in &snapshot.agents {
        hash_agent(&mut hasher, agent);
    }
    hasher.update_u64_le(snapshot.entities.len() as u64);
    for entity in &snapshot.entities {
        hash_entity(&mut hasher, entity);
    }
    hasher.update_u64_le(snapshot.blockers.len() as u64);
    for blocker in &snapshot.blockers {
        hash_blocker(&mut hasher, blocker);
    }
    hasher.finish_hex()
}

pub fn compute_perception_run_fnv1a64(report: &PerceptionRunReport) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &report.schema);
    hasher.update_u64_le(report.tick);
    update_str(&mut hasher, &report.snapshot_fnv1a64);
    hasher.update_u64_le(report.sim_state_hash_before);
    hasher.update_u64_le(report.sim_state_hash_after);
    hasher.update_u8(u8::from(report.sim_state_unchanged));
    hasher.update_u64_le(report.agent_count as u64);
    hasher.update_u64_le(report.entity_count as u64);
    hasher.update_u64_le(report.blocker_count as u64);
    for agent in &report.agents {
        hash_agent_perception(&mut hasher, agent);
    }
    hasher.finish_hex()
}

fn hash_agent(hasher: &mut Fnv1a64, agent: &PerceptionAgent) {
    update_str(hasher, &agent.id);
    hasher.update_u64_le(agent.body_id.index() as u64);
    update_str(hasher, &agent.faction);
    hash_vec3(hasher, agent.position);
    hash_coord(hasher, agent.nav_cell);
    hasher.update_i64_le(agent.sight_range_fp);
    hasher.update_i64_le(agent.attack_range_fp);
    hasher.update_i64_le(agent.threat_range_fp);
}

fn hash_entity(hasher: &mut Fnv1a64, entity: &PerceptionEntity) {
    update_str(hasher, &entity.id);
    hasher.update_u64_le(entity.body_id.index() as u64);
    update_str(hasher, &entity.faction);
    hash_vec3(hasher, entity.position);
    hash_coord(hasher, entity.nav_cell);
    hasher.update_u8(u8::from(entity.alive));
}

fn hash_agent_perception(hasher: &mut Fnv1a64, agent: &AgentPerception) {
    update_str(hasher, &agent.agent_id);
    hasher.update_u64_le(agent.body_id.index() as u64);
    hash_optional_text(hasher, &agent.nearest_enemy_id);
    hash_optional_i64(hasher, agent.nearest_enemy_distance_fp);
    hash_optional_text(hasher, &agent.nearest_visible_enemy_id);
    hash_optional_i64(hasher, agent.nearest_visible_enemy_distance_fp);
    for ids in [
        &agent.enemies_in_sight_range,
        &agent.enemies_in_attack_range,
        &agent.enemies_with_line_of_sight,
        &agent.threat_enemy_ids,
    ] {
        hasher.update_u64_le(ids.len() as u64);
        for id in ids {
            update_str(hasher, id);
        }
    }
    hasher.update_u64_le(agent.enemies.len() as u64);
    for enemy in &agent.enemies {
        update_str(hasher, &enemy.id);
        hasher.update_u64_le(enemy.body_id.index() as u64);
        hash_coord(hasher, enemy.nav_cell);
        hasher.update_i64_le(enemy.distance_fp);
        hasher.update_u8(u8::from(enemy.in_sight_range));
        hasher.update_u8(u8::from(enemy.in_attack_range));
        hasher.update_u8(u8::from(enemy.has_line_of_sight));
        hasher.update_u8(u8::from(enemy.threat));
    }
    update_str(hasher, &compute_blackboard_fnv1a64(&agent.blackboard));
    for status in agent.blackboard.leaf_state.conditions.values() {
        hash_status(hasher, *status);
    }
}

fn hash_blocker(hasher: &mut Fnv1a64, blocker: &PerceptionBlocker) {
    update_str(hasher, &blocker.id);
    hash_coord(hasher, blocker.min);
    hash_coord(hasher, blocker.max);
}

fn hash_vec3(hasher: &mut Fnv1a64, value: FixedVec3) {
    hasher.update_i64_le(value.x.raw());
    hasher.update_i64_le(value.y.raw());
    hasher.update_i64_le(value.z.raw());
}

fn hash_coord(hasher: &mut Fnv1a64, coord: NavCellCoord) {
    hasher.update_i64_le(coord.x as i64);
    hasher.update_i64_le(coord.z as i64);
}

fn hash_optional_text(hasher: &mut Fnv1a64, value: &Option<String>) {
    match value {
        Some(value) => {
            hasher.update_u8(1);
            update_str(hasher, value);
        }
        None => {
            hasher.update_u8(0);
        }
    }
}

fn hash_optional_i64(hasher: &mut Fnv1a64, value: Option<i64>) {
    match value {
        Some(value) => {
            hasher.update_u8(1);
            hasher.update_i64_le(value);
        }
        None => {
            hasher.update_u8(0);
        }
    }
}
