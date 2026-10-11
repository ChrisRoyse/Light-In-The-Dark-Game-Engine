#[path = "fixtures/issue1060_mortality_fixture.rs"]
mod issue1060_mortality_fixture;
#[path = "fixtures/issue1061_undeath_fixture.rs"]
mod issue1061_undeath_fixture;

use std::fs;

use calyx_assay::DEFAULT_GATE_PANEL_LENSES;
use issue1060_mortality_fixture::ashen_reach_mortality_policy;
use issue1061_undeath_fixture::ashen_reach_undeath_policy;
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::{begin_issue_run, workspace_path},
    rpg::{
        AttributeScores, MortalityCharacterState, MortalityEvent, MortalityEventKind,
        MortalityPolicy, RPG_MORTALITY_SCHEMA, UndeadCharacterState, UndeadForm, UndeathPolicy,
        WitheringRequest, apply_withering, evaluate_undead_aging, mortality_state_hash,
        persist_and_lower_rpg_authored_content, record_undead_physical_death,
        undeath_content_records, undeath_state_hash, validate_undeath_policy,
        validate_withering_request,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const MOURNING_LORE_PATH: &str = "docs/lore/ashen-reach-peoples-mourning-legends.md";
const WITHERING_REF_PATH: &str = "docs/readme/S03-withering.png";
const NECROMANCER_REF_PATH: &str = "docs/readme/GP15-necromancer.png";
const LICH_REF_PATH: &str = "docs/readme/B02-lichlord.png";
const WRAITH_REF_PATH: &str = "docs/readme/B09-wraithking.png";
const MUMMY_REF_PATH: &str = "docs/readme/B10-mummylord.png";
const MOURNING_REF_PATH: &str = "docs/readme/S15-mourning.png";
const MORTALITY_TYPES_PATH: &str = "crates/litd-engine/src/rpg/mortality_types.rs";
const MORTALITY_POLICY_PATH: &str = "crates/litd-engine/src/rpg/mortality.rs";
const MORTALITY_TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/mortality_test_fixture.rs";
const MORTALITY_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1060_mortality_fixture.rs";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/undeath_types.rs";
const POLICY_PATH: &str = "crates/litd-engine/src/rpg/undeath.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/undeath_content.rs";
const UNDEATH_TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/undeath_test_fixture.rs";
const UNDEATH_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1061_undeath_fixture.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/undeath_tests.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1061_undeath_fsv.rs";
const SOURCE_PATHS: [&str; 20] = [
    WORLD_BIBLE_PATH,
    COSMOLOGY_PATH,
    MOURNING_LORE_PATH,
    WITHERING_REF_PATH,
    NECROMANCER_REF_PATH,
    LICH_REF_PATH,
    WRAITH_REF_PATH,
    MUMMY_REF_PATH,
    MOURNING_REF_PATH,
    MORTALITY_TYPES_PATH,
    MORTALITY_POLICY_PATH,
    MORTALITY_TEST_FIXTURE_PATH,
    MORTALITY_EXAMPLE_FIXTURE_PATH,
    TYPES_PATH,
    POLICY_PATH,
    CONTENT_PATH,
    UNDEATH_TEST_FIXTURE_PATH,
    UNDEATH_EXAMPLE_FIXTURE_PATH,
    TEST_PATH,
    EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1061_undeath")?;
    let mortality = ashen_reach_mortality_policy();
    let undeath = ashen_reach_undeath_policy();
    let records = undeath_content_records(&undeath)?;
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
            .starts_with("editor://rpg/ashen_reach/undeath/")
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
    let measured_learned_lenses = persisted_rows
        .iter()
        .map(|row| row.measurement.measured_learned_lenses)
        .min()
        .unwrap_or_default();
    let measurement_panel_source_cpu_explicit = persisted_rows.iter().all(|row| {
        row.measurement
            .panel_source
            .starts_with("vault_cpu_explicit:")
    });
    let mortal = mortal_state(16, 95);
    let lich = apply_withering(&mortal, &mortality, &undeath, &lich_request())?;
    let lich_aging = evaluate_undead_aging(&lich.undead_state, &mortality, &undeath)?;
    let mummy = apply_withering(
        &mortal_state(16, 64),
        &mortality,
        &undeath,
        &mummy_request(),
    )?;
    let wraith = apply_withering(
        &mortal_state(16, 64),
        &mortality,
        &undeath,
        &wraith_request(),
    )?;
    let final_physical_death = final_physical_death(&wraith.undead_state, &mortality, &undeath)?;
    let edges = json!({
        "low_level_rejected": low_level_edge(&undeath),
        "missing_discovery_rejected": missing_discovery_edge(&undeath),
        "wrong_focus_rejected": wrong_focus_edge(&mortality, &undeath),
        "old_age_death_rejected": old_age_edge(&lich.undead_state, &mortality, &undeath),
        "dead_source_resurrection_rejected": dead_source_edge(&mortality, &undeath),
        "reversible_policy_rejected": reversible_policy_edge(&undeath),
    });
    require_all_edges_unchanged(&edges)?;
    let source_readbacks = SOURCE_PATHS
        .into_iter()
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;
    let source_readback_count = source_readbacks.len();
    let source_count_for = |needle: &str| -> usize {
        SOURCE_PATHS
            .iter()
            .filter(|path| path.contains(needle))
            .count()
    };

    let report = json!({
        "schema": "litd.issue1061.undeath_fsv.v1",
        "issue": 1061,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this JSON through Synapse/computer control, inspect the persisted source hashes and expected actuals, and manually inspect a saved bitmap from the same browser target."
        },
        "sufficiency_argument": "The minimal #1061 proof uses one old human dark-caster to prove the Withering reset, old-age suppression, hunger/thirst immunity, irreversible no-resurrection flags, and secret in-character refs; one mummy and one wraith probe prove all three forms grant distinct original rituals; one wraith physical-kill probe proves undead can still permanently die at CON 3. It also persists and lowers one policy row, one requirements row, and three form ritual rows as editor-authored Calyx content. Six edge probes cover low level, missing discovery, wrong focus, old-age death fallback, dead-source resurrection, and reversible policy drift.",
        "expected_actuals": {
            "minimum_caster_level": 47,
            "fixed_constitution_after_withering": 6,
            "lich_constitution_before": 16,
            "lich_constitution_after": 6,
            "lich_ritual": "ritual.vessel_siphon",
            "mummy_ritual": "ritual.flesh_memory_theft",
            "wraith_ritual": "ritual.thin_veil_stride",
            "old_age_death_suppressed": true,
            "hunger_thirst_immune": true,
            "physical_kill_only": true,
            "normal_resurrection_forbidden": true,
            "final_physical_death_con": 3,
            "final_physical_death_reason": "constitution_depleted",
            "source_readback_count": source_readback_count,
            "reference_source_count": source_count_for("docs/"),
            "mortality_source_count": source_count_for("mortality"),
            "undeath_source_count": source_count_for("undeath"),
            "measured_learned_lenses": measured_learned_lenses,
            "measurement_panel_source_cpu_explicit": measurement_panel_source_cpu_explicit,
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
            "edge_count": 6,
            "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "policy": {
            "server_policy_id": undeath.server_policy_id,
            "source_issue": undeath.source_issue,
            "minimum_caster_level": undeath.minimum_caster_level,
            "fixed_constitution_after_withering": undeath.fixed_constitution_after_withering,
            "aging_death_immune": undeath.aging_death_immune,
            "hunger_thirst_immune": undeath.hunger_thirst_immune,
            "physical_kill_only": undeath.physical_kill_only,
            "irreversible": undeath.irreversible,
            "normal_resurrection_forbidden": undeath.normal_resurrection_forbidden,
        },
        "happy_path": {
            "mortal_state_sha256": mortality_state_hash(&mortal)?,
            "lich_withering": lich,
            "lich_aging": lich_aging,
            "mummy_withering": mummy,
            "wraith_withering": wraith,
            "final_physical_death": final_physical_death,
        },
        "calyx": {
            "persisted_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });
    require_i64(&report["expected_actuals"]["source_readback_count"], 20)?;
    require_i64(&report["expected_actuals"]["reference_source_count"], 9)?;
    require_i64(&report["expected_actuals"]["mortality_source_count"], 4)?;
    require_i64(&report["expected_actuals"]["undeath_source_count"], 7)?;
    // Derive the expected learned-lens count from the authoritative gate roster
    // (`calyx_assay::DEFAULT_GATE_PANEL_LENSES`) rather than a magic literal so a roster
    // change can never silently strand this FSV assertion again (#1523). The default
    // CPU-explicit content panel loads exactly this many learned lenses, matching the gate
    // constant and the `calyx_content_tests` unit assertion.
    require_i64(
        &report["expected_actuals"]["measured_learned_lenses"],
        DEFAULT_GATE_PANEL_LENSES as i64,
    )?;
    require_bool(
        &report["expected_actuals"]["measurement_panel_source_cpu_explicit"],
        true,
    )?;
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 5)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 5)?;
    require_bool(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_compiled_source_path"],
        false,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_map_or_gps_true"],
        false,
    )?;
    require_i64(
        &report["happy_path"]["lich_withering"]["constitution_after"],
        6,
    )?;
    require_bool(
        &report["happy_path"]["lich_aging"]["old_age_death_suppressed"],
        true,
    )?;
    require_str(
        &report["happy_path"]["final_physical_death"]["mortality"]["permanent_death_reason"],
        "constitution_depleted",
    )?;
    require_i64(
        &report["happy_path"]["final_physical_death"]["mortality"]["constitution_after"],
        3,
    )?;
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1061_UNDEATH_FSV report={} bytes={} sha256={} rows={} anchors={} editor_sources={} lowered={} con_after={} old_age_suppressed={} final_con={} map_true={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["expected_actuals"]["content_rows_read_back"],
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["expected_actuals"]["lowered_artifact_count"],
        readback.json["happy_path"]["lich_withering"]["constitution_after"],
        readback.json["happy_path"]["lich_aging"]["old_age_death_suppressed"],
        readback.json["happy_path"]["final_physical_death"]["mortality"]["constitution_after"],
        readback.json["expected_actuals"]["content_has_map_or_gps_true"],
        readback.json["edges"]
            .as_object()
            .map(|edges| edges.len())
            .unwrap_or_default(),
    );
    Ok(())
}

fn final_physical_death(
    state: &UndeadCharacterState,
    mortality: &MortalityPolicy,
    undeath: &UndeathPolicy,
) -> Result<Value, EngineError> {
    let mut hunted = state.clone();
    hunted.mortality_state.current_constitution = 4;
    hunted.mortality_state.total_deaths = 8;
    hunted.mortality_state.deaths_since_constitution_loss = 2;
    let readback = record_undead_physical_death(
        &hunted,
        mortality,
        undeath,
        &combat_event("death.combat.wraith_hunted"),
    )?;
    serde_json::to_value(readback).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize final physical death: {error}"),
    })
}

fn source_readback(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    // Anchor to the workspace root so the readback is independent of the process working
    // directory: `cargo test --example` (the run gate) sets CWD to the package root, where a
    // bare relative `docs/…` path does not exist. The recorded `path` stays workspace-relative
    // for report stability; only the read is anchored. Fail loud with the resolved path so a
    // genuinely missing source is diagnosable instead of a bare `NotFound` (#1527).
    let resolved = workspace_path(path);
    let bytes = fs::read(&resolved).map_err(|error| {
        format!(
            "source_readback: cannot read FSV source '{path}' (resolved to '{}'): {error}",
            resolved.display()
        )
    })?;
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
                "measurement_panel_source": row.measurement.panel_source,
                "measurement_panel_version": row.measurement.panel_version,
                "measured_learned_lenses": row.measurement.measured_learned_lenses,
                "measured_lens_names": row.measurement.measured_lens_names,
                "measurement_association_family_count": row.measurement.association_family_count,
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn low_level_edge(policy: &UndeathPolicy) -> Value {
    let mut request = lich_request();
    request.caster_level = 46;
    edge_from_result(
        validate_withering_request(&request, policy).map(|_| ()),
        true,
    )
}

fn missing_discovery_edge(policy: &UndeathPolicy) -> Value {
    let mut request = lich_request();
    request.discovery_refs.pop();
    edge_from_result(
        validate_withering_request(&request, policy).map(|_| ()),
        true,
    )
}

fn wrong_focus_edge(mortality: &MortalityPolicy, undeath: &UndeathPolicy) -> Value {
    let state = mortal_state(16, 64);
    let before = mortality_state_hash(&state).unwrap();
    let mut request = lich_request();
    request.prepared_focus_id = "focus.borrowed_crown".to_string();
    let result = apply_withering(&state, mortality, undeath, &request).map(|_| ());
    edge_from_result(result, before == mortality_state_hash(&state).unwrap())
}

fn old_age_edge(
    state: &UndeadCharacterState,
    mortality: &MortalityPolicy,
    undeath: &UndeathPolicy,
) -> Value {
    let before = undeath_state_hash(state).unwrap();
    let result =
        record_undead_physical_death(state, mortality, undeath, &old_age_event()).map(|_| ());
    edge_from_result(result, before == undeath_state_hash(state).unwrap())
}

fn dead_source_edge(mortality: &MortalityPolicy, undeath: &UndeathPolicy) -> Value {
    let mut state = mortal_state(3, 64);
    state.permanently_dead = true;
    state.mourning_wall_entry_id = Some("mourning.wall.char.varn_black.3".to_string());
    let before = mortality_state_hash(&state).unwrap();
    let result = apply_withering(&state, mortality, undeath, &lich_request()).map(|_| ());
    edge_from_result(result, before == mortality_state_hash(&state).unwrap())
}

fn reversible_policy_edge(policy: &UndeathPolicy) -> Value {
    let mut policy = policy.clone();
    policy.irreversible = false;
    edge_from_result(validate_undeath_policy(&policy).map(|_| ()), true)
}

fn edge_from_result(result: Result<(), EngineError>, state_unchanged: bool) -> Value {
    match result {
        Ok(()) => json!({"unexpected_success": true, "state_unchanged": state_unchanged}),
        Err(error) => json!({
            "error_code": error.code(),
            "error": error.to_string(),
            "state_unchanged": state_unchanged,
        }),
    }
}

fn mortal_state(current_constitution: i16, age_years: u16) -> MortalityCharacterState {
    MortalityCharacterState {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        character_id: "char.varn_black".to_string(),
        character_name: "Varn Black".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 47,
        age_years,
        base_attributes: AttributeScores::new(10, 19, 12, 12, 16, 8),
        current_constitution,
        deaths_since_constitution_loss: 0,
        total_deaths: 0,
        permanently_dead: false,
        mourning_wall_entry_id: None,
    }
}

fn lich_request() -> WitheringRequest {
    request(UndeadForm::Lich, "focus.vessel_of_last_breath")
}

fn mummy_request() -> WitheringRequest {
    request(UndeadForm::Mummy, "focus.grave_wrappings")
}

fn wraith_request() -> WitheringRequest {
    request(UndeadForm::Wraith, "focus.veil_anchor")
}

fn request(chosen_form: UndeadForm, prepared_focus_id: &str) -> WitheringRequest {
    WitheringRequest {
        ritual_id: format!("ritual.withering.{}", chosen_form.label()),
        chosen_form,
        caster_class_id: "class.necromancer".to_string(),
        caster_level: 47,
        discovery_refs: vec![
            "quest.secret_withering_path".to_string(),
            "immortal.mphirae.last_veil_witness".to_string(),
        ],
        ritual_components: vec![
            "component.name_ash".to_string(),
            "component.black_candle".to_string(),
            "component.last_breath_salt".to_string(),
        ],
        prepared_focus_id: prepared_focus_id.to_string(),
        accepted_irreversible_cost: true,
    }
}

fn combat_event(event_id: &str) -> MortalityEvent {
    MortalityEvent::new(
        event_id,
        MortalityEventKind::CombatDeath,
        "Hunted in the crypt and shattered under witness.",
    )
}

fn old_age_event() -> MortalityEvent {
    MortalityEvent::new(
        "death.old_age.undead_forbidden",
        MortalityEventKind::OldAge,
        "This should never age-kill an undead character.",
    )
}

include!("issue1061_undeath_fsv/assertions.rs");
