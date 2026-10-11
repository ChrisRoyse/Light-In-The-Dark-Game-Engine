use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AttributeScores, MortalityCharacterState, MortalityEvent, MortalityEventKind,
        RPG_MORTALITY_SCHEMA, UndeadForm, WitheringRequest, apply_withering, evaluate_undead_aging,
        persist_and_lower_rpg_authored_content, record_undead_physical_death,
        undeath_content_records, validate_rpg_authored_content_records, validate_undeath_policy,
        validate_withering_request,
    },
    rpg_mortality_test_fixture::ashen_reach_mortality_policy,
    rpg_undeath_test_fixture::ashen_reach_undeath_policy,
};

#[test]
fn issue1061_withering_resets_constitution_and_locks_undead_invariants() {
    let mortality = ashen_reach_mortality_policy();
    let undeath = ashen_reach_undeath_policy();
    let readback =
        apply_withering(&mortal_state(16, 64), &mortality, &undeath, &lich_request()).unwrap();

    assert_eq!(readback.chosen_form, UndeadForm::Lich);
    assert_eq!(readback.constitution_before, 16);
    assert_eq!(readback.constitution_after, 6);
    assert_eq!(readback.discovery_ref_count, 2);
    assert_eq!(readback.component_count, 3);
    assert!(readback.undead_state.aging_death_immune);
    assert!(readback.undead_state.hunger_thirst_immune);
    assert!(readback.undead_state.physical_kill_only);
    assert!(readback.undead_state.irreversible);
    assert!(readback.undead_state.normal_resurrection_forbidden);
    assert_eq!(
        readback.unlocked_rituals[0].ability_id,
        "ritual.vessel_siphon"
    );
    assert_eq!(readback.undead_state.source_issue, 1061);
    assert_eq!(readback.result_sha256.len(), 64);
}

#[test]
fn issue1061_issue1455_persists_undeath_policy_as_authored_calyx_content() {
    let policy = ashen_reach_undeath_policy();
    let records = undeath_content_records(&policy).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault) = content_store("undeath");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 5);
    assert_eq!(lowered.row_count, 5);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/undeath/")
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
            .any(|row| row.kind == EngineContentKind::Trigger
                && row.surface_kind == "withering_requirements"
                && row.id == "server.ashen_reach.undeath.withering_requirements")
    );
    assert!(lowered.entities.iter().any(|row| {
        row.kind == EngineContentKind::Lore && row.surface_kind == "undeath_policy"
    }));
    assert!(
        lowered
            .entities
            .iter()
            .filter(|row| {
                row.kind == EngineContentKind::Ability && row.surface_kind == "undeath_form_ritual"
            })
            .count()
            == 3
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
fn issue1061_undead_old_age_is_suppressed_not_resurrected() {
    let mortality = ashen_reach_mortality_policy();
    let undeath = ashen_reach_undeath_policy();
    let mut mortal = mortal_state(16, 95);
    mortal.total_deaths = 2;
    let readback = apply_withering(&mortal, &mortality, &undeath, &lich_request()).unwrap();
    let aging = evaluate_undead_aging(&readback.undead_state, &mortality, &undeath).unwrap();

    assert!(aging.mortal_aging.old_age_death_due);
    assert!(aging.aging_death_immune);
    assert!(aging.old_age_death_suppressed);
    assert_code(
        record_undead_physical_death(
            &readback.undead_state,
            &mortality,
            &undeath,
            &old_age_event(),
        ),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1061_physical_kill_still_erodes_constitution_to_final_death() {
    let mortality = ashen_reach_mortality_policy();
    let undeath = ashen_reach_undeath_policy();
    let mut transformed = apply_withering(
        &mortal_state(16, 64),
        &mortality,
        &undeath,
        &wraith_request(),
    )
    .unwrap()
    .undead_state;
    transformed.mortality_state.current_constitution = 4;
    transformed.mortality_state.total_deaths = 8;
    transformed.mortality_state.deaths_since_constitution_loss = 2;

    let readback = record_undead_physical_death(
        &transformed,
        &mortality,
        &undeath,
        &combat_event("death.combat.wraith_hunted"),
    )
    .unwrap();

    assert_eq!(readback.form, UndeadForm::Wraith);
    assert!(readback.physical_kill_only);
    assert_eq!(readback.mortality.constitution_before, 4);
    assert_eq!(readback.mortality.constitution_after, 3);
    assert!(readback.mortality.constitution_lost_this_event);
    assert!(readback.mortality.permanently_dead);
    assert_eq!(
        readback.mortality.permanent_death_reason.as_deref(),
        Some("constitution_depleted")
    );
    assert_eq!(
        readback
            .undead_state_after
            .mortality_state
            .mourning_wall_entry_id
            .as_deref(),
        Some("mourning.wall.char.varn_black.9")
    );
}

#[test]
fn issue1061_forms_grant_distinct_original_rituals() {
    let mortality = ashen_reach_mortality_policy();
    let undeath = ashen_reach_undeath_policy();
    let mummy = apply_withering(
        &mortal_state(16, 64),
        &mortality,
        &undeath,
        &mummy_request(),
    )
    .unwrap();
    let wraith = apply_withering(
        &mortal_state(16, 64),
        &mortality,
        &undeath,
        &wraith_request(),
    )
    .unwrap();

    assert_eq!(
        mummy.unlocked_rituals[0].ability_id,
        "ritual.flesh_memory_theft"
    );
    assert_eq!(mummy.focus_id, "focus.grave_wrappings");
    assert_eq!(
        wraith.unlocked_rituals[0].ability_id,
        "ritual.thin_veil_stride"
    );
    assert_eq!(wraith.focus_id, "focus.veil_anchor");
}

#[test]
fn issue1061_undeath_fails_closed_on_invalid_edges() {
    let mortality = ashen_reach_mortality_policy();
    let undeath = ashen_reach_undeath_policy();

    let mut low_level = lich_request();
    low_level.caster_level = 46;
    assert_code(
        validate_withering_request(&low_level, &undeath),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut missing_secret = lich_request();
    missing_secret.discovery_refs.pop();
    assert_code(
        validate_withering_request(&missing_secret, &undeath),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut wrong_class = lich_request();
    wrong_class.caster_class_id = "class.healer".to_string();
    assert_code(
        validate_withering_request(&wrong_class, &undeath),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut wrong_focus = lich_request();
    wrong_focus.prepared_focus_id = "focus.borrowed_crown".to_string();
    assert_code(
        apply_withering(&mortal_state(16, 64), &mortality, &undeath, &wrong_focus),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut reversible = undeath;
    reversible.irreversible = false;
    assert_code(
        validate_undeath_policy(&reversible),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
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
    crate::test_temp::TestTempDir::new(&format!("issue1061-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
