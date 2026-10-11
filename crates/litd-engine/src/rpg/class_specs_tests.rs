use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AbilityAcquisitionCost, AttributeScores, ClassFount, ClassRaceAccess, PrimeStat,
        RaceAccessRule, RaceAlignmentException, WARRIOR_CLASS_ID, class_spec_content_records,
        class_spec_from_content_payload, class_spec_readback,
        persist_and_lower_rpg_authored_content, validate_class_race_access, validate_class_spec,
        validate_rpg_authored_content_records,
    },
    rpg_class_specs_test_fixture::{known_races, warrior_class_spec},
};

#[test]
fn issue1750_class_spec_calyx_payload_round_trips_to_full_spec() {
    let authored = warrior_class_spec();
    let record = &class_spec_content_records(&authored).unwrap()[0];
    let decoded = class_spec_from_content_payload(&record.payload).unwrap();
    assert_eq!(decoded, authored);
}

#[test]
fn issue1020_warrior_class_spec_matches_source_facts_and_canon() {
    let spec = warrior_class_spec();
    let readback = class_spec_readback(&spec).unwrap();

    assert_eq!(readback.class_id, WARRIOR_CLASS_ID);
    assert_eq!(readback.display_name, "Warrior");
    assert_eq!(readback.player_facing_canon_name, "Warrior");
    assert_eq!(readback.source_issue, 1020);
    assert_eq!(readback.prime_stat, PrimeStat::Strength);
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert!(!readback.empowerment);
    assert_eq!(readback.race_access, ClassRaceAccess::All);
    assert_eq!(readback.allowed_weapon_count, 10);
    assert_eq!(readback.allowed_armor_count, 3);
    assert_eq!(readback.leveled_ability_count, 61);
    assert_eq!(readback.specialist_ability_count, 7);
    assert_eq!(readback.specialization_count, 10);
    assert_eq!(readback.specialization_choice_count, 2);
    assert_eq!(readback.legacy_count, 36);
    assert_eq!(readback.legacy_choice_count, 2);
    assert_eq!(readback.visual_renderer, "Rust/wgpu");
    assert_eq!(readback.signature_vfx_count, 5);
    assert_eq!(readback.class_spec_sha256.len(), 64);
}

#[test]
fn issue1020_warrior_class_spec_persists_as_authored_calyx_content() {
    let spec = warrior_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    let records = class_spec_content_records(&spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("warrior-class-spec");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(validation.record_count, 1);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(records[0].anchors.is_empty());
    assert_eq!(row.kind, EngineContentKind::Unit);
    assert_eq!(row.id, WARRIOR_CLASS_ID);
    assert_eq!(row.payload["surface_kind"], "class_spec");
    assert_eq!(row.payload["class_id"], WARRIOR_CLASS_ID);
    assert_eq!(row.payload["prime_stat"], "strength");
    assert_eq!(
        row.payload["class_spec_sha256"],
        serde_json::json!(readback.class_spec_sha256)
    );
    assert_ne!(row.payload["built_in_map_required"], true);
    assert_ne!(row.payload["grants_map_or_minimap"], true);
    assert_ne!(row.payload["grants_gps_markers"], true);
    assert!(
        row.source_path
            .starts_with("editor://rpg/ashen_reach/class_specs/")
    );
    assert_eq!(lowered.row_count, 1);
    assert_eq!(lowered.entities[0].surface_kind, "class_spec");
    assert!(lowered.entities[0].lowered_field_count > 0);
}

#[test]
fn issue1020_warrior_class_spec_fail_closed_edges() {
    let mut wrong_schema = warrior_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut invalid_stat_req = warrior_class_spec();
    invalid_stat_req.stat_minimums = AttributeScores::new(200, 0, 0, 0, 0, 0);
    assert_code(
        validate_class_spec(&invalid_stat_req),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut stale_renderer = warrior_class_spec();
    stale_renderer.visual.renderer = "g3n-native".to_string();
    assert_code(
        validate_class_spec(&stale_renderer),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_specialization = warrior_class_spec();
    dangling_specialization.leveled_abilities[0].specialization_ids =
        vec!["spec.weapon.nonexistent".to_string()];
    assert_code(
        validate_class_spec(&dangling_specialization),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1020_warrior_race_access_flags_unknown_race() {
    let races = known_races();

    // Happy path: `All` needs no roster cross-check.
    let open = warrior_class_spec();
    validate_class_race_access(&open, &races).unwrap();

    // A restriction that names only known races passes.
    let mut known_only = warrior_class_spec();
    known_only.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.ashen_human".to_string()],
    };
    validate_class_race_access(&known_only, &races).unwrap();

    // A restriction that names a non-existent race is flagged.
    let mut dangling_race = warrior_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec![
            "race.ashen_human".to_string(),
            "race.does_not_exist".to_string(),
        ],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &races),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

// Schema-v2 (#1718) fail-loud edges: every new typed field rejects malformed
// values with the structured RPG config diagnostic, built from the Warrior spec.
#[test]
fn issue1718_class_spec_v2_fields_fail_closed() {
    const CODE: &str = "LITD-RPG-STATS-INVALID-CONFIG";

    // gap 4: an ability tagged to the same specialization twice.
    let mut dup_membership = warrior_class_spec();
    dup_membership.leveled_abilities[0].specialization_ids =
        vec!["spec.weapon.axe".to_string(), "spec.weapon.axe".to_string()];
    assert_code(validate_class_spec(&dup_membership), CODE);

    // gap 1: a point cost on a general (non-specialist) ability.
    let mut cost_without_spec = warrior_class_spec();
    cost_without_spec.leveled_abilities[0].acquisition = Some(AbilityAcquisitionCost {
        cost: 3,
        cumulative_cost: 3,
        prerequisite_ability_id: None,
    });
    assert_code(validate_class_spec(&cost_without_spec), CODE);

    // gap 1: cumulative cost that does not equal cost when there is no prereq.
    let mut bad_cumulative = warrior_class_spec();
    let specialist = bad_cumulative
        .leveled_abilities
        .iter_mut()
        .find(|ability| !ability.specialization_ids.is_empty())
        .expect("warrior has specialist abilities");
    specialist.acquisition = Some(AbilityAcquisitionCost {
        cost: 3,
        cumulative_cost: 9,
        prerequisite_ability_id: None,
    });
    assert_code(validate_class_spec(&bad_cumulative), CODE);

    // gap 7: an Unresolved fount with an empty reason hides the open decision.
    let mut empty_reason = warrior_class_spec();
    empty_reason.fount = ClassFount::Unresolved {
        reason: String::new(),
    };
    assert_code(validate_class_spec(&empty_reason), CODE);

    // gap 2: duplicate alignment-exception race.
    let mut dup_exception = warrior_class_spec();
    dup_exception.alignment_exceptions = vec![
        RaceAlignmentException {
            race_id: "race.ashen_human".to_string(),
            allowed: RaceAccessRule::Any,
        },
        RaceAlignmentException {
            race_id: "race.ashen_human".to_string(),
            allowed: RaceAccessRule::Any,
        },
    ];
    assert_code(validate_class_spec(&dup_exception), CODE);

    // gap 2: an alignment exception for a race that cannot even take the class.
    let mut exception_unknown_race = warrior_class_spec();
    exception_unknown_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.ashen_human".to_string()],
    };
    exception_unknown_race.alignment_exceptions = vec![RaceAlignmentException {
        race_id: "race.elf".to_string(),
        allowed: RaceAccessRule::Any,
    }];
    assert_code(validate_class_spec(&exception_unknown_race), CODE);
}

fn assert_code(result: Result<(), EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}

// #1535: TestTempDir auto-cleans on Drop.
fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1020-{label}"))
}
