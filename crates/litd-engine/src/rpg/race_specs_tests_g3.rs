use super::rpg_race_specs_fixture_g3::{
    deep_kin_race_attribute_caps, deep_kin_race_spec, gnome_race_attribute_caps, gnome_race_spec,
    orc_race_attribute_caps, orc_race_spec,
};
use super::rpg_race_specs_tests_support::{assert_code, content_store};
use crate::{
    calyx_content::EngineContentKind,
    rpg::{
        AttributeBuildInput, AttributeScores, DEEP_KIN_RACE_ID, DerivedStatFormulaTable,
        DerivedStats, GNOME_RACE_ID, ORC_RACE_ID, RaceAccessRule, RaceClassAccess,
        RaceResistanceProfile, RaceSize, StatModifier, StatModifierSource, derive_character_stats,
        persist_and_lower_rpg_authored_content, race_spec_content_records, race_spec_readback,
        validate_race_spec, validate_rpg_authored_content_records,
    },
};

#[test]
fn issue1007_gnome_race_spec_matches_source_facts_and_canon() {
    let spec = gnome_race_spec();
    let readback = race_spec_readback(&spec).unwrap();
    assert_eq!(readback.race_id, GNOME_RACE_ID);
    assert_eq!(readback.display_name, "Gnome");
    assert_eq!(readback.source_issue, 1007);
    assert_eq!(
        readback.stat_caps,
        AttributeScores::new(18, 23, 25, 20, 18, 19)
    );
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["neutral".to_string()]
        }
    );
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert_eq!(readback.size, RaceSize::Small);
    assert_eq!(readback.resistance_count, 1);
    assert_eq!(readback.vulnerability_count, 1);
    assert_eq!(readback.innate_ability_count, 0);
    assert!(readback.infravision);
    assert_eq!(
        readback.class_access,
        RaceClassAccess::Restricted {
            class_ids: vec![
                "class.ashen_warrior".to_string(),
                "class.galdmere_shade".to_string(),
                "class.sinewright".to_string(),
                "class.currentbraider".to_string(),
                "class.circlebinder".to_string(),
                "class.wildbinder".to_string(),
                "class.shapeshifter".to_string(),
                "class.thornwarden".to_string(),
                "class.lifewarden".to_string()
            ]
        }
    );
    assert_eq!(readback.experience_penalty_percent, 300);
    assert_eq!(readback.visual_renderer, "Rust/wgpu");
    assert_eq!(readback.animation_count, 8);
    assert_eq!(readback.race_spec_sha256.len(), 64);
}

#[test]
fn issue1007_gnome_caps_feed_existing_attribute_pipeline() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = AttributeBuildInput {
        character_id: "char.gnome.probe".to_string(),
        race_id: GNOME_RACE_ID.to_string(),
        level: 5,
        base: AttributeScores::new(18, 23, 25, 20, 18, 19),
        race_caps: vec![gnome_race_attribute_caps()],
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
        AttributeScores::new(18, 23, 25, 20, 18, 19)
    );
    assert_eq!(readback.uncapped_attributes.constitution, 28);
    assert_eq!(readback.effective_attributes.constitution, 18);
    assert!(
        readback
            .cap_clamps
            .iter()
            .any(|c| c.attribute == "constitution" && c.cap == 18)
    );
    assert_eq!(readback.race_id, GNOME_RACE_ID);
}

#[test]
fn issue1007_gnome_race_spec_persists_and_reads_back_from_calyx_store() {
    let spec = gnome_race_spec();
    let records = race_spec_content_records(&spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("gnome-race-spec");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(validation.record_count, 1);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert_eq!(row.kind, EngineContentKind::Unit);
    assert_eq!(row.id, GNOME_RACE_ID);
    assert_eq!(row.payload["surface_kind"], "race_spec");
    assert_eq!(row.payload["race_id"], GNOME_RACE_ID);
    assert_eq!(
        row.payload["stat_caps"],
        serde_json::json!(AttributeScores::new(18, 23, 25, 20, 18, 19))
    );
    assert_eq!(row.payload["experience_penalty_percent"], 300);
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
fn issue1007_gnome_race_spec_fail_closed_edges() {
    let mut wrong_schema = gnome_race_spec();
    wrong_schema.schema = "litd.rpg.race_spec.bad".to_string();
    assert_code(
        validate_race_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut over_cap = gnome_race_spec();
    over_cap.stat_caps = AttributeScores::new(200, 23, 25, 20, 18, 19);
    assert_code(
        validate_race_spec(&over_cap),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut missing_canon = gnome_race_spec();
    missing_canon.canon_issues = vec![984, 985];
    assert_code(
        validate_race_spec(&missing_canon),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut unknown_resistance = gnome_race_spec();
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
fn issue1008_deep_kin_race_spec_matches_source_facts_and_canon() {
    let spec = deep_kin_race_spec();
    let readback = race_spec_readback(&spec).unwrap();
    assert_eq!(readback.race_id, DEEP_KIN_RACE_ID);
    assert_eq!(readback.display_name, "Deep-Kin");
    assert_eq!(readback.source_issue, 1008);
    assert_eq!(
        readback.stat_caps,
        AttributeScores::new(22, 18, 22, 20, 22, 19)
    );
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["neutral".to_string()]
        }
    );
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert_eq!(readback.size, RaceSize::Small);
    assert_eq!(readback.resistance_count, 3);
    assert_eq!(readback.vulnerability_count, 1);
    assert_eq!(readback.innate_ability_count, 2);
    assert!(!readback.infravision);
    assert_eq!(
        readback.class_access,
        RaceClassAccess::Restricted {
            class_ids: vec![
                "class.circlebinder".to_string(),
                "class.lifewarden".to_string(),
                "class.currentbraider".to_string(),
                "class.thornwarden".to_string(),
                "class.shapeshifter".to_string(),
                "class.galdmere_shade".to_string(),
                "class.sinewright".to_string(),
                "class.ashen_warrior".to_string()
            ]
        }
    );
    assert_eq!(readback.experience_penalty_percent, 250);
    assert_eq!(readback.visual_renderer, "Rust/wgpu");
    assert_eq!(readback.animation_count, 8);
    assert_eq!(readback.race_spec_sha256.len(), 64);
}

#[test]
fn issue1008_deep_kin_caps_feed_existing_attribute_pipeline() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = AttributeBuildInput {
        character_id: "char.deep_kin.probe".to_string(),
        race_id: DEEP_KIN_RACE_ID.to_string(),
        level: 5,
        base: AttributeScores::new(22, 18, 22, 20, 22, 19),
        race_caps: vec![deep_kin_race_attribute_caps()],
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
        AttributeScores::new(22, 18, 22, 20, 22, 19)
    );
    assert_eq!(readback.uncapped_attributes.constitution, 32);
    assert_eq!(readback.effective_attributes.constitution, 22);
    assert!(
        readback
            .cap_clamps
            .iter()
            .any(|c| c.attribute == "constitution" && c.cap == 22)
    );
    assert_eq!(readback.race_id, DEEP_KIN_RACE_ID);
}

#[test]
fn issue1008_deep_kin_race_spec_persists_and_reads_back_from_calyx_store() {
    let spec = deep_kin_race_spec();
    let records = race_spec_content_records(&spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("deep-kin-race-spec");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(validation.record_count, 1);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert_eq!(row.kind, EngineContentKind::Unit);
    assert_eq!(row.id, DEEP_KIN_RACE_ID);
    assert_eq!(row.payload["surface_kind"], "race_spec");
    assert_eq!(row.payload["race_id"], DEEP_KIN_RACE_ID);
    assert_eq!(
        row.payload["stat_caps"],
        serde_json::json!(AttributeScores::new(22, 18, 22, 20, 22, 19))
    );
    assert_eq!(row.payload["experience_penalty_percent"], 250);
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
fn issue1008_deep_kin_race_spec_fail_closed_edges() {
    let mut wrong_schema = deep_kin_race_spec();
    wrong_schema.schema = "litd.rpg.race_spec.bad".to_string();
    assert_code(
        validate_race_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut over_cap = deep_kin_race_spec();
    over_cap.stat_caps = AttributeScores::new(200, 18, 22, 20, 22, 19);
    assert_code(
        validate_race_spec(&over_cap),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut missing_canon = deep_kin_race_spec();
    missing_canon.canon_issues = vec![984, 985];
    assert_code(
        validate_race_spec(&missing_canon),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut unknown_resistance = deep_kin_race_spec();
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
fn issue1009_orc_race_spec_matches_source_facts_and_canon() {
    let spec = orc_race_spec();
    let readback = race_spec_readback(&spec).unwrap();
    assert_eq!(readback.race_id, ORC_RACE_ID);
    assert_eq!(readback.display_name, "Orc");
    assert_eq!(readback.source_issue, 1009);
    assert_eq!(
        readback.stat_caps,
        AttributeScores::new(23, 15, 16, 18, 24, 13)
    );
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["evil".to_string()]
        }
    );
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert_eq!(readback.size, RaceSize::Medium);
    assert_eq!(readback.resistance_count, 2);
    assert_eq!(readback.vulnerability_count, 1);
    assert_eq!(readback.innate_ability_count, 0);
    assert!(!readback.infravision);
    assert_eq!(
        readback.class_access,
        RaceClassAccess::Restricted {
            class_ids: vec!["class.reach_ravager".to_string()]
        }
    );
    assert_eq!(readback.experience_penalty_percent, 100);
    assert_eq!(readback.visual_renderer, "Rust/wgpu");
    assert_eq!(readback.animation_count, 8);
    assert_eq!(readback.race_spec_sha256.len(), 64);
}

#[test]
fn issue1009_orc_caps_feed_existing_attribute_pipeline() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = AttributeBuildInput {
        character_id: "char.orc.probe".to_string(),
        race_id: ORC_RACE_ID.to_string(),
        level: 5,
        base: AttributeScores::new(23, 15, 16, 18, 24, 13),
        race_caps: vec![orc_race_attribute_caps()],
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
        AttributeScores::new(23, 15, 16, 18, 24, 13)
    );
    assert_eq!(readback.uncapped_attributes.constitution, 34);
    assert_eq!(readback.effective_attributes.constitution, 24);
    assert!(
        readback
            .cap_clamps
            .iter()
            .any(|c| c.attribute == "constitution" && c.cap == 24)
    );
    assert_eq!(readback.race_id, ORC_RACE_ID);
}

#[test]
fn issue1009_orc_race_spec_persists_and_reads_back_from_calyx_store() {
    let spec = orc_race_spec();
    let records = race_spec_content_records(&spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("orc-race-spec");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(validation.record_count, 1);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert_eq!(row.kind, EngineContentKind::Unit);
    assert_eq!(row.id, ORC_RACE_ID);
    assert_eq!(row.payload["surface_kind"], "race_spec");
    assert_eq!(row.payload["race_id"], ORC_RACE_ID);
    assert_eq!(
        row.payload["stat_caps"],
        serde_json::json!(AttributeScores::new(23, 15, 16, 18, 24, 13))
    );
    assert_eq!(row.payload["experience_penalty_percent"], 100);
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
fn issue1009_orc_race_spec_fail_closed_edges() {
    let mut wrong_schema = orc_race_spec();
    wrong_schema.schema = "litd.rpg.race_spec.bad".to_string();
    assert_code(
        validate_race_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut over_cap = orc_race_spec();
    over_cap.stat_caps = AttributeScores::new(200, 15, 16, 18, 24, 13);
    assert_code(
        validate_race_spec(&over_cap),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut missing_canon = orc_race_spec();
    missing_canon.canon_issues = vec![984, 985];
    assert_code(
        validate_race_spec(&missing_canon),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    let mut unknown_resistance = orc_race_spec();
    unknown_resistance.resistances = vec![RaceResistanceProfile {
        resistance_id: "not_a_valid_channel".to_string(),
        amount: 10,
    }];
    assert_code(
        validate_race_spec(&unknown_resistance),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}
