use litd_hash::Fnv1a64;

use crate::{nav::NavCellCoord, sim::SimBodyId};

use super::{
    AiControllerAction, AiControllerDecision, AiControllerRunReport, blackboard::update_str,
};

pub fn compute_controller_run_fnv1a64(report: &AiControllerRunReport) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &report.schema);
    hasher.update_u64_le(report.tick);
    update_str(&mut hasher, &report.perception_fnv1a64);
    update_str(&mut hasher, &report.navmesh_fnv1a64);
    hasher.update_u64_le(report.agent_count as u64);
    hasher.update_u64_le(report.order_count as u64);
    hasher.update_u64_le(report.decisions.len() as u64);
    for decision in &report.decisions {
        hash_decision(&mut hasher, decision);
    }
    hasher.update_u64_le(report.orders.len() as u64);
    for order in &report.orders {
        hasher.update_u64_le(order.fingerprint());
    }
    hasher.finish_hex()
}

fn hash_decision(hasher: &mut Fnv1a64, decision: &AiControllerDecision) {
    update_str(hasher, &decision.agent_id);
    hasher.update_u64_le(decision.source_id as u64);
    hash_body(hasher, decision.body_id);
    hash_action(hasher, decision.action);
    hash_optional_text(hasher, &decision.selected_binding);
    hash_optional_text(hasher, &decision.target_enemy_id);
    hash_optional_body(hasher, decision.target_body_id);
    hash_optional_text(hasher, &decision.path_fnv1a64);
    hash_optional_text(hasher, &decision.avoidance_fnv1a64);
    update_str(hasher, &decision.blackboard_fnv1a64);
    update_str(hasher, &decision.behavior_tick.fnv1a64);
    hasher.update_u64_le(decision.order_count as u64);
    hasher.update_u64_le(decision.order_fingerprints.len() as u64);
    for fingerprint in &decision.order_fingerprints {
        update_str(hasher, fingerprint);
    }
}

fn hash_action(hasher: &mut Fnv1a64, action: AiControllerAction) {
    hasher.update_u8(match action {
        AiControllerAction::Attack => 1,
        AiControllerAction::Chase => 2,
        AiControllerAction::Patrol => 3,
        AiControllerAction::Idle => 4,
    });
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

fn hash_optional_body(hasher: &mut Fnv1a64, value: Option<SimBodyId>) {
    match value {
        Some(value) => {
            hasher.update_u8(1);
            hash_body(hasher, value);
        }
        None => {
            hasher.update_u8(0);
        }
    }
}

fn hash_body(hasher: &mut Fnv1a64, body_id: SimBodyId) {
    hasher.update_u64_le(body_id.index() as u64);
}

#[allow(dead_code)]
fn hash_coord(hasher: &mut Fnv1a64, coord: NavCellCoord) {
    hasher.update_i64_le(coord.x as i64);
    hasher.update_i64_le(coord.z as i64);
}
