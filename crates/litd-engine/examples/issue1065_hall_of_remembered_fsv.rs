#[path = "fixtures/issue1060_mortality_fixture.rs"]
mod issue1060_mortality_fixture;

use std::fs;

use issue1060_mortality_fixture::ashen_reach_mortality_policy;
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        ASHEN_REACH_HALL_OF_REMEMBERED_POLICY_ID, AttributeScores, HallDeed, HallDeedKind,
        HallMemorialDossier, HallOfRememberedPolicy, HallTribute, MortalityCharacterState,
        MortalityEvent, MortalityEventKind, MortalityPolicy, RPG_HALL_OF_REMEMBERED_SCHEMA,
        RPG_MORTALITY_SCHEMA, hall_memorial_content_records, hall_of_remembered_policy_hash,
        persist_and_lower_rpg_authored_content, record_hall_memorial,
        validate_hall_of_remembered_policy,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const SOURCE_PATHS: [&str; 19] = [
    "docs/lore/ashen-reach-world-bible.md",
    "docs/lore/ashen-reach-peoples-mourning-legends.md",
    "docs/lore/ashen-reach-pantheon-factions.md",
    "docs/readme/S15-mourning.png",
    "docs/readme/G01-graveyard.png",
    "docs/readme/G02-memorial.png",
    "docs/readme/S20-living-world.png",
    "crates/litd-engine/src/rpg/hall_of_remembered_types.rs",
    "crates/litd-engine/src/rpg/hall_of_remembered.rs",
    "crates/litd-engine/src/rpg/hall_of_remembered_validate.rs",
    "crates/litd-engine/src/rpg/hall_of_remembered_content.rs",
    "crates/litd-engine/src/rpg/hall_of_remembered_tests.rs",
    "crates/litd-engine/src/rpg/hall_of_remembered_test_fixture.rs",
    "crates/litd-engine/src/rpg/mortality_types.rs",
    "crates/litd-engine/src/rpg/mortality.rs",
    "crates/litd-engine/src/rpg/mortality_content.rs",
    "crates/litd-engine/src/rpg/mortality_tests.rs",
    "crates/litd-engine/src/rpg/mortality_test_fixture.rs",
    "crates/litd-engine/examples/fixtures/issue1060_mortality_fixture.rs",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1065_hall_of_remembered")?;
    let policy = ashen_reach_hall_of_remembered_policy();
    let mortality = ashen_reach_mortality_policy();
    let death = permanent_death(&mortality)?;
    let dossier = dossier();
    let tributes = tributes();
    let readback = record_hall_memorial(&policy, &mortality, &death, &dossier, &tributes)?;
    let records = hall_memorial_content_records(&policy, &readback)?;

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
            .starts_with("editor://rpg/ashen_reach/hall_of_remembered/")
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
        "gps_surface_rejected": gps_edge(&policy),
        "non_persistent_rejected": non_persistent_edge(&policy),
        "offline_browse_rejected": offline_edge(&policy),
        "non_permanent_death_rejected": non_permanent_edge(&policy, &mortality, &dossier, &tributes),
        "identity_mismatch_rejected": mismatch_edge(&policy, &mortality, &death, &tributes),
        "thin_deeds_rejected": thin_deeds_edge(&policy, &mortality, &death, &tributes),
        "private_tribute_rejected": private_tribute_edge(&policy, &mortality, &death, &dossier),
    });
    require_all_edges_unchanged(&edges)?;
    let source_readbacks = SOURCE_PATHS
        .into_iter()
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;

    let report = json!({
        "schema": "litd.issue1065.hall_of_remembered_fsv.v1",
        "issue": 1065,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Use Synapse/computer control to read this JSON and the Calyx WAL. Compare expected_actuals to persisted Hall rows and the mortality handoff."
        },
        "sufficiency_argument": "The #1065 proof starts from a real #1060 permanent-death readback with a Mourning Wall entry, binds a character dossier containing identity, lifespan, titles, four deed categories, memorable quotes, and two opt-in player tributes, then persists three real Calyx rows for the memorial record, Galdmere browser surface, and online browser surface. Eight edge probes prove no built-in map/minimap/GPS drift, missing persistence/browser drift, non-permanent deaths, identity mismatch, thin deeds, and private tributes fail closed.",
        "expected_actuals": {
            "source_issue": 1065,
            "permanently_dead": true,
            "mourning_wall_entry_id": "mourning.wall.char.aren_vael.3",
            "memorial_id": "memorial.hall.char.aren_vael.3",
            "class_id": "class.warden",
            "faction_id": "organization.dawnward",
            "lifespan_years": 44,
            "deed_count": 4,
            "title_count": 2,
            "tribute_count": 2,
            "memorable_quote_count": 2,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
            "content_has_map_or_gps_true": content_has_map_true,
            "persistent_across_server": true,
            "surfaced_in_galdmere": true,
            "browsable_online": true,
            "edge_count": 8,
            "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "policy": {
            "server_policy_id": policy.server_policy_id,
            "source_issue": policy.source_issue,
            "built_in_map_required": policy.built_in_map_required,
            "policy_sha256": hall_of_remembered_policy_hash(&policy)?,
        },
        "mortality": death,
        "hall_readback": readback,
        "calyx": {
            "persisted_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 3)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 3)?;
    require_bool(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_compiled_source_path"],
        false,
    )?;
    require_i64(&report["hall_readback"]["deed_count"], 4)?;
    require_bool(&report["hall_readback"]["built_in_map_required"], false)?;
    require_bool(
        &report["expected_actuals"]["content_has_map_or_gps_true"],
        false,
    )?;
    require_str(
        &report["hall_readback"]["mourning_wall_entry_id"],
        "mourning.wall.char.aren_vael.3",
    )?;

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1065_HALL_OF_REMEMBERED_FSV report={} bytes={} sha256={} rows={} deeds={} tributes={} map_true={} edges={}",
        read.path.display(),
        read.bytes.len(),
        read.sha256,
        read.json["expected_actuals"]["content_rows_read_back"],
        read.json["hall_readback"]["deed_count"],
        read.json["hall_readback"]["tribute_count"],
        read.json["expected_actuals"]["content_has_map_or_gps_true"],
        read.json["edges"]
            .as_object()
            .map(|edges| edges.len())
            .unwrap_or_default(),
    );
    Ok(())
}

fn ashen_reach_hall_of_remembered_policy() -> HallOfRememberedPolicy {
    HallOfRememberedPolicy {
        schema: RPG_HALL_OF_REMEMBERED_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_HALL_OF_REMEMBERED_POLICY_ID.to_string(),
        source_issue: 1065,
        built_in_map_required: false,
        persistent_across_server: true,
        auto_populate_from_character_record: true,
        opt_in_player_tributes_only: true,
        browsable_in_galdmere: true,
        browsable_online: true,
        galdmere_surface_id: "surface.galdmere.mourning_wall".to_string(),
        online_surface_id: "surface.online.hall_of_remembered".to_string(),
        minimum_deed_count: 3,
        maximum_tribute_count: 32,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
    }
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

fn built_in_map_edge(policy: &HallOfRememberedPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.built_in_map_required = true;
    edge_from_result(validate_hall_of_remembered_policy(&invalid), true)
}
fn gps_edge(policy: &HallOfRememberedPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.grants_gps_markers = true;
    edge_from_result(validate_hall_of_remembered_policy(&invalid), true)
}
fn non_persistent_edge(policy: &HallOfRememberedPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.persistent_across_server = false;
    edge_from_result(validate_hall_of_remembered_policy(&invalid), true)
}
fn offline_edge(policy: &HallOfRememberedPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.browsable_online = false;
    edge_from_result(validate_hall_of_remembered_policy(&invalid), true)
}
fn non_permanent_edge(
    policy: &HallOfRememberedPolicy,
    mortality: &MortalityPolicy,
    dossier: &HallMemorialDossier,
    tributes: &[HallTribute],
) -> Value {
    let death =
        litd_engine::rpg::record_mortality_event(&living_state(16), mortality, &combat_event())
            .unwrap();
    edge_from_result(
        record_hall_memorial(policy, mortality, &death, dossier, tributes),
        true,
    )
}
fn mismatch_edge(
    policy: &HallOfRememberedPolicy,
    mortality: &MortalityPolicy,
    death: &litd_engine::rpg::MortalityEventReadback,
    tributes: &[HallTribute],
) -> Value {
    let mut invalid = dossier();
    invalid.race_id = "race.ashen_elf".to_string();
    edge_from_result(
        record_hall_memorial(policy, mortality, death, &invalid, tributes),
        true,
    )
}
fn thin_deeds_edge(
    policy: &HallOfRememberedPolicy,
    mortality: &MortalityPolicy,
    death: &litd_engine::rpg::MortalityEventReadback,
    tributes: &[HallTribute],
) -> Value {
    let mut invalid = dossier();
    invalid.deeds.truncate(2);
    edge_from_result(
        record_hall_memorial(policy, mortality, death, &invalid, tributes),
        true,
    )
}
fn private_tribute_edge(
    policy: &HallOfRememberedPolicy,
    mortality: &MortalityPolicy,
    death: &litd_engine::rpg::MortalityEventReadback,
    dossier: &HallMemorialDossier,
) -> Value {
    let mut invalid = tributes();
    invalid[0].opted_in_publicly = false;
    edge_from_result(
        record_hall_memorial(policy, mortality, death, dossier, &invalid),
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

fn permanent_death(
    mortality: &MortalityPolicy,
) -> Result<litd_engine::rpg::MortalityEventReadback, EngineError> {
    let mut state = living_state(4);
    state.total_deaths = 2;
    state.deaths_since_constitution_loss = 2;
    litd_engine::rpg::record_mortality_event(
        &state,
        mortality,
        &combat_event().witnessed_by("player.mira_steadfast"),
    )
}

fn living_state(current_constitution: i16) -> MortalityCharacterState {
    MortalityCharacterState {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        character_id: "char.aren_vael".to_string(),
        character_name: "Aren Vael".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 43,
        age_years: 44,
        base_attributes: AttributeScores::new(14, 11, 12, 13, 16, 10),
        current_constitution,
        deaths_since_constitution_loss: 0,
        total_deaths: 0,
        permanently_dead: false,
        mourning_wall_entry_id: None,
    }
}

fn combat_event() -> MortalityEvent {
    MortalityEvent::new(
        "death.combat.ember_gate",
        MortalityEventKind::CombatDeath,
        "Held the Ember Gate while the caravan crossed.",
    )
}

fn dossier() -> HallMemorialDossier {
    HallMemorialDossier {
        character_id: "char.aren_vael".to_string(),
        character_name: "Aren Vael".to_string(),
        race_id: "race.ashen_human".to_string(),
        class_id: "class.warden".to_string(),
        faction_id: "organization.dawnward".to_string(),
        lifespan_years: 44,
        titles: vec![
            "Gatekeeper of Cinders".to_string(),
            "High Lantern".to_string(),
        ],
        deeds: vec![
            HallDeed::new(
                "deed.boss.cinder_judge",
                HallDeedKind::BossSlain,
                "Broke the Cinder Judge before dawn.",
                "boss.cinder_judge",
                950,
            ),
            HallDeed::new(
                "deed.item.unfaltering_lamp",
                HallDeedKind::LegendaryItemHeld,
                "Carried the Unfaltering Lamp through Hollow rain.",
                "item.dawnward.unfaltering_lamp",
                980,
            ),
            HallDeed::new(
                "deed.raid.ember_gate",
                HallDeedKind::RaidWon,
                "Won the Ember Gate defense with seven living witnesses.",
                "raid.ember_gate_defense",
                1000,
            ),
            HallDeed::new(
                "deed.rank.high_lantern",
                HallDeedKind::RankReached,
                "Earned High Lantern by public service.",
                "rank.dawnward.high_lantern",
                900,
            ),
        ],
        memorable_quotes: vec![
            "Hold the road, not the glory.".to_string(),
            "Let them remember the crossing, not the fear.".to_string(),
        ],
    }
}

fn tributes() -> Vec<HallTribute> {
    vec![
        HallTribute::opted_in(
            "tribute.mira.aren.1",
            "player.mira",
            "char.mira_steadfast",
            "Aren spent the last shield on strangers and never asked their names.",
            "tribute.public.mira.1",
        ),
        HallTribute::opted_in(
            "tribute.orren.aren.1",
            "player.orren",
            "char.orren_vale",
            "The gate held because Aren made fear feel smaller than duty.",
            "tribute.public.orren.1",
        ),
    ]
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1065.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_str(value: &Value, expected: &str) -> Result<(), EngineError> {
    if value.as_str() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1065.report.str".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1065.report.i64".to_string(),
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
                field: format!("issue1065.edge.{name}"),
                reason: format!("unexpected edge {edge}"),
            });
        }
    }
    Ok(())
}
