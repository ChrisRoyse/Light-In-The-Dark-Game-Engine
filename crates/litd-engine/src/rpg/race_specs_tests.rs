use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        ASHEN_HUMAN_RACE_ID, AttributeBuildInput, AttributeScores, DerivedStatFormulaTable,
        DerivedStats, ELF_RACE_ID, RaceAccessRule, RaceClassAccess, RaceResistanceProfile,
        RaceSize, StatModifier, StatModifierSource, derive_character_stats,
        persist_and_lower_rpg_authored_content, race_spec_content_records, race_spec_readback,
        validate_race_spec, validate_rpg_authored_content_records,
    },
    rpg_race_specs_test_fixture::{
        elf_race_attribute_caps, elf_race_spec, human_race_attribute_caps, human_race_spec,
    },
};

#[test]
fn issue999_human_race_spec_matches_source_facts_and_canon() {
    let spec = human_race_spec();
    let readback = race_spec_readback(&spec).unwrap();

    assert_eq!(readback.race_id, ASHEN_HUMAN_RACE_ID);
    assert_eq!(readback.display_name, "Human");
    assert_eq!(readback.player_facing_canon_name, "Human");
    assert_eq!(readback.source_issue, 999);
    assert_eq!(
        readback.stat_caps,
        AttributeScores::new(20, 20, 20, 20, 20, 20)
    );
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert_eq!(readback.size, RaceSize::Medium);
    assert_eq!(readback.resistance_count, 0);
    assert_eq!(readback.vulnerability_count, 0);
    assert_eq!(readback.innate_ability_count, 0);
    assert!(!readback.infravision);
    assert_eq!(readback.class_access, RaceClassAccess::All);
    assert_eq!(readback.experience_penalty_percent, 0);
    assert_eq!(readback.hometown_count, 3);
    assert_eq!(readback.visual_renderer, "Rust/wgpu");
    assert_eq!(readback.animation_count, 8);
    assert_eq!(readback.race_spec_sha256.len(), 64);
}

#[test]
fn issue999_human_caps_feed_existing_attribute_pipeline() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = AttributeBuildInput {
        character_id: "char.galdmere.wayfarer".to_string(),
        race_id: ASHEN_HUMAN_RACE_ID.to_string(),
        level: 5,
        base: AttributeScores::new(20, 20, 20, 20, 20, 20),
        race_caps: vec![human_race_attribute_caps()],
        modifiers: Vec::new(),
    };
    let readback = derive_character_stats(&input, &table).unwrap();

    assert_eq!(
        readback.race_caps,
        AttributeScores::new(20, 20, 20, 20, 20, 20)
    );
    assert_eq!(
        readback.effective_attributes,
        AttributeScores::new(20, 20, 20, 20, 20, 20)
    );
    assert!(readback.cap_clamps.is_empty());
    assert_eq!(readback.race_id, ASHEN_HUMAN_RACE_ID);
}

#[test]
fn issue999_human_race_spec_persists_as_authored_calyx_content() {
    let spec = human_race_spec();
    let records = race_spec_content_records(&spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("human-race-spec");
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
    assert_eq!(row.id, ASHEN_HUMAN_RACE_ID);
    assert_eq!(row.payload["surface_kind"], "race_spec");
    assert_eq!(row.payload["race_id"], ASHEN_HUMAN_RACE_ID);
    assert_eq!(
        row.payload["stat_caps"],
        serde_json::json!(AttributeScores::new(20, 20, 20, 20, 20, 20))
    );
    assert_ne!(row.payload["built_in_map_required"], true);
    assert_ne!(row.payload["grants_map_or_minimap"], true);
    assert_ne!(row.payload["grants_gps_markers"], true);
    assert!(
        row.source_path
            .starts_with("editor://rpg/ashen_reach/race_specs/")
    );
    assert_eq!(lowered.row_count, 1);
    assert_eq!(lowered.entities[0].surface_kind, "race_spec");
    assert!(lowered.entities[0].lowered_field_count > 0);
}

#[test]
fn issue999_human_race_spec_fail_closed_edges() {
    let mut wrong_schema = human_race_spec();
    wrong_schema.schema = "litd.rpg.race_spec.bad".to_string();
    assert_code(
        validate_race_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut stale_renderer = human_race_spec();
    stale_renderer.visual.renderer = "g3n-native".to_string();
    assert_code(
        validate_race_spec(&stale_renderer),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut invalid_vision = human_race_spec();
    invalid_vision.vision.infravision_tiles = 6;
    assert_code(
        validate_race_spec(&invalid_vision),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut duplicate_resistance = human_race_spec();
    duplicate_resistance.resistances = vec![
        RaceResistanceProfile {
            resistance_id: "fire".to_string(),
            amount: 5,
        },
        RaceResistanceProfile {
            resistance_id: "fire".to_string(),
            amount: 7,
        },
    ];
    assert_code(
        validate_race_spec(&duplicate_resistance),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut missing_hometown = human_race_spec();
    missing_hometown.hometowns.clear();
    assert_code(
        validate_race_spec(&missing_hometown),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1000_elf_race_spec_matches_source_facts_and_canon() {
    let spec = elf_race_spec();
    let readback = race_spec_readback(&spec).unwrap();

    assert_eq!(readback.race_id, ELF_RACE_ID);
    assert_eq!(readback.display_name, "Elf");
    assert_eq!(readback.player_facing_canon_name, "Elf");
    assert_eq!(readback.source_issue, 1000);
    // CF Max Stats captured verbatim: STR 18, INT 25, WIS 21, DEX 23, CON 16, CHA 21.
    assert_eq!(
        readback.stat_caps,
        AttributeScores::new(18, 25, 21, 23, 16, 21)
    );
    // CF: all High Elves must begin Good.
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["good".to_string()],
        }
    );
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert_eq!(readback.size, RaceSize::Medium);
    // CF Resistant: Curse / Vulnerable: Iron, encoded via the #1631 RaceAffinity vocabulary.
    assert_eq!(readback.resistance_count, 1);
    assert_eq!(readback.vulnerability_count, 1);
    // CF Inherent Abilities: Swiftstrike.
    assert_eq!(readback.innate_ability_count, 1);
    // CF: no infravision entry (surface-dwellers).
    assert!(!readback.infravision);
    // CF class denylist encoded as AllExcept.
    assert_eq!(
        readback.class_access,
        RaceClassAccess::AllExcept {
            class_ids: vec![
                "class.wildbinder".to_string(),
                "class.blackoath_knight".to_string(),
                "class.gravebinder".to_string(),
                "class.hushblade".to_string(),
                "class.scourge_priest".to_string(),
            ],
        }
    );
    // CF Experience Penalty: 400.
    assert_eq!(readback.experience_penalty_percent, 400);
    assert_eq!(readback.hometown_count, 1);
    assert_eq!(readback.visual_renderer, "Rust/wgpu");
    assert_eq!(readback.animation_count, 8);
    assert_eq!(readback.race_spec_sha256.len(), 64);
}

#[test]
fn issue1000_elf_caps_feed_existing_attribute_pipeline() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = AttributeBuildInput {
        character_id: "char.name_keeper.warden".to_string(),
        race_id: ELF_RACE_ID.to_string(),
        // Base sits at the elf caps; a +10 CON gear modifier pushes the uncapped value
        // above the elf CON cap of 16, proving the cap actually clamps in the pipeline.
        level: 5,
        base: AttributeScores::new(18, 25, 21, 23, 16, 21),
        race_caps: vec![elf_race_attribute_caps()],
        modifiers: vec![StatModifier {
            modifier_id: "gear.iron_band".to_string(),
            source: StatModifierSource::Gear,
            attributes: AttributeScores::new(0, 0, 0, 0, 10, 0),
            derived: DerivedStats::ZERO,
            resistances: Vec::new(),
        }],
    };
    let readback = derive_character_stats(&input, &table).unwrap();

    assert_eq!(
        readback.race_caps,
        AttributeScores::new(18, 25, 21, 23, 16, 21)
    );
    // Uncapped CON would be 26; the elf cap clamps it to 16.
    assert_eq!(readback.uncapped_attributes.constitution, 26);
    assert_eq!(readback.effective_attributes.constitution, 16);
    assert!(
        readback
            .cap_clamps
            .iter()
            .any(|clamp| clamp.attribute == "constitution" && clamp.cap == 16)
    );
    assert_eq!(readback.race_id, ELF_RACE_ID);
}

#[test]
fn issue1000_elf_race_spec_persists_and_reads_back_from_calyx_store() {
    let spec = elf_race_spec();
    let records = race_spec_content_records(&spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("elf-race-spec");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    // Source-of-truth read-back: fetch from the persisted store, not the return value.
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(validation.record_count, 1);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert_eq!(row.kind, EngineContentKind::Unit);
    assert_eq!(row.id, ELF_RACE_ID);
    assert_eq!(row.payload["surface_kind"], "race_spec");
    assert_eq!(row.payload["race_id"], ELF_RACE_ID);
    assert_eq!(
        row.payload["stat_caps"],
        serde_json::json!(AttributeScores::new(18, 25, 21, 23, 16, 21))
    );
    assert_eq!(row.payload["experience_penalty_percent"], 400);
    // The denylist survives the round-trip through the store intact.
    assert_eq!(
        row.payload["class_access"]["all_except"]["class_ids"][0],
        "class.wildbinder"
    );
    assert_ne!(row.payload["built_in_map_required"], true);
    assert_ne!(row.payload["grants_map_or_minimap"], true);
    assert_ne!(row.payload["grants_gps_markers"], true);
    assert!(
        row.source_path
            .starts_with("editor://rpg/ashen_reach/race_specs/")
    );
    assert_eq!(lowered.row_count, 1);
    assert_eq!(lowered.entities[0].surface_kind, "race_spec");
    assert!(lowered.entities[0].lowered_field_count > 0);
}

#[test]
fn issue1000_elf_out_of_range_stat_cap_fails_closed() {
    // FSV edge (a): an out-of-range stat cap must be rejected loud.
    let mut over_cap = elf_race_spec();
    over_cap.stat_caps = AttributeScores::new(200, 25, 21, 23, 16, 21);
    assert_code(
        validate_race_spec(&over_cap),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut zero_cap = elf_race_spec();
    zero_cap.stat_caps = AttributeScores::new(0, 25, 21, 23, 16, 21);
    assert_code(
        validate_race_spec(&zero_cap),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1000_elf_class_access_integrity_edges() {
    // FSV edge (b): class_access referential integrity.
    // A MALFORMED class token is rejected loud by the token validator.
    let mut malformed = elf_race_spec();
    malformed.class_access = RaceClassAccess::AllExcept {
        class_ids: vec!["class.not a token!".to_string()],
    };
    assert_code(
        validate_race_spec(&malformed),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // An empty denylist is rejected loud.
    let mut empty = elf_race_spec();
    empty.class_access = RaceClassAccess::AllExcept {
        class_ids: Vec::new(),
    };
    assert_code(validate_race_spec(&empty), "LITD-RPG-STATS-INVALID-CONFIG");

    // #1631: per-spec validation stays FORM-only by design (mirrors the #1704 area
    // pattern, where referential integrity is a CORPUS-level check, not per-spec). A
    // well-formed but non-existent class id passes validate_race_spec here; existence
    // is enforced separately by validate_race_class_access_against_registry over the
    // whole authored corpus (see class_access_corpus_tests.rs).
    let mut ghost_class = elf_race_spec();
    ghost_class.class_access = RaceClassAccess::Restricted {
        class_ids: vec!["class.does_not_exist_anywhere".to_string()],
    };
    assert!(
        validate_race_spec(&ghost_class).is_ok(),
        "per-spec validation is form-only; existence is a corpus-level check (#1631)"
    );
}

#[test]
fn issue1000_elf_race_spec_fail_closed_edges() {
    let mut wrong_schema = elf_race_spec();
    wrong_schema.schema = "litd.rpg.race_spec.bad".to_string();
    assert_code(
        validate_race_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut missing_canon = elf_race_spec();
    missing_canon.canon_issues = vec![984, 985];
    assert_code(
        validate_race_spec(&missing_canon),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut unknown_resistance = elf_race_spec();
    unknown_resistance.resistances = vec![RaceResistanceProfile {
        resistance_id: "not_a_valid_channel".to_string(),
        amount: 10,
    }];
    // Confirms WHY curse is unencodable today: "curse" is not in the resistance vocabulary.
    assert_code(
        validate_race_spec(&unknown_resistance),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
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
    crate::test_temp::TestTempDir::new(&format!("issue999-{label}"))
}
