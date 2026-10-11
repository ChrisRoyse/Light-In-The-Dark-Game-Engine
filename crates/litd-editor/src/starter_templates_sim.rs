use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::{PlayCameraRig, PlaySession},
    starter_templates::{StarterTemplateClone, starter_error},
};

#[path = "starter_templates_sim_modes.rs"]
mod modes;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarterTerminalResult {
    Win,
    Loss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterTemplateRunReport {
    pub template_id: String,
    pub project_id: String,
    pub terminal_result: StarterTerminalResult,
    pub terminal_reason: String,
    pub ticks_requested: u64,
    pub final_tick: u64,
    pub final_state_hash: String,
    pub replay_state_hash: String,
    pub deterministic: bool,
    pub executed_trigger_ids: Vec<String>,
    pub play_initial_state_hash: u64,
    pub play_spawned_bodies: usize,
    pub visible_instance_count: usize,
    pub draw_batches: usize,
    pub alive_player_units: usize,
    pub alive_enemy_units: usize,
    pub base_health: i64,
    pub resource_total: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct StarterSimState {
    template_id: String,
    tick: u64,
    terminal: Option<StarterTerminalResult>,
    terminal_reason: String,
    units: BTreeMap<String, SimUnit>,
    resources: BTreeMap<String, i64>,
    executed_trigger_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SimUnit {
    id: String,
    role: String,
    owner: String,
    alive: bool,
    health: i64,
    damage: i64,
    attack_period_ticks: u64,
    path_progress: i64,
    path_length: i64,
    speed_per_tick: i64,
    leak_damage: i64,
    bounty: i64,
}

struct SingleRun {
    state: StarterSimState,
    state_hash: String,
    play_initial_state_hash: u64,
    play_spawned_bodies: usize,
    visible_instance_count: usize,
    draw_batches: usize,
}

pub fn run_starter_template_headless(
    clone: &StarterTemplateClone,
    ticks: u64,
) -> Result<StarterTemplateRunReport, EngineError> {
    if !(1..=36_000).contains(&ticks) {
        return starter_error("run.ticks", "ticks must be in 1..=36000");
    }
    clone.project.document.validate()?;
    clone.trigger_project.validate()?;
    let first = run_once(clone, ticks)?;
    let replay = run_once(clone, ticks)?;
    let deterministic = first.state_hash == replay.state_hash
        && first.state.terminal == replay.state.terminal
        && first.state.tick == replay.state.tick
        && first.state.executed_trigger_ids == replay.state.executed_trigger_ids;
    if !deterministic {
        return starter_error("run.determinism", "starter template replay diverged");
    }
    let terminal_result = first.state.terminal.ok_or_else(|| {
        starter_error::<()>("run.terminal", "run did not reach terminal result").unwrap_err()
    })?;
    let alive_player_units = first
        .state
        .units
        .values()
        .filter(|unit| unit.alive && unit.owner == "player_one")
        .count();
    let alive_enemy_units = first
        .state
        .units
        .values()
        .filter(|unit| unit.alive && unit.owner != "player_one")
        .count();
    let base_health = first
        .state
        .units
        .values()
        .find(|unit| unit.role == "base")
        .map(|unit| unit.health)
        .unwrap_or(0);
    let resource_total = first.state.resources.values().copied().sum::<i64>();
    Ok(StarterTemplateRunReport {
        template_id: clone.template_id.clone(),
        project_id: clone.project.project_id.clone(),
        terminal_result,
        terminal_reason: first.state.terminal_reason,
        ticks_requested: ticks,
        final_tick: first.state.tick,
        final_state_hash: first.state_hash,
        replay_state_hash: replay.state_hash,
        deterministic,
        executed_trigger_ids: first.state.executed_trigger_ids.into_iter().collect(),
        play_initial_state_hash: first.play_initial_state_hash,
        play_spawned_bodies: first.play_spawned_bodies,
        visible_instance_count: first.visible_instance_count,
        draw_batches: first.draw_batches,
        alive_player_units,
        alive_enemy_units,
        base_health,
        resource_total,
    })
}

fn run_once(clone: &StarterTemplateClone, ticks: u64) -> Result<SingleRun, EngineError> {
    let play = PlaySession::start(&clone.project.document, PlayCameraRig::TopDown)?;
    let mut state = initial_state(&clone.template_id, &clone.project.document)?;
    fire_event(&mut state, clone, "map_initialization", None);
    for tick in 1..=ticks {
        state.tick = tick;
        modes::tick_template(&mut state, clone)?;
        if state.terminal.is_some() {
            break;
        }
    }
    if state.terminal.is_none() {
        return starter_error(
            "run.terminal",
            format!(
                "{} did not reach terminal result in {ticks} ticks",
                clone.template_id
            ),
        );
    }
    let state_hash = stable_fingerprint(&state)?;
    Ok(SingleRun {
        state,
        state_hash,
        play_initial_state_hash: play.build.snapshot.state_hash,
        play_spawned_bodies: play.build.spawned_bodies.len(),
        visible_instance_count: play.build.visible_instance_ids.len(),
        draw_batches: play.build.draw_batches,
    })
}

fn initial_state(
    template_id: &str,
    document: &EditorDocument,
) -> Result<StarterSimState, EngineError> {
    let mut units = BTreeMap::new();
    for object in document
        .objects
        .values()
        .filter(|object| object.kind == "Unit")
    {
        let role = required_prop(document, &object.id, "template.role")?;
        let unit = SimUnit {
            id: object.id.clone(),
            role,
            owner: required_prop(document, &object.id, "template.owner_player_id")?,
            alive: true,
            health: stat(document, &object.id, "stat.health", 1)?,
            damage: stat(document, &object.id, "stat.damage", 0)?,
            attack_period_ticks: stat(document, &object.id, "stat.attack_period_ticks", 1)? as u64,
            path_progress: 0,
            path_length: stat(document, &object.id, "stat.path_length", 0)?,
            speed_per_tick: stat(document, &object.id, "stat.speed_per_tick", 0)?,
            leak_damage: stat(document, &object.id, "stat.leak_damage", 0)?,
            bounty: stat(document, &object.id, "stat.bounty", 0)?,
        };
        if unit.damage > 0 && unit.attack_period_ticks == 0 {
            return starter_error(
                "run.attack_period_ticks",
                format!("{} attack period must be positive", unit.id),
            );
        }
        units.insert(unit.id.clone(), unit);
    }
    if units.is_empty() {
        return starter_error("run.units", "starter simulation requires at least one Unit");
    }
    Ok(StarterSimState {
        template_id: template_id.to_string(),
        tick: 0,
        terminal: None,
        terminal_reason: String::new(),
        units,
        resources: BTreeMap::from([("player_one".to_string(), 0)]),
        executed_trigger_ids: BTreeSet::new(),
    })
}

pub(super) fn damage_unit(
    state: &mut StarterSimState,
    clone: &StarterTemplateClone,
    unit_id: &str,
    damage: i64,
) -> Result<(), EngineError> {
    if damage <= 0 {
        return starter_error("run.damage", "damage must be positive");
    }
    let mut bounty = 0;
    let mut died = false;
    {
        let unit = state.units.get_mut(unit_id).ok_or_else(|| {
            starter_error::<()>("run.unit", format!("missing unit {unit_id}")).unwrap_err()
        })?;
        if !unit.alive {
            return Ok(());
        }
        unit.health -= damage;
        if unit.health <= 0 {
            unit.alive = false;
            bounty = unit.bounty;
            died = true;
        }
    }
    if died {
        if bounty > 0 {
            *state.resources.entry("player_one".to_string()).or_insert(0) += bounty;
        }
        fire_event(state, clone, "unit_dies", Some(unit_id));
    }
    Ok(())
}

pub(super) fn terminal(
    state: &mut StarterSimState,
    clone: &StarterTemplateClone,
    result: StarterTerminalResult,
    reason: &str,
) {
    let event = match result {
        StarterTerminalResult::Win => "game_victory",
        StarterTerminalResult::Loss => "game_defeat",
    };
    state.terminal = Some(result);
    state.terminal_reason = reason.to_string();
    fire_event(state, clone, event, None);
}

fn fire_event(
    state: &mut StarterSimState,
    clone: &StarterTemplateClone,
    operation: &str,
    unit_id: Option<&str>,
) {
    for graph in &clone.trigger_project.graphs {
        let matches = graph.events.iter().any(|event| {
            event.operation == operation
                && unit_id
                    .map(|id| event.args.get("unit_id").map(String::as_str) == Some(id))
                    .unwrap_or(true)
        });
        if matches {
            state.executed_trigger_ids.insert(graph.trigger_id.clone());
        }
    }
}

pub(super) fn arena_target(state: &StarterSimState) -> Option<String> {
    ids_by_role(state, "monster", true)
        .first()
        .cloned()
        .or_else(|| ids_by_role(state, "boss", true).first().cloned())
}

pub(super) fn ids_by_role(state: &StarterSimState, role: &str, alive: bool) -> Vec<String> {
    let mut ids = state
        .units
        .values()
        .filter(|unit| unit.role == role && (!alive || unit.alive))
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

pub(super) fn ids_by_owner(state: &StarterSimState, owner: &str, same_owner: bool) -> Vec<String> {
    let mut ids = state
        .units
        .values()
        .filter(|unit| {
            unit.alive
                && if same_owner {
                    unit.owner == owner
                } else {
                    unit.owner != owner
                }
        })
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn required_prop(
    document: &EditorDocument,
    object_id: &str,
    key: &str,
) -> Result<String, EngineError> {
    document
        .objects
        .get(object_id)
        .and_then(|object| object.properties.get(key))
        .cloned()
        .ok_or_else(|| {
            starter_error::<()>("run.property", format!("{object_id} missing {key}")).unwrap_err()
        })
}

fn stat(
    document: &EditorDocument,
    object_id: &str,
    key: &str,
    default: i64,
) -> Result<i64, EngineError> {
    let Some(value) = document
        .objects
        .get(object_id)
        .and_then(|object| object.properties.get(key))
    else {
        return Ok(default);
    };
    let parsed = value.parse::<i64>().map_err(|_| {
        starter_error::<()>("run.stat", format!("{object_id}.{key} must be an integer"))
            .unwrap_err()
    })?;
    if parsed < 0 {
        return starter_error("run.stat", format!("{object_id}.{key} cannot be negative"));
    }
    Ok(parsed)
}
