use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AttributeBuildInput, AttributeScores, DerivedStatFormulaTable, DerivedStats,
        RPG_STATS_SCHEMA, RaceAttributeCaps, ResistanceModifier, ResistanceReadback, StatModifier,
        StatModifierSource, attribute_content_records, derive_character_stats,
        persist_and_lower_rpg_authored_content, validate_rpg_authored_content_records,
    },
};

#[test]
fn issue960_attributes_derive_known_mud_stats() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let readback = derive_character_stats(&happy_input(), &table).unwrap();

    assert_eq!(readback.schema, RPG_STATS_SCHEMA);
    assert_eq!(
        readback.effective_attributes,
        AttributeScores::new(20, 13, 14, 20, 16, 11)
    );
    assert!(readback.cap_clamps.is_empty());
    assert_eq!(
        readback.derived,
        DerivedStats {
            hit_points: 300,
            mana: 243,
            movement: 247,
            hitroll: 52,
            damroll: 55,
            armor_class: -1,
            carry_weight: 353,
            wield_weight: 82,
        }
    );
    assert_eq!(resistance(&readback.resistances, "fire"), 42);
    assert_eq!(resistance(&readback.resistances, "cold"), 26);
    assert_eq!(resistance(&readback.resistances, "holy"), 35);
    assert_eq!(readback.gear_modifier_count, 3);
    assert_eq!(readback.affect_modifier_count, 2);
    assert_eq!(readback.input_sha256.len(), 64);
    assert_eq!(readback.table_sha256.len(), 64);
    assert_eq!(readback.result_sha256.len(), 64);
}

#[test]
fn issue960_attribute_caps_are_explicit_readback() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let mut input = happy_input();
    input.modifiers.push(affect(
        "giant_strength",
        AttributeScores::new(8, 0, 0, 0, 0, 0),
        DerivedStats::ZERO,
        &[],
    ));
    let readback = derive_character_stats(&input, &table).unwrap();

    assert_eq!(readback.uncapped_attributes.strength, 28);
    assert_eq!(readback.effective_attributes.strength, 22);
    assert_eq!(readback.cap_clamps.len(), 1);
    assert_eq!(readback.cap_clamps[0].attribute, "strength");
}

#[test]
fn issue960_attributes_fail_closed_on_invalid_edges() {
    let table = DerivedStatFormulaTable::mud_defaults();

    let mut over_cap = happy_input();
    over_cap.base.strength = 23;
    assert_code(
        derive_character_stats(&over_cap, &table),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut duplicate_modifier = happy_input();
    duplicate_modifier.modifiers.push(gear(
        "iron_sword",
        AttributeScores::ZERO,
        DerivedStats::ZERO,
        &[],
    ));
    assert_code(
        derive_character_stats(&duplicate_modifier, &table),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut unknown_resistance = happy_input();
    unknown_resistance.modifiers.push(gear(
        "void_ring",
        AttributeScores::ZERO,
        DerivedStats::ZERO,
        &[("void", 99)],
    ));
    assert_code(
        derive_character_stats(&unknown_resistance, &table),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut missing_race = happy_input();
    missing_race.race_id = "race.missing".to_string();
    assert_code(
        derive_character_stats(&missing_race, &table),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue960_issue1455_persists_attribute_tuning_as_authored_calyx_content() {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = happy_input();
    let records = attribute_content_records(&table, &input.race_caps).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("attributes");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let formula_row = store
        .get_content_for_editor(
            EngineContentKind::Lore,
            "attributes.formula_table.mud_defaults",
        )
        .unwrap();
    let caps_row = store
        .get_content_for_editor(
            EngineContentKind::Unit,
            "attributes.race_caps.race.ashen_human",
        )
        .unwrap();
    let payload_json = serde_json::to_string(&records).unwrap();

    assert_eq!(records.len(), 2);
    assert_eq!(validation.record_count, 2);
    assert_eq!(validation.acquired_map_record_count, 0);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert_eq!(
        formula_row.payload["surface_kind"],
        "attribute_formula_table"
    );
    assert_eq!(formula_row.payload["formula_count"], 8);
    assert_eq!(
        formula_row.payload["formula_table"]["hit_points"]["base"],
        20
    );
    assert_eq!(caps_row.payload["surface_kind"], "attribute_race_caps");
    assert_eq!(caps_row.payload["race_id"], "race.ashen_human");
    assert_eq!(
        caps_row.payload["caps"],
        serde_json::json!(AttributeScores::new(22, 22, 22, 22, 22, 22))
    );
    assert_ne!(formula_row.payload["built_in_map_required"], true);
    assert_ne!(formula_row.payload["grants_map_or_minimap"], true);
    assert_ne!(formula_row.payload["grants_gps_markers"], true);
    assert_ne!(caps_row.payload["built_in_map_required"], true);
    assert_ne!(caps_row.payload["grants_map_or_minimap"], true);
    assert_ne!(caps_row.payload["grants_gps_markers"], true);
    assert_eq!(lowered.row_count, 2);
    assert!(!lowered.calyx_live_query_allowed_in_hot_path);
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Lore)
            .count(),
        1
    );
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Unit)
            .count(),
        1
    );
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/attributes/")
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
            .any(|row| row.surface_kind == "attribute_formula_table")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.surface_kind == "attribute_race_caps")
    );
    assert!(!payload_json.contains("\"built_in_map_required\":true"));
    assert!(!payload_json.contains("\"grants_map_or_minimap\":true"));
    assert!(!payload_json.contains("\"grants_gps_markers\":true"));
}

pub(crate) fn happy_input() -> AttributeBuildInput {
    AttributeBuildInput {
        character_id: "char.ashen.guardian".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 12,
        base: AttributeScores::new(18, 13, 12, 17, 16, 10),
        race_caps: vec![RaceAttributeCaps {
            race_id: "race.ashen_human".to_string(),
            caps: AttributeScores::new(22, 22, 22, 22, 22, 22),
        }],
        modifiers: vec![
            gear(
                "iron_sword",
                AttributeScores::new(2, 0, 0, 0, 0, 0),
                DerivedStats {
                    hitroll: 3,
                    damroll: 5,
                    wield_weight: 2,
                    ..DerivedStats::ZERO
                },
                &[],
            ),
            gear(
                "ember_cloak",
                AttributeScores::ZERO,
                DerivedStats {
                    armor_class: -5,
                    ..DerivedStats::ZERO
                },
                &[("fire", 12), ("cold", -3)],
            ),
            gear(
                "pack_harness",
                AttributeScores::ZERO,
                DerivedStats {
                    carry_weight: 40,
                    ..DerivedStats::ZERO
                },
                &[],
            ),
            affect(
                "bless",
                AttributeScores::new(0, 0, 2, 0, 0, 1),
                DerivedStats {
                    hitroll: 2,
                    damroll: 2,
                    ..DerivedStats::ZERO
                },
                &[("holy", 10), ("shadow", -5)],
            ),
            affect(
                "haste",
                AttributeScores::new(0, 0, 0, 3, 0, 0),
                DerivedStats {
                    movement: 25,
                    armor_class: -8,
                    ..DerivedStats::ZERO
                },
                &[("lightning", 4)],
            ),
        ],
    }
}

pub(crate) fn gear(
    id: &str,
    attributes: AttributeScores,
    derived: DerivedStats,
    resistances: &[(&str, i32)],
) -> StatModifier {
    modifier(
        id,
        StatModifierSource::Gear,
        attributes,
        derived,
        resistances,
    )
}

pub(crate) fn affect(
    id: &str,
    attributes: AttributeScores,
    derived: DerivedStats,
    resistances: &[(&str, i32)],
) -> StatModifier {
    modifier(
        id,
        StatModifierSource::Affect,
        attributes,
        derived,
        resistances,
    )
}

fn modifier(
    id: &str,
    source: StatModifierSource,
    attributes: AttributeScores,
    derived: DerivedStats,
    resistances: &[(&str, i32)],
) -> StatModifier {
    StatModifier {
        modifier_id: id.to_string(),
        source,
        attributes,
        derived,
        resistances: resistances
            .iter()
            .map(|(resistance_id, amount)| ResistanceModifier {
                resistance_id: (*resistance_id).to_string(),
                amount: *amount,
            })
            .collect(),
    }
}

fn resistance(values: &[ResistanceReadback], id: &str) -> i32 {
    values
        .iter()
        .find(|value| value.resistance_id == id)
        .unwrap()
        .value
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
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
    crate::test_temp::TestTempDir::new(&format!("issue960-{label}"))
}
