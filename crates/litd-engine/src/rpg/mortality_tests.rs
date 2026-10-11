use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AttributeScores, MortalityAgeBand, MortalityCharacterState, MortalityEvent,
        MortalityEventKind, RPG_MORTALITY_SCHEMA, derive_mortality_stats, evaluate_mortality_aging,
        mortality_content_records, persist_and_lower_rpg_authored_content, record_mortality_event,
        validate_mortality_policy, validate_mortality_state, validate_rpg_authored_content_records,
    },
    rpg_mortality_test_fixture::ashen_reach_mortality_policy,
};

#[test]
fn issue1060_constitution_drives_durability_stats() {
    let policy = ashen_reach_mortality_policy();
    let stats = derive_mortality_stats(16, &policy).unwrap();
    let aging = evaluate_mortality_aging(&living_state(16), &policy).unwrap();

    assert_eq!(stats.constitution_hit_points, 128);
    assert_eq!(stats.regen_per_tick, 32);
    assert_eq!(stats.death_threshold_hit_points, -44);
    assert_eq!(aging.age_band, MortalityAgeBand::Adult);
    assert_eq!(aging.maximum_lifespan_years, 95);
}

#[test]
fn issue1060_issue1455_persists_mortality_policy_as_authored_calyx_content() {
    let policy = ashen_reach_mortality_policy();
    let records = mortality_content_records(&policy).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault) = content_store("mortality");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 12);
    assert_eq!(lowered.row_count, 12);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/mortality/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.kind == EngineContentKind::Lore
                && row.surface_kind == "mortality_policy"
                && row.id == "server.ashen_reach.mortality.default")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.kind == EngineContentKind::Unit
                && row.surface_kind == "mortality_race_lifespan"
                && row.id == "mortality.lifespan.race.ashen_human")
    );
    assert!(
        records
            .iter()
            .all(|record| record.payload["built_in_map_required"] != true)
    );
    assert!(
        records
            .iter()
            .all(|record| record.payload["grants_map_or_minimap"] != true)
    );
    assert!(
        records
            .iter()
            .all(|record| record.payload["grants_gps_markers"] != true)
    );
}

#[test]
fn issue1060_every_third_death_costs_permanent_constitution() {
    let policy = ashen_reach_mortality_policy();
    let mut state = living_state(7);
    state.total_deaths = 5;
    state.deaths_since_constitution_loss = 2;

    let readback = record_mortality_event(&state, &policy, &combat_event()).unwrap();

    assert!(readback.constitution_lost_this_event);
    assert_eq!(readback.constitution_before, 7);
    assert_eq!(readback.constitution_after, 6);
    assert_eq!(readback.total_deaths_after, 6);
    assert_eq!(readback.deaths_since_loss_after, 0);
    assert!(!readback.permanently_dead);
    assert_eq!(readback.derived_after.constitution_hit_points, 48);
}

#[test]
fn issue1060_constitution_three_permanently_dies_and_carves_wall() {
    let policy = ashen_reach_mortality_policy();
    let mut state = living_state(4);
    state.total_deaths = 2;
    state.deaths_since_constitution_loss = 2;

    let readback = record_mortality_event(
        &state,
        &policy,
        &combat_event().witnessed_by("npc.wall.witness"),
    )
    .unwrap();

    let entry = readback.mourning_wall_entry.as_ref().unwrap();
    assert!(readback.permanently_dead);
    assert_eq!(
        readback.permanent_death_reason.as_deref(),
        Some("constitution_depleted")
    );
    assert_eq!(readback.constitution_after, 3);
    assert_eq!(entry.carved_name, "Aren Vael");
    assert_eq!(entry.final_constitution, 3);
    assert_eq!(entry.source_issue, 1060);
    assert_eq!(
        readback.after_state.mourning_wall_entry_id.as_deref(),
        Some("mourning.wall.char.aren_vael.3")
    );
}

#[test]
fn issue1060_race_dependent_aging_can_permanently_end_a_life() {
    let policy = ashen_reach_mortality_policy();
    let mut human = living_state(16);
    human.age_years = 95;
    let mut elf = human.clone();
    elf.race_id = "race.ashen_elf".to_string();

    let human_aging = evaluate_mortality_aging(&human, &policy).unwrap();
    let elf_aging = evaluate_mortality_aging(&elf, &policy).unwrap();
    let old_age = MortalityEvent::new(
        "death.old_age.aren_vael",
        MortalityEventKind::OldAge,
        "Kept the Last Fire until the hand could hold no torch.",
    );
    let readback = record_mortality_event(&human, &policy, &old_age).unwrap();

    assert_eq!(human_aging.age_band, MortalityAgeBand::OldAgeDue);
    assert_eq!(elf_aging.age_band, MortalityAgeBand::Adult);
    assert!(readback.permanently_dead);
    assert_eq!(readback.constitution_after, 16);
    assert_eq!(readback.permanent_death_reason.as_deref(), Some("old_age"));
    assert_eq!(readback.total_deaths_after, 1);
}

#[test]
fn issue1060_mortality_fails_closed_on_invalid_edges() {
    let policy = ashen_reach_mortality_policy();

    let mut bad_policy = policy.clone();
    bad_policy.deaths_per_constitution_loss = 0;
    assert_code(
        validate_mortality_policy(&bad_policy),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut unknown_race = living_state(16);
    unknown_race.race_id = "race.missing".to_string();
    assert_code(
        validate_mortality_state(&unknown_race, &policy),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let live_at_threshold = living_state(3);
    assert_code(
        validate_mortality_state(&live_at_threshold, &policy),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut near_final = living_state(4);
    near_final.deaths_since_constitution_loss = 2;
    let dead = record_mortality_event(&near_final, &policy, &combat_event()).unwrap();
    assert_code(
        record_mortality_event(&dead.after_state, &policy, &combat_event()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let early_old_age = MortalityEvent::new(
        "death.old_age.too_early",
        MortalityEventKind::OldAge,
        "This should not bypass the race lifespan.",
    );
    assert_code(
        record_mortality_event(&living_state(16), &policy, &early_old_age),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut duplicate_lifespan = policy;
    duplicate_lifespan
        .race_lifespans
        .push(duplicate_lifespan.race_lifespans[0].clone());
    assert_code(
        validate_mortality_policy(&duplicate_lifespan),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn living_state(current_constitution: i16) -> MortalityCharacterState {
    MortalityCharacterState {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        character_id: "char.aren_vael".to_string(),
        character_name: "Aren Vael".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 12,
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
    crate::test_temp::TestTempDir::new(&format!("issue1060-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
