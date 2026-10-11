use litd_engine::EngineError;

use crate::starter_templates::{StarterTemplateClone, StarterTerminalResult, starter_error};

use super::{StarterSimState, arena_target, damage_unit, ids_by_owner, ids_by_role, terminal};

pub(super) fn tick_template(
    state: &mut StarterSimState,
    clone: &StarterTemplateClone,
) -> Result<(), EngineError> {
    match clone.template_id.as_str() {
        "starter.tower_defense.cinder_lane" | "starter.survival_chaos.obsidian_lanes" => {
            tick_tower_defense(state, clone)
        }
        "starter.melee_skirmish.frost_crossing" | "starter.moba_aos.dawnward_crossing" => {
            tick_melee(state, clone)
        }
        "starter.rpg_arena.crypt_ring"
        | "starter.orpg.silverleaf_chronicle"
        | "starter.hero_arena.sunspire_duel"
        | "starter.hero_defense.last_brazier"
        | "starter.mini_game_party.crystal_dash"
        | "starter.maze_escape.gloomroot_path" => tick_arena(state, clone),
        other => starter_error("run.template_id", format!("unknown template {other}")),
    }
}

fn tick_tower_defense(
    state: &mut StarterSimState,
    clone: &StarterTemplateClone,
) -> Result<(), EngineError> {
    let tower_ids = ids_by_role(state, "tower", true);
    for tower_id in tower_ids {
        let period = state.units[&tower_id].attack_period_ticks;
        if state.tick.is_multiple_of(period)
            && let Some(creep_id) = ids_by_role(state, "creep", true).first().cloned()
        {
            damage_unit(state, clone, &creep_id, state.units[&tower_id].damage)?;
        }
    }
    for creep_id in ids_by_role(state, "creep", true) {
        let creep = state.units.get_mut(&creep_id).unwrap();
        creep.path_progress += creep.speed_per_tick;
        if creep.path_progress >= creep.path_length {
            creep.alive = false;
            let leak_damage = creep.leak_damage;
            if let Some(base_id) = ids_by_role(state, "base", true).first().cloned() {
                state.units.get_mut(&base_id).unwrap().health -= leak_damage;
            }
        }
    }
    let base_health = ids_by_role(state, "base", true)
        .first()
        .and_then(|id| state.units.get(id))
        .map(|unit| unit.health)
        .unwrap_or(0);
    if base_health <= 0 {
        terminal(state, clone, StarterTerminalResult::Loss, "base destroyed");
    } else if ids_by_role(state, "creep", true).is_empty() {
        terminal(
            state,
            clone,
            StarterTerminalResult::Win,
            "all creeps defeated",
        );
    }
    Ok(())
}

fn tick_melee(
    state: &mut StarterSimState,
    clone: &StarterTemplateClone,
) -> Result<(), EngineError> {
    for attacker_id in ids_by_role(state, "army", true) {
        let attacker = state.units[&attacker_id].clone();
        if !state.tick.is_multiple_of(attacker.attack_period_ticks) {
            continue;
        }
        let Some(target_id) = ids_by_owner(state, &attacker.owner, false).first().cloned() else {
            continue;
        };
        damage_unit(state, clone, &target_id, attacker.damage)?;
    }
    let player_alive = ids_by_owner(state, "player_one", true).len();
    let enemy_alive = state
        .units
        .values()
        .filter(|unit| unit.alive && unit.owner != "player_one")
        .count();
    if player_alive == 0 {
        terminal(
            state,
            clone,
            StarterTerminalResult::Loss,
            "player army defeated",
        );
    } else if enemy_alive == 0 {
        terminal(
            state,
            clone,
            StarterTerminalResult::Win,
            "opposing army defeated",
        );
    }
    Ok(())
}

fn tick_arena(
    state: &mut StarterSimState,
    clone: &StarterTemplateClone,
) -> Result<(), EngineError> {
    if let Some(hero_id) = ids_by_role(state, "hero", true).first().cloned() {
        let hero = state.units[&hero_id].clone();
        if state.tick.is_multiple_of(hero.attack_period_ticks)
            && let Some(target_id) = arena_target(state)
        {
            damage_unit(state, clone, &target_id, hero.damage)?;
        }
    }
    for enemy_id in ids_by_role(state, "monster", true)
        .into_iter()
        .chain(ids_by_role(state, "boss", true))
        .collect::<Vec<_>>()
    {
        let enemy = state.units[&enemy_id].clone();
        if state.tick.is_multiple_of(enemy.attack_period_ticks)
            && let Some(hero_id) = ids_by_role(state, "hero", true).first().cloned()
        {
            damage_unit(state, clone, &hero_id, enemy.damage)?;
        }
    }
    if ids_by_role(state, "hero", true).is_empty() {
        terminal(state, clone, StarterTerminalResult::Loss, "hero defeated");
    } else if ids_by_role(state, "boss", true).is_empty() {
        terminal(state, clone, StarterTerminalResult::Win, "boss defeated");
    }
    Ok(())
}
