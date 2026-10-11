//! Support module for the #1851 FSV harness: real-valve authoring (map document +
//! MCP trigger tools over Calyx Aster), the deterministic order stream, the
//! lowering/tamper edge probes, and the certutil sha256 cross-check.

use std::{fs, path::Path, process::Command};

use litd_editor::asset_products::assign_vault_model_to_unit;
use litd_editor::asset_vault::AssetVaultCatalog;
use litd_editor::history::{EditorDocument, EditorObject};
use litd_editor::mcp_trigger_tools::{
    McpTriggerRuntimeState, UPSERT_ECA_GRAPH_TOOL, UPSERT_VARIABLE_TOOL, call_mcp_trigger_tool,
};
use litd_editor::quick_share::QuickSharePackage;
use litd_editor::trigger_runtime_lowering::lower_trigger_graphs_for_runtime;
use litd_engine::{
    EngineError,
    game_package::load_runtime_locked_package,
    game_runtime::{MatchRuntimeSession, RuntimeDatasetsManifest, SessionOrder},
    math::{Transform, Vec3},
};
use serde_json::{Value, json};

pub const SHARE_ID: &str = "issue1851eca";
pub const MAX_PACKAGE_BYTES: usize = 4 * 1024 * 1024;
pub const VAULT_MODEL: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
pub const STATE_ID: &str = "issue1851-state";
pub const LEDGER_ID: &str = "issue1851ledger";
pub const TICK_RATE: u32 = 4;
pub const TIMER_PERIOD_TICKS: u64 = 8;
pub const MATCH_TICKS: usize = 20;
pub const VICTORY_TRIGGER: u32 = 7;

/// Author the mission trigger graph through the REAL MCP trigger tools: one flag
/// variable and three triggers (init -> timer kill -> victory).
pub fn author_trigger_state(state_path: &Path, ledger_path: &Path) -> Result<(), EngineError> {
    call_mcp_trigger_tool(
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": state_path, "ledger_path": ledger_path,
            "state_id": STATE_ID, "ledger_id": LEDGER_ID,
            "request_id": "var-mission-started", "requested_by": "issue1851-fsv",
            "variable": {
                "name": "mission_started", "value_type": "Boolean", "scope": "Global",
                "is_array": false, "initial_value": "false",
            },
        }),
    )?;
    let upsert_graph = |request_id: &str, graph: Value| -> Result<(), EngineError> {
        call_mcp_trigger_tool(
            UPSERT_ECA_GRAPH_TOOL,
            json!({
                "state_path": state_path, "ledger_path": ledger_path,
                "state_id": STATE_ID, "ledger_id": LEDGER_ID,
                "request_id": request_id, "requested_by": "issue1851-fsv",
                "graph": graph,
            }),
        )
        .map(|_| ())
    };
    let node = |id: &str, op: &str, order: u32, args: Value| -> Value {
        json!({"node_id": id, "operation": op, "order": order, "args": args, "variable_refs": []})
    };
    upsert_graph(
        "graph-init",
        json!({
            "trigger_id": "trg_init", "display_name": "Arm the mission", "enabled": true,
            "folder_path": null, "initially_on": true, "comment": null, "wasm_escape": null,
            "events": [node("e_init", "map_initialization", 0, json!({}))],
            "conditions": [],
            "actions": [node("a_arm", "set_boolean_variable", 0,
                json!({"variable_name": "mission_started", "value": "true"}))],
        }),
    )?;
    upsert_graph(
        "graph-timer-kill",
        json!({
            "trigger_id": "trg_timer_kill", "display_name": "Countdown strike", "enabled": true,
            "folder_path": null, "initially_on": true, "comment": null, "wasm_escape": null,
            "events": [node("e_timer", "countdown_timer_expires", 0, json!({"timer_id": "0"}))],
            "conditions": [
                node("c_armed", "boolean_equal", 0,
                    json!({"left": "mission_started", "right": "true"})),
                node("c_alive", "unit_alive", 1, json!({"unit_id": "placed_soldier_p1"})),
            ],
            "actions": [node("a_kill", "kill_unit", 0,
                json!({"unit_id": "placed_soldier_p1"}))],
        }),
    )?;
    upsert_graph(
        "graph-victory",
        json!({
            "trigger_id": "trg_victory", "display_name": "Victory on kill", "enabled": true,
            "folder_path": null, "initially_on": true, "comment": null, "wasm_escape": null,
            "events": [node("e_dies", "unit_dies", 0, json!({"unit_id": "placed_soldier_p1"}))],
            "conditions": [],
            "actions": [node("a_win", "fire_trigger", 0,
                json!({"trigger": VICTORY_TRIGGER.to_string()}))],
        }),
    )?;
    Ok(())
}

/// Author the 2-player runtime map with the authored custom-trigger victory for
/// player_one (the #1851 fire_trigger target).
pub fn author_map() -> Result<EditorDocument, EngineError> {
    fn obj(id: &str, kind: &str, transform: Transform, props: &[(&str, &str)]) -> EditorObject {
        let mut object = EditorObject::new(id, kind, Some("world".to_string()), transform);
        for (key, value) in props {
            object = object.with_property(*key, *value);
        }
        object
    }
    let mut document = EditorDocument::new("calyx://world/issue1851-eca-triggers", "rev-1")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(obj(
        "runtime_config",
        "GameplayConstant",
        Transform::IDENTITY,
        &[
            ("runtime.tick_rate", "4"),
            ("runtime.worker_cells_per_sec", "8"),
            ("runtime.deposit_ticks", "1"),
            ("runtime.start_gold", "1000"),
            ("runtime.start_lumber", "1000"),
            ("runtime.upkeep_food_cap", "80"),
            ("runtime.day_length_ticks", "200"),
            ("runtime.base_sight_radius_mm", "6000"),
            ("runtime.night_sight_penalty_permille", "500"),
            ("runtime.fog_cell_size_mm", "1000"),
            ("runtime.fog_grid_width", "16"),
            ("runtime.fog_grid_height", "16"),
            ("runtime.match_seed", "1851"),
            ("runtime.custom_victory_player", "player_one"),
            ("runtime.custom_victory_trigger", "7"),
        ],
    ))?;
    document.upsert_object(obj(
        "type_worker",
        "Unit",
        Transform::IDENTITY,
        &[("runtime.role", "worker"), ("economy.gold_cost", "50")],
    ))?;
    document.upsert_object(obj(
        "gold_mine",
        "ResourceNode",
        Transform::from_translation(Vec3::new(5.0, 0.0, 3.0)),
        &[
            ("resource.key", "gold"),
            ("resource.amount_total", "5000"),
            ("resource.yield_per_trip", "10"),
        ],
    ))?;
    for (id, owner, role) in [
        ("placed_hall_p0", "player_one", "structure"),
        ("placed_hall_p1", "player_two", "structure"),
        ("placed_soldier_p0", "player_one", "soldier"),
        ("placed_soldier_p1", "player_two", "soldier"),
    ] {
        document.upsert_object(obj(
            id,
            "Unit",
            Transform::IDENTITY,
            &[("runtime.role", role), ("template.owner_player_id", owner)],
        ))?;
    }
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| record.relative_path == VAULT_MODEL)
        .cloned()
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: "issue1851.fsv.vault_model".to_string(),
            reason: format!("vault record {VAULT_MODEL} not found"),
        })?;
    for unit_id in [
        "type_worker",
        "placed_hall_p0",
        "placed_hall_p1",
        "placed_soldier_p0",
        "placed_soldier_p1",
    ] {
        assign_vault_model_to_unit(&mut document, unit_id, &record)?;
    }
    Ok(document)
}

/// Player 1's placed soldier: the unit the authored triggers kill.
pub fn enemy_soldier_handle(manifest: &RuntimeDatasetsManifest) -> Result<u32, EngineError> {
    let blueprint = manifest.lowered_match.verify()?;
    blueprint
        .units
        .iter()
        .find(|u| !u.is_structure && u.owner == 1)
        .map(|u| u.unit)
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: "issue1851.fsv.enemy_soldier".to_string(),
            reason: "no player-1 field unit in the blueprint".to_string(),
        })
}

/// The host order stream: start the countdown, then Ticks only -- every kill and
/// the victory come from the AUTHORED triggers, not scripted orders.
pub fn order_stream() -> Vec<SessionOrder> {
    let mut orders = vec![SessionOrder::StartTimer {
        one_shot: true,
        period_ticks: TIMER_PERIOD_TICKS,
    }];
    for _ in 0..MATCH_TICKS {
        orders.push(SessionOrder::Tick);
    }
    orders
}

pub fn run_hash_stream(
    manifest: &RuntimeDatasetsManifest,
    orders: &[SessionOrder],
) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
    let mut session = MatchRuntimeSession::from_manifest(manifest)?;
    let mut hashes = Vec::with_capacity(orders.len());
    for order in orders {
        session.apply(order.clone())?;
        hashes.push(session.combined_state_hash());
    }
    Ok(hashes)
}

pub fn save_load_acid(
    manifest: &RuntimeDatasetsManifest,
    orders: &[SessionOrder],
) -> Result<(u64, u64), Box<dyn std::error::Error>> {
    let midpoint = orders.len() / 2;
    let mut original = MatchRuntimeSession::from_manifest(manifest)?;
    for order in &orders[..midpoint] {
        original.apply(order.clone())?;
    }
    let save = original.capture_save()?;
    for order in &orders[midpoint..] {
        original.apply(order.clone())?;
    }
    let mut reloaded = MatchRuntimeSession::restore_from_save(manifest, &save)?;
    for order in &orders[midpoint..] {
        reloaded.apply(order.clone())?;
    }
    Ok((
        original.combined_state_hash(),
        reloaded.combined_state_hash(),
    ))
}

/// Clone the authored state, add one bad graph, and capture the typed lowering
/// rejection (code + whether the message names #1856).
pub fn lowering_edge(
    base: &McpTriggerRuntimeState,
    trigger_id: &str,
    event_op: &str,
    event_args: &[(&str, &str)],
) -> Result<(String, bool), Box<dyn std::error::Error>> {
    let mut state = base.clone();
    let mut graph: litd_editor::mcp_trigger_tools::EcaGraph = serde_json::from_value(json!({
        "trigger_id": trigger_id, "display_name": "edge", "enabled": true,
        "folder_path": null, "initially_on": true, "comment": null, "wasm_escape": null,
        "events": [], "conditions": [],
        "actions": [{"node_id": "a_edge", "operation": "fire_trigger", "order": 0,
                     "args": {"trigger": "1"}, "variable_refs": []}],
    }))?;
    graph.events.push(litd_editor::mcp_trigger_tools::EcaNode {
        node_id: "e_edge".to_string(),
        operation: event_op.to_string(),
        order: 0,
        args: event_args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        variable_refs: Vec::new(),
    });
    state.graphs.push(graph);
    let handles = [
        ("placed_hall_p0".to_string(), 1_u32),
        ("placed_hall_p1".to_string(), 2),
        ("placed_soldier_p0".to_string(), 3),
        ("placed_soldier_p1".to_string(), 4),
    ]
    .into_iter()
    .collect();
    match lower_trigger_graphs_for_runtime(
        &state,
        TICK_RATE,
        &handles,
        &std::collections::BTreeMap::new(),
    ) {
        Ok(_) => Ok(("ACCEPTED_SILENTLY".to_string(), false)),
        Err(error) => {
            let names_1856 = error.to_string().contains("1856");
            Ok((error.code().to_string(), names_1856))
        }
    }
}

/// A condition referencing an undeclared variable must be rejected at lowering.
pub fn ghost_variable_edge(
    base: &McpTriggerRuntimeState,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut state = base.clone();
    let graph: litd_editor::mcp_trigger_tools::EcaGraph = serde_json::from_value(json!({
        "trigger_id": "trg_ghost_var", "display_name": "edge", "enabled": true,
        "folder_path": null, "initially_on": true, "comment": null, "wasm_escape": null,
        "events": [{"node_id": "e_g", "operation": "map_initialization", "order": 0,
                    "args": {}, "variable_refs": []}],
        "conditions": [{"node_id": "c_g", "operation": "boolean_equal", "order": 0,
                        "args": {"left": "ghost_flag", "right": "true"},
                        "variable_refs": []}],
        "actions": [{"node_id": "a_g", "operation": "fire_trigger", "order": 0,
                     "args": {"trigger": "1"}, "variable_refs": []}],
    }))?;
    state.graphs.push(graph);
    let handles = std::collections::BTreeMap::new();
    match lower_trigger_graphs_for_runtime(
        &state,
        TICK_RATE,
        &handles,
        &std::collections::BTreeMap::new(),
    ) {
        Ok(_) => Ok("ACCEPTED_SILENTLY".to_string()),
        Err(error) => Ok(error.code().to_string()),
    }
}

/// Tamper a lowered trigger node WITHOUT re-deriving the fingerprint; the production
/// load path must reject it typed, never boot a tampered mission script.
pub fn tampered_triggers_rejected(
    path: &Path,
    package: &QuickSharePackage,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut tampered = package.clone();
    if let Some(runtime) = tampered.runtime.as_mut()
        && let Some(graph) = runtime.triggers.as_mut()
        && let Some(trigger) = graph.triggers.first_mut()
    {
        trigger.initially_on = !trigger.initially_on; // fingerprint no longer matches
    }
    fs::write(path, serde_json::to_vec_pretty(&tampered)?)?;
    match load_runtime_locked_package(path, "litd-game-cli") {
        Ok(_) => Ok("ACCEPTED_SILENTLY".to_string()),
        Err(error) => Ok(error.code().to_string()),
    }
}

pub fn certutil_sha256(path: &Path) -> Option<String> {
    let output = Command::new("certutil")
        .args(["-hashfile", &path.display().to_string(), "SHA256"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let candidate: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if candidate.len() == 64 && candidate.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(candidate.to_ascii_lowercase());
        }
    }
    None
}
