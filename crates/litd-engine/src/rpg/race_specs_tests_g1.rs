use super::rpg_race_specs_fixture_g1::{
    ashkin_deep_race_attribute_caps, ashkin_deep_race_spec, ashkin_mixed_race_attribute_caps,
    ashkin_mixed_race_spec, half_elf_race_attribute_caps, half_elf_race_spec,
};
use super::rpg_race_specs_tests_support::{assert_code, content_store};
use crate::{
    calyx_content::EngineContentKind,
    rpg::{
        ASHKIN_DEEP_RACE_ID, ASHKIN_MIXED_RACE_ID, AttributeBuildInput, AttributeScores,
        DerivedStatFormulaTable, DerivedStats, HALF_ELF_RACE_ID, RaceAccessRule, RaceClassAccess,
        RaceResistanceProfile, RaceSize, StatModifier, StatModifierSource, derive_character_stats,
        persist_and_lower_rpg_authored_content, race_spec_content_records, race_spec_readback,
        validate_race_spec, validate_rpg_authored_content_records,
    },
};

#[test]
fn issue1001_half_elf_race_spec_matches_source_facts_and_canon() {
    let spec = half_elf_race_spec();
    let readback = race_spec_readback(&spec).unwrap();
    assert_eq!(readback.race_id, HALF_ELF_RACE_ID);
    assert_eq!(readback.display_name, "Half-Elf");
    assert_eq!(readback.source_issue, 1001);
    assert_eq!(
        readback.stat_caps,
        AttributeScores::new(19, 21, 21, 21, 19, 20)
    );
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert_eq!(readback.size, RaceSize::Medium);
    assert_eq!(readback.resistance_count, 0);
    assert_eq!(readback.vulnerability_count, 1);
    assert_eq!(readback.innate_ability_count, 2);
    assert!(readback.infravision);
    assert_eq!(
        readback.class_access,
        RaceClassAccess::AllExcept {
            class_ids: vec!["class.reach_ravager".to_string()]
        }
    );
    assert_eq!(readback.experience_penalty_percent, 0);
    assert_eq!(readback.visual_renderer, "Rust/wgpu");
    assert_eq!(readback.animation_count, 8);
    assert_eq!(readback.race_spec_sha256.len(), 64);
}

#[test]
fn issue1001_half_elf_caps_feed_existing_attribute_pipeline() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = AttributeBuildInput {
        character_id: "char.half_elf.probe".to_string(),
        race_id: HALF_ELF_RACE_ID.to_string(),
        level: 5,
        base: AttributeScores::new(19, 21, 21, 21, 19, 20),
        race_caps: vec![half_elf_race_attribute_caps()],
        modifiers: vec![StatModifier {
            modifier_id: "gear.probe_band".to_string(),
            source: StatModifierSource::Gear,
            attributes: AttributeScores::new(0, 0, 0, 0, 10, 0),
            derived: DerivedStats::ZERO,
            resistances: Vec::new(),
        }],
    };
    let readback = derive_character_stats(&input, &table).unwrap();
    assert_eq!(
        readback.race_caps,
        AttributeScores::new(19, 21, 21, 21, 19, 20)
    );
    assert_eq!(readback.uncapped_attributes.constitution, 29);
    assert_eq!(readback.effective_attributes.constitution, 19);
    assert!(
        readback
            .cap_clamps
            .iter()
            .any(|c| c.attribute == "constitution" && c.cap == 19)
    );
    assert_eq!(readback.race_id, HALF_ELF_RACE_ID);
}

#[test]
fn issue1001_half_elf_race_spec_persists_and_reads_back_from_calyx_store() {
    let spec = half_elf_race_spec();
    let records = race_spec_content_records(&spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("half-elf-race-spec");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(validation.record_count, 1);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert_eq!(row.kind, EngineContentKind::Unit);
    assert_eq!(row.id, HALF_ELF_RACE_ID);
    assert_eq!(row.payload["surface_kind"], "race_spec");
    assert_eq!(row.payload["race_id"], HALF_ELF_RACE_ID);
    assert_eq!(
        row.payload["stat_caps"],
        serde_json::json!(AttributeScores::new(19, 21, 21, 21, 19, 20))
    );
    assert_eq!(row.payload["experience_penalty_percent"], 0);
    assert_eq!(
        row.payload["class_access"],
        serde_json::to_value(&spec.class_access).unwrap()
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
fn issue1001_half_elf_race_spec_fail_closed_edges() {
    let mut wrong_schema = half_elf_race_spec();
    wrong_schema.schema = "litd.rpg.race_spec.bad".to_string();
    assert_code(
        validate_race_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut over_cap = half_elf_race_spec();
    over_cap.stat_caps = AttributeScores::new(200, 21, 21, 21, 19, 20);
    assert_code(
        validate_race_spec(&over_cap),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut missing_canon = half_elf_race_spec();
    missing_canon.canon_issues = vec![984, 985];
    assert_code(
        validate_race_spec(&missing_canon),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut unknown_resistance = half_elf_race_spec();
    unknown_resistance.resistances = vec![RaceResistanceProfile {
        resistance_id: "not_a_valid_channel".to_string(),
        amount: 10,
    }];
    assert_code(
        validate_race_spec(&unknown_resistance),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}
#[test]
fn issue1002_ashkin_mixed_race_spec_matches_source_facts_and_canon() {
    let spec = ashkin_mixed_race_spec();
    let readback = race_spec_readback(&spec).unwrap();
    assert_eq!(readback.race_id, ASHKIN_MIXED_RACE_ID);
    assert_eq!(readback.display_name, "Ashkin (Mixed-Line)");
    assert_eq!(readback.source_issue, 1002);
    assert_eq!(
        readback.stat_caps,
        AttributeScores::new(19, 21, 21, 21, 19, 20)
    );
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert_eq!(readback.size, RaceSize::Medium);
    assert_eq!(readback.resistance_count, 0);
    assert_eq!(readback.vulnerability_count, 1);
    assert_eq!(readback.innate_ability_count, 2);
    assert!(readback.infravision);
    assert_eq!(
        readback.class_access,
        RaceClassAccess::AllExcept {
            class_ids: vec!["class.reach_ravager".to_string()]
        }
    );
    assert_eq!(readback.experience_penalty_percent, 0);
    assert_eq!(readback.visual_renderer, "Rust/wgpu");
    assert_eq!(readback.animation_count, 8);
    assert_eq!(readback.race_spec_sha256.len(), 64);
}

#[test]
fn issue1002_ashkin_mixed_caps_feed_existing_attribute_pipeline() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = AttributeBuildInput {
        character_id: "char.ashkin_mixed.probe".to_string(),
        race_id: ASHKIN_MIXED_RACE_ID.to_string(),
        level: 5,
        base: AttributeScores::new(19, 21, 21, 21, 19, 20),
        race_caps: vec![ashkin_mixed_race_attribute_caps()],
        modifiers: vec![StatModifier {
            modifier_id: "gear.probe_band".to_string(),
            source: StatModifierSource::Gear,
            attributes: AttributeScores::new(0, 0, 0, 0, 10, 0),
            derived: DerivedStats::ZERO,
            resistances: Vec::new(),
        }],
    };
    let readback = derive_character_stats(&input, &table).unwrap();
    assert_eq!(
        readback.race_caps,
        AttributeScores::new(19, 21, 21, 21, 19, 20)
    );
    assert_eq!(readback.uncapped_attributes.constitution, 29);
    assert_eq!(readback.effective_attributes.constitution, 19);
    assert!(
        readback
            .cap_clamps
            .iter()
            .any(|c| c.attribute == "constitution" && c.cap == 19)
    );
    assert_eq!(readback.race_id, ASHKIN_MIXED_RACE_ID);
}

#[test]
fn issue1002_ashkin_mixed_race_spec_persists_and_reads_back_from_calyx_store() {
    let spec = ashkin_mixed_race_spec();
    let records = race_spec_content_records(&spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("ashkin-mixed-race-spec");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(validation.record_count, 1);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert_eq!(row.kind, EngineContentKind::Unit);
    assert_eq!(row.id, ASHKIN_MIXED_RACE_ID);
    assert_eq!(row.payload["surface_kind"], "race_spec");
    assert_eq!(row.payload["race_id"], ASHKIN_MIXED_RACE_ID);
    assert_eq!(
        row.payload["stat_caps"],
        serde_json::json!(AttributeScores::new(19, 21, 21, 21, 19, 20))
    );
    assert_eq!(row.payload["experience_penalty_percent"], 0);
    assert_eq!(
        row.payload["class_access"],
        serde_json::to_value(&spec.class_access).unwrap()
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
fn issue1002_ashkin_mixed_race_spec_fail_closed_edges() {
    let mut wrong_schema = ashkin_mixed_race_spec();
    wrong_schema.schema = "litd.rpg.race_spec.bad".to_string();
    assert_code(
        validate_race_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut over_cap = ashkin_mixed_race_spec();
    over_cap.stat_caps = AttributeScores::new(200, 21, 21, 21, 19, 20);
    assert_code(
        validate_race_spec(&over_cap),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut missing_canon = ashkin_mixed_race_spec();
    missing_canon.canon_issues = vec![984, 985];
    assert_code(
        validate_race_spec(&missing_canon),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut unknown_resistance = ashkin_mixed_race_spec();
    unknown_resistance.resistances = vec![RaceResistanceProfile {
        resistance_id: "not_a_valid_channel".to_string(),
        amount: 10,
    }];
    assert_code(
        validate_race_spec(&unknown_resistance),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}
#[test]
fn issue1003_ashkin_deep_race_spec_matches_source_facts_and_canon() {
    let spec = ashkin_deep_race_spec();
    let readback = race_spec_readback(&spec).unwrap();
    assert_eq!(readback.race_id, ASHKIN_DEEP_RACE_ID);
    assert_eq!(readback.display_name, "Ashkin (Deep-Line)");
    assert_eq!(readback.source_issue, 1003);
    assert_eq!(
        readback.stat_caps,
        AttributeScores::new(18, 24, 21, 24, 16, 18)
    );
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["evil".to_string()]
        }
    );
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert_eq!(readback.size, RaceSize::Medium);
    assert_eq!(readback.resistance_count, 0);
    assert_eq!(readback.vulnerability_count, 1);
    assert_eq!(readback.innate_ability_count, 2);
    assert!(readback.infravision);
    assert_eq!(
        readback.class_access,
        RaceClassAccess::Restricted {
            class_ids: vec![
                "class.ashen_warrior".to_string(),
                "class.galdmere_shade".to_string(),
                "class.sinewright".to_string(),
                "class.currentbraider".to_string(),
                "class.gravebinder".to_string(),
                "class.circlebinder".to_string(),
                "class.hushblade".to_string(),
                "class.blackoath_knight".to_string(),
                "class.lifewarden".to_string(),
                "class.scourge_priest".to_string(),
                "class.thornwarden".to_string()
            ]
        }
    );
    assert_eq!(readback.experience_penalty_percent, 400);
    assert_eq!(readback.visual_renderer, "Rust/wgpu");
    assert_eq!(readback.animation_count, 8);
    assert_eq!(readback.race_spec_sha256.len(), 64);
}

#[test]
fn issue1003_ashkin_deep_caps_feed_existing_attribute_pipeline() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = AttributeBuildInput {
        character_id: "char.ashkin_deep.probe".to_string(),
        race_id: ASHKIN_DEEP_RACE_ID.to_string(),
        level: 5,
        base: AttributeScores::new(18, 24, 21, 24, 16, 18),
        race_caps: vec![ashkin_deep_race_attribute_caps()],
        modifiers: vec![StatModifier {
            modifier_id: "gear.probe_band".to_string(),
            source: StatModifierSource::Gear,
            attributes: AttributeScores::new(0, 0, 0, 0, 10, 0),
            derived: DerivedStats::ZERO,
            resistances: Vec::new(),
        }],
    };
    let readback = derive_character_stats(&input, &table).unwrap();
    assert_eq!(
        readback.race_caps,
        AttributeScores::new(18, 24, 21, 24, 16, 18)
    );
    assert_eq!(readback.uncapped_attributes.constitution, 26);
    assert_eq!(readback.effective_attributes.constitution, 16);
    assert!(
        readback
            .cap_clamps
            .iter()
            .any(|c| c.attribute == "constitution" && c.cap == 16)
    );
    assert_eq!(readback.race_id, ASHKIN_DEEP_RACE_ID);
}

#[test]
fn issue1003_ashkin_deep_race_spec_persists_and_reads_back_from_calyx_store() {
    let spec = ashkin_deep_race_spec();
    let records = race_spec_content_records(&spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("ashkin-deep-race-spec");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(validation.record_count, 1);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert_eq!(row.kind, EngineContentKind::Unit);
    assert_eq!(row.id, ASHKIN_DEEP_RACE_ID);
    assert_eq!(row.payload["surface_kind"], "race_spec");
    assert_eq!(row.payload["race_id"], ASHKIN_DEEP_RACE_ID);
    assert_eq!(
        row.payload["stat_caps"],
        serde_json::json!(AttributeScores::new(18, 24, 21, 24, 16, 18))
    );
    assert_eq!(row.payload["experience_penalty_percent"], 400);
    assert_eq!(
        row.payload["class_access"],
        serde_json::to_value(&spec.class_access).unwrap()
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
fn issue1003_ashkin_deep_race_spec_fail_closed_edges() {
    let mut wrong_schema = ashkin_deep_race_spec();
    wrong_schema.schema = "litd.rpg.race_spec.bad".to_string();
    assert_code(
        validate_race_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut over_cap = ashkin_deep_race_spec();
    over_cap.stat_caps = AttributeScores::new(200, 24, 21, 24, 16, 18);
    assert_code(
        validate_race_spec(&over_cap),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut missing_canon = ashkin_deep_race_spec();
    missing_canon.canon_issues = vec![984, 985];
    assert_code(
        validate_race_spec(&missing_canon),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut unknown_resistance = ashkin_deep_race_spec();
    unknown_resistance.resistances = vec![RaceResistanceProfile {
        resistance_id: "not_a_valid_channel".to_string(),
        amount: 10,
    }];
    assert_code(
        validate_race_spec(&unknown_resistance),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}
