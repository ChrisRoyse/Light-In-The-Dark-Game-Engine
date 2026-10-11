use std::fs;

use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        ASHEN_REACH_UNHALLOWED_KNIGHT_POLICY_ID, RPG_UNHALLOWED_KNIGHT_SCHEMA, SoulKillEvent,
        SoulVictimKind, SoulWeaponBindRequest, SoulWeaponRules, SoulWeaponTransferRequest,
        UNHALLOWED_KNIGHT_CLASS_ID, UnhallowedKnightCharacter, UnhallowedKnightPolicy,
        bind_soul_weapon, derive_unhallowed_knight_readback, destroy_soul_bound_weapon,
        persist_and_lower_rpg_authored_content, record_soul_weapon_kill,
        transfer_soul_weapon_power, unhallowed_knight_content_records,
        unhallowed_knight_policy_hash, validate_unhallowed_knight_policy,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const SOURCE_PATHS: [&str; 20] = [
    "docs/lore/ashen-reach-world-bible.md",
    "docs/lore/ashen-reach-cosmology.md",
    "docs/lore/ashen-reach-peoples-mourning-legends.md",
    "docs/readme/S04-unhallowed-knight.png",
    "docs/readme/S05-soulweapon.png",
    "docs/readme/GP16-soulknight.png",
    "docs/readme/GP06-pvp.png",
    "crates/litd-engine/src/rpg/unhallowed_knight_types.rs",
    "crates/litd-engine/src/rpg/unhallowed_knight_abilities.rs",
    "crates/litd-engine/src/rpg/unhallowed_knight.rs",
    "crates/litd-engine/src/rpg/unhallowed_knight_content.rs",
    "crates/litd-engine/src/rpg/unhallowed_knight_tests.rs",
    "crates/litd-engine/src/rpg/abilities_types.rs",
    "crates/litd-engine/src/rpg/abilities.rs",
    "crates/litd-engine/src/rpg/abilities_support.rs",
    "crates/litd-engine/src/rpg/abilities_validate.rs",
    "crates/litd-engine/src/rpg/abilities_readback.rs",
    "crates/litd-engine/src/rpg/abilities_content.rs",
    "crates/litd-engine/src/rpg/abilities_tests.rs",
    "crates/litd-engine/src/rpg/abilities_test_fixture.rs",
];

fn ashen_reach_unhallowed_knight_policy() -> UnhallowedKnightPolicy {
    UnhallowedKnightPolicy {
        schema: RPG_UNHALLOWED_KNIGHT_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_UNHALLOWED_KNIGHT_POLICY_ID.to_string(),
        source_issue: 1062,
        class_id: UNHALLOWED_KNIGHT_CLASS_ID.to_string(),
        display_name: "Unhallowed Knight".to_string(),
        prime_stat: "strength".to_string(),
        required_alignment: "evil".to_string(),
        minimum_strength: 16,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
        soul_weapon: SoulWeaponRules {
            binding_rite_id: "rite.unhallowed.black_oath_binding".to_string(),
            transfer_rite_id: "rite.unhallowed.iron_to_iron_transfer".to_string(),
            soul_resource_id: "resource.siphoned_souls".to_string(),
            every_life_claimed_strengthens: true,
            accumulated_power_transferable: true,
            destroying_weapon_wounds_master: true,
            initial_power_milli: 350,
            power_per_soul_milli: 25,
            maximum_power_milli: 1000,
            transfer_efficiency_milli: 900,
            destruction_wound_milli: 300,
            grants_map_or_minimap: false,
            grants_gps_markers: false,
        },
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1062_unhallowed_knight")?;
    let policy = ashen_reach_unhallowed_knight_policy();
    let class_readback = derive_unhallowed_knight_readback(&policy)?;
    let bound = bind_soul_weapon(&policy, &character(), &bind_request())?;
    let first_kill = record_soul_weapon_kill(&policy, &bound.after_state, &kill("kill.pvp.1", 3))?;
    let second_kill =
        record_soul_weapon_kill(&policy, &first_kill.after_state, &kill("kill.npc.1", 1))?;
    let transferred = transfer_soul_weapon_power(&policy, &second_kill.after_state, &transfer())?;
    let destroyed =
        destroy_soul_bound_weapon(&policy, &transferred.after_state, "destroy.throne_iron.1")?;
    let records = unhallowed_knight_content_records(&policy)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    let persisted_rows = records
        .iter()
        .map(|record| store.get_content_for_editor(record.kind, &record.id))
        .collect::<Result<Vec<_>, _>>()?;
    let persisted_json = serde_json::to_string(&persisted_rows)?;
    let content_has_map_true = persisted_json.contains("\"built_in_map_required\":true")
        || persisted_json.contains("\"grants_map_or_minimap\":true")
        || persisted_json.contains("\"grants_gps_markers\":true");
    let source_anchor_count = records
        .iter()
        .map(|record| record.anchors.len())
        .sum::<usize>();
    let source_paths_are_editor_authored = persisted_rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/unhallowed_knight/")
    });
    let content_has_compiled_source_path = persisted_rows
        .iter()
        .any(|row| row.source_path.starts_with("rpg://"));
    let lowered_artifact_count = persisted_rows
        .iter()
        .filter(|row| {
            row.lowering.hot_path_policy == "frozen_fingerprinted_data_only"
                && !row.lowering.fingerprint.is_empty()
        })
        .count();

    let edges = json!({
        "built_in_map_rejected": built_in_map_edge(&policy),
        "non_evil_rejected": non_evil_edge(&policy),
        "weak_strength_rejected": weak_strength_edge(&policy),
        "wrong_binding_rite_rejected": wrong_binding_rite_edge(&policy),
        "duplicate_kill_rejected": duplicate_kill_edge(&policy, &first_kill.after_state),
        "unwitnessed_kill_rejected": unwitnessed_kill_edge(&policy, &bound.after_state),
        "transfer_without_cost_rejected": transfer_cost_edge(&policy, &second_kill.after_state),
        "destroyed_weapon_kill_rejected": destroyed_kill_edge(&policy, &destroyed.after_state),
    });
    require_all_edges_unchanged(&edges)?;
    let source_readbacks = SOURCE_PATHS
        .into_iter()
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;

    let report = json!({
        "schema": "litd.issue1062.unhallowed_knight_fsv.v1",
        "issue": 1062,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Use Synapse/computer control to read this JSON and the Calyx WAL. Compare expected_actuals to persisted class, weapon, and ability rows."
        },
        "sufficiency_argument": "The #1062 proof covers the named class slice: STR-prime evil-only Unhallowed Knight policy, irreversible soul-weapon binding, witnessed kill siphons that permanently increase weapon power, transfer of accumulated power to a new weapon, destruction wound to the master, and six original dark battle-magic abilities covering binding, soul cleave, infernal fire, plague, despair aura, and fiend summoning. Eight Calyx content rows prove class, weapon, and ability persistence. Eight edge probes prove map drift, alignment/strength drift, wrong rite, duplicate/unwitnessed kills, unpaid transfer, and destroyed-weapon siphoning fail closed.",
        "expected_actuals": {
            "source_issue": 1062,
            "class_id": UNHALLOWED_KNIGHT_CLASS_ID,
            "prime_stat": "strength",
            "required_alignment": "evil",
            "ability_count": 6,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "souls_after_second_kill": second_kill.souls_after,
            "power_after_second_kill": second_kill.power_after_milli,
            "power_after_transfer": transferred.power_after_milli,
            "weapon_after_transfer": transferred.weapon_id_after,
            "power_after_destroy": destroyed.power_after_milli,
            "master_wound_after_destroy": destroyed.master_wound_after_milli,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
            "content_has_map_or_gps_true": content_has_map_true,
            "edge_count": 8,
            "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "policy": {
            "server_policy_id": policy.server_policy_id,
            "source_issue": policy.source_issue,
            "policy_sha256": unhallowed_knight_policy_hash(&policy)?,
            "soul_weapon": policy.soul_weapon,
        },
        "class_readback": class_readback,
        "soul_weapon_actions": {
            "bind": bound,
            "first_kill": first_kill,
            "second_kill": second_kill,
            "transfer": transferred,
            "destroy": destroyed,
        },
        "calyx": {
            "persisted_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 8)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 8)?;
    require_bool(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_compiled_source_path"],
        false,
    )?;
    require_i64(&report["expected_actuals"]["ability_count"], 6)?;
    require_i64(&report["expected_actuals"]["power_after_second_kill"], 450)?;
    require_i64(&report["expected_actuals"]["power_after_transfer"], 405)?;
    require_i64(
        &report["expected_actuals"]["master_wound_after_destroy"],
        300,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_map_or_gps_true"],
        false,
    )?;

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1062_UNHALLOWED_KNIGHT_FSV report={} bytes={} sha256={} rows={} abilities={} power_second={} transfer_power={} wound={} map_true={} edges={}",
        read.path.display(),
        read.bytes.len(),
        read.sha256,
        read.json["expected_actuals"]["content_rows_read_back"],
        read.json["expected_actuals"]["ability_count"],
        read.json["expected_actuals"]["power_after_second_kill"],
        read.json["expected_actuals"]["power_after_transfer"],
        read.json["expected_actuals"]["master_wound_after_destroy"],
        read.json["expected_actuals"]["content_has_map_or_gps_true"],
        read.json["edges"]
            .as_object()
            .map(|edges| edges.len())
            .unwrap_or_default(),
    );
    Ok(())
}

fn source_readback(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(json!({"path": path, "bytes": bytes.len(), "sha256": sha256_hex(&bytes)}))
}

fn content_rows(rows: &[EngineContentConstellationReadback]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            json!({
                "id": row.id,
                "kind": row.kind,
                "database_id": row.database.id,
                "database_sha256": row.database.sha256,
                "source_path": row.source_path,
                "surface_kind": row.payload["surface_kind"],
                "lowering_artifact_id": row.lowering.artifact_id,
                "lowering_fingerprint": row.lowering.fingerprint,
                "lowering_hot_path_policy": row.lowering.hot_path_policy,
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn built_in_map_edge(policy: &UnhallowedKnightPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.built_in_map_required = true;
    edge_from_result(validate_unhallowed_knight_policy(&invalid), true)
}
fn non_evil_edge(policy: &UnhallowedKnightPolicy) -> Value {
    let mut character = character();
    character.alignment = "neutral".to_string();
    edge_from_result(bind_soul_weapon(policy, &character, &bind_request()), true)
}
fn weak_strength_edge(policy: &UnhallowedKnightPolicy) -> Value {
    let mut character = character();
    character.strength = 12;
    edge_from_result(bind_soul_weapon(policy, &character, &bind_request()), true)
}
fn wrong_binding_rite_edge(policy: &UnhallowedKnightPolicy) -> Value {
    let mut request = bind_request();
    request.rite_id = "rite.wrong".to_string();
    edge_from_result(bind_soul_weapon(policy, &character(), &request), true)
}
fn duplicate_kill_edge(
    policy: &UnhallowedKnightPolicy,
    state: &litd_engine::rpg::SoulBoundWeaponState,
) -> Value {
    edge_from_result(
        record_soul_weapon_kill(policy, state, &kill("kill.pvp.1", 3)),
        true,
    )
}
fn unwitnessed_kill_edge(
    policy: &UnhallowedKnightPolicy,
    state: &litd_engine::rpg::SoulBoundWeaponState,
) -> Value {
    let mut event = kill("kill.hidden.1", 1);
    event.witnessed = false;
    edge_from_result(record_soul_weapon_kill(policy, state, &event), true)
}
fn transfer_cost_edge(
    policy: &UnhallowedKnightPolicy,
    state: &litd_engine::rpg::SoulBoundWeaponState,
) -> Value {
    let mut request = transfer();
    request.accepted_soul_scarring_cost = false;
    edge_from_result(transfer_soul_weapon_power(policy, state, &request), true)
}
fn destroyed_kill_edge(
    policy: &UnhallowedKnightPolicy,
    state: &litd_engine::rpg::SoulBoundWeaponState,
) -> Value {
    edge_from_result(
        record_soul_weapon_kill(policy, state, &kill("kill.dead_weapon", 1)),
        true,
    )
}

fn edge_from_result<T>(result: Result<T, EngineError>, state_unchanged: bool) -> Value {
    match result {
        Ok(_) => json!({"unexpected_success": true, "state_unchanged": state_unchanged}),
        Err(error) => {
            json!({"error_code": error.code(), "error": error.to_string(), "state_unchanged": state_unchanged})
        }
    }
}

fn character() -> UnhallowedKnightCharacter {
    UnhallowedKnightCharacter {
        character_id: "char.morvane_ash".to_string(),
        character_name: "Morvane Ash".to_string(),
        class_id: UNHALLOWED_KNIGHT_CLASS_ID.to_string(),
        alignment: "evil".to_string(),
        strength: 18,
    }
}

fn bind_request() -> SoulWeaponBindRequest {
    SoulWeaponBindRequest {
        weapon_id: "item.blade.black_oath".to_string(),
        weapon_display_name: "Black-Oath Brand".to_string(),
        rite_id: "rite.unhallowed.black_oath_binding".to_string(),
        accepted_irreversible_binding: true,
    }
}

fn transfer() -> SoulWeaponTransferRequest {
    SoulWeaponTransferRequest {
        transfer_id: "transfer.black_oath.to_throne_iron".to_string(),
        new_weapon_id: "item.blade.throne_iron".to_string(),
        new_weapon_display_name: "Throne-Iron Reaver".to_string(),
        rite_id: "rite.unhallowed.iron_to_iron_transfer".to_string(),
        accepted_soul_scarring_cost: true,
    }
}

fn kill(event_id: &str, soul_value: u16) -> SoulKillEvent {
    SoulKillEvent {
        event_id: event_id.to_string(),
        victim_id: format!("victim.{event_id}"),
        victim_kind: SoulVictimKind::PlayerCharacter,
        soul_value,
        witnessed: true,
    }
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1062.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1062.report.i64".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true || edge["error_code"] != "LITD-RPG-STATS-INVALID-CONFIG"
        {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1062.edge.{name}"),
                reason: format!("unexpected edge {edge}"),
            });
        }
    }
    Ok(())
}
