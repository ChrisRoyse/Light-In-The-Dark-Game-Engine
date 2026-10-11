use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AttributeScores, InfernalAttritionState, InfernalAttritionStep, InfernalCircleId,
        InfernalHazardKind, MortalityCharacterState, RPG_INFERNAL_ATTRITION_SCHEMA,
        RPG_MORTALITY_SCHEMA, apply_infernal_attrition, infernal_attrition_content_records,
        persist_and_lower_rpg_authored_content, validate_infernal_attrition_policy,
        validate_infernal_attrition_state, validate_infernal_attrition_step,
        validate_rpg_authored_content_records,
    },
    rpg_infernal_attrition_test_fixture::ashen_reach_infernal_attrition_policy,
    rpg_mortality_test_fixture::ashen_reach_mortality_policy,
};

#[test]
fn issue1070_ember_wastes_heat_drains_health_and_strain() {
    let mortality = ashen_reach_mortality_policy();
    let attrition = ashen_reach_infernal_attrition_policy();
    let state = state(InfernalCircleId::EmberWastes, 220, 16, 0);
    let readback = apply_infernal_attrition(
        &state,
        &attrition,
        &mortality,
        &step("ember.5", InfernalCircleId::EmberWastes, 5, 0),
    )
    .unwrap();

    assert_eq!(readback.hazard, InfernalHazardKind::Heat);
    assert_eq!(readback.health_before, 220);
    assert_eq!(readback.health_after, 175);
    assert_eq!(readback.strain_after, 60);
    assert_eq!(readback.constitution_after, 16);
    assert!(readback.death.is_none());
}

#[test]
fn issue1070_dark_pressure_respects_mitigation_without_fallback() {
    let mortality = ashen_reach_mortality_policy();
    let attrition = ashen_reach_infernal_attrition_policy();
    let state = state(InfernalCircleId::HungeringDark, 100, 16, 0);
    let readback = apply_infernal_attrition(
        &state,
        &attrition,
        &mortality,
        &step("dark.10", InfernalCircleId::HungeringDark, 10, 50),
    )
    .unwrap();

    assert_eq!(readback.hazard, InfernalHazardKind::DarkPressure);
    assert_eq!(readback.health_after, 65);
    assert_eq!(readback.strain_gained, 75);
    assert_eq!(readback.strain_after, 75);
    assert_eq!(readback.constitution_lost_to_strain, 0);
}

#[test]
fn issue1070_soul_drain_directly_costs_constitution_over_time() {
    let mortality = ashen_reach_mortality_policy();
    let attrition = ashen_reach_infernal_attrition_policy();
    let state = state(InfernalCircleId::WailingTombs, 120, 16, 90);
    let readback = apply_infernal_attrition(
        &state,
        &attrition,
        &mortality,
        &step("soul.1", InfernalCircleId::WailingTombs, 1, 0),
    )
    .unwrap();

    assert_eq!(readback.hazard, InfernalHazardKind::SoulDrain);
    assert_eq!(readback.health_after, 115);
    assert_eq!(readback.strain_gained, 25);
    assert_eq!(readback.strain_after, 15);
    assert_eq!(readback.constitution_lost_to_strain, 1);
    assert_eq!(readback.constitution_after, 15);
}

#[test]
fn issue1070_attrition_crossing_hp_threshold_records_mortality_event() {
    let mortality = ashen_reach_mortality_policy();
    let attrition = ashen_reach_infernal_attrition_policy();
    let state = state(InfernalCircleId::EmberWastes, 0, 16, 0);
    let readback = apply_infernal_attrition(
        &state,
        &attrition,
        &mortality,
        &step("ember.death", InfernalCircleId::EmberWastes, 5, 0),
    )
    .unwrap();
    let death = readback.death.as_ref().unwrap();
    let event = death.mortality_event.as_ref().unwrap();

    assert_eq!(readback.health_after, -45);
    assert_eq!(readback.death_threshold_hit_points, -44);
    assert!(!death.permanent);
    assert_eq!(death.reason, "infernal_death_threshold");
    assert_eq!(
        event.event_kind,
        crate::rpg::MortalityEventKind::HollowAttrition
    );
    assert_eq!(event.total_deaths_after, 1);
}

#[test]
fn issue1070_constitution_drain_to_three_is_permanent_death() {
    let mortality = ashen_reach_mortality_policy();
    let attrition = ashen_reach_infernal_attrition_policy();
    let state = state(InfernalCircleId::WailingTombs, 120, 4, 90);
    let readback = apply_infernal_attrition(
        &state,
        &attrition,
        &mortality,
        &step("soul.final", InfernalCircleId::WailingTombs, 1, 0),
    )
    .unwrap();
    let death = readback.death.as_ref().unwrap();
    let entry = death.mourning_wall_entry.as_ref().unwrap();

    assert_eq!(readback.constitution_after, 3);
    assert!(death.permanent);
    assert_eq!(death.reason, "infernal_constitution_drained");
    assert_eq!(entry.final_constitution, 3);
    assert_eq!(entry.source_issue, 1070);
    assert_eq!(
        readback
            .state_after
            .mortality_state
            .mourning_wall_entry_id
            .as_deref(),
        Some("mourning.wall.char.hellrunner.1")
    );
}

#[test]
fn issue1484_reports_applied_constitution_delta_not_quotient() {
    // Clamp case: the strain quotient exceeds the constitution actually on hand,
    // so `.max(1)` clamps the applied loss. strain 99 + 13*25 = 424 -> con_loss 4,
    // but constitution 4 can only fall to 1 (delta 3). Before #1484-3 the readback
    // reported the quotient (4), making constitution_before - constitution_after
    // disagree with the field and poisoning result_sha256.
    let mortality = ashen_reach_mortality_policy();
    let attrition = ashen_reach_infernal_attrition_policy();
    let state = state(InfernalCircleId::WailingTombs, 120, 4, 99);
    let readback = apply_infernal_attrition(
        &state,
        &attrition,
        &mortality,
        &step("soul.clamp", InfernalCircleId::WailingTombs, 13, 0),
    )
    .unwrap();

    assert_eq!(readback.constitution_before, 4);
    assert_eq!(readback.constitution_after, 1);
    assert_eq!(readback.constitution_lost_to_strain, 3);
    // The reported strain loss now reconciles with the before/after readback.
    assert_eq!(
        readback.constitution_lost_to_strain,
        readback.constitution_before - readback.constitution_after
    );
    assert!(readback.death.as_ref().unwrap().permanent);
}

#[test]
fn issue1070_attrition_policy_persists_as_authored_calyx_content() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let records = infernal_attrition_content_records(&attrition).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault_dir) = content_store("attrition");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let rows = records
        .iter()
        .map(|record| {
            store
                .get_content_for_editor(record.kind, &record.id)
                .unwrap()
        })
        .collect::<Vec<_>>();

    assert_eq!(validation.record_count, 8);
    assert_eq!(lowered.row_count, 8);
    assert_eq!(rows.len(), 8);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(
        rows.iter()
            .all(|row| row.kind == EngineContentKind::Trigger)
    );
    assert!(rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_attrition/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|entity| entity.lowered_field_count > 0)
    );
    assert!(
        rows.iter()
            .filter(|row| row.payload["surface_kind"] == "infernal_attrition_circle_profile")
            .count()
            == 7
    );
    assert!(rows.iter().all(|row| {
        row.payload["built_in_map_required"] != true
            && row.payload["grants_map_or_minimap"] != true
            && row.payload["grants_gps_markers"] != true
    }));
}

#[test]
fn issue1070_infernal_attrition_fails_closed_on_invalid_edges() {
    let mortality = ashen_reach_mortality_policy();
    let attrition = ashen_reach_infernal_attrition_policy();

    let mut duplicate = attrition.clone();
    duplicate.circles[1].circle_id = InfernalCircleId::Threshold;
    assert_code(
        validate_infernal_attrition_policy(&duplicate),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    assert_code(
        validate_infernal_attrition_step(
            &step("bad.zero", InfernalCircleId::EmberWastes, 0, 0),
            &attrition,
        ),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    assert_code(
        validate_infernal_attrition_step(
            &step("bad.mitigation", InfernalCircleId::EmberWastes, 1, 90),
            &attrition,
        ),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut bad_state = state(InfernalCircleId::EmberWastes, 100, 16, 100);
    assert_code(
        validate_infernal_attrition_state(&bad_state, &attrition, &mortality),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    bad_state.constitution_strain = 0;
    assert_code(
        apply_infernal_attrition(
            &bad_state,
            &attrition,
            &mortality,
            &InfernalAttritionStep {
                circle_id: InfernalCircleId::WailingTombs,
                ..step("bad.circle", InfernalCircleId::EmberWastes, 1, 0)
            },
        ),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn state(
    circle_id: InfernalCircleId,
    hit_points: i32,
    current_constitution: i16,
    constitution_strain: i32,
) -> InfernalAttritionState {
    InfernalAttritionState {
        schema: RPG_INFERNAL_ATTRITION_SCHEMA.to_string(),
        run_id: "run.infernal.deeps.1".to_string(),
        party_member_id: "party.vanguard".to_string(),
        circle_id,
        hit_points,
        max_hit_points: 220,
        constitution_strain,
        total_exposure_ticks: 0,
        mortality_state: mortality_state(current_constitution),
    }
}

fn mortality_state(current_constitution: i16) -> MortalityCharacterState {
    MortalityCharacterState {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        character_id: "char.hellrunner".to_string(),
        character_name: "Ira Cindermark".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 51,
        age_years: 39,
        base_attributes: AttributeScores::new(16, 13, 12, 14, 16, 11),
        current_constitution,
        deaths_since_constitution_loss: 0,
        total_deaths: 0,
        permanently_dead: false,
        mourning_wall_entry_id: None,
    }
}

fn step(
    step_id: &str,
    circle_id: InfernalCircleId,
    tick_count: u16,
    mitigation_percent: u8,
) -> InfernalAttritionStep {
    InfernalAttritionStep {
        step_id: step_id.to_string(),
        circle_id,
        tick_count,
        mitigation_percent,
    }
}

fn content_store(
    label: &str,
) -> (
    EngineCalyxContentStore,
    EngineCalyxDatabase,
    crate::test_temp::TestTempDir,
) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database.clone(), label),
        database,
        vault_dir,
    )
}

// #1535: TestTempDir auto-cleans on Drop.
fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1070-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
