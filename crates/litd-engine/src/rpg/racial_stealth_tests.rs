use crate::{
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        MovementNoiseClass, RPG_RACIAL_STEALTH_SCHEMA, StealthTerrainScope, StealthTraitRole,
        build_racial_stealth_readback, persist_and_lower_rpg_authored_content,
        racial_stealth_content_records, racial_stealth_profile_table,
        validate_racial_stealth_table,
    },
};

#[test]
fn issue1603_table_covers_all_twenty_authored_races() {
    let readback = build_racial_stealth_readback().unwrap();
    assert_eq!(readback.schema, RPG_RACIAL_STEALTH_SCHEMA);
    assert_eq!(readback.profile_count, 20);
    assert_eq!(readback.race_ids.len(), 20);
    // Noise classes: 2 silent (elf/dark-elf), 3 quiet (small: gnome/svirf/goblin),
    // 9 normal (other medium), 2 heavy (large), 4 thunderous (giant) -> 20 total,
    // 6 creep-penalized (large+giant).
    assert_eq!(readback.noise_class_counts["silent"], 2);
    assert_eq!(readback.noise_class_counts["quiet"], 3);
    assert_eq!(readback.noise_class_counts["normal"], 9);
    assert_eq!(readback.noise_class_counts["heavy"], 2);
    assert_eq!(readback.noise_class_counts["thunderous"], 4);
    assert_eq!(readback.creep_penalized_races, 6);
    assert_eq!(readback.table_sha256.len(), 64);
}

#[test]
fn issue1603_innate_traits_are_cf_factual() {
    let table = racial_stealth_profile_table();
    let elf = profile(&table, "race.elf");
    assert_eq!(elf.movement_noise, MovementNoiseClass::Silent);
    assert_eq!(elf.footstep_radius_scale_percent, 0);
    assert_eq!(elf.innate_traits.len(), 1);
    assert_eq!(elf.innate_traits[0].ability_id, "ability.sneak");
    assert_eq!(elf.innate_traits[0].scope, StealthTerrainScope::Always);

    let dark_elf = profile(&table, "race.ashkin_deep");
    assert_eq!(dark_elf.innate_traits.len(), 2);
    assert!(
        dark_elf
            .innate_traits
            .iter()
            .any(|t| t.ability_id == "ability.faerie_fire" && t.role == StealthTraitRole::Reveal)
    );

    let svirf = profile(&table, "race.deep_kin");
    assert_eq!(
        svirf.innate_traits[0].ability_id,
        "ability.detect_invisibility"
    );
    assert_eq!(svirf.innate_traits[0].role, StealthTraitRole::Detect);

    let duergar = profile(&table, "race.onyx_kin");
    assert_eq!(
        duergar.innate_traits[0].scope,
        StealthTerrainScope::UndergroundAndCity
    );

    // Felar cannot innately sneak: zero innate traits, non-silent noise.
    let felar = profile(&table, "race.felisar");
    assert!(felar.innate_traits.is_empty());
    assert_ne!(felar.movement_noise, MovementNoiseClass::Silent);
}

#[test]
fn issue1603_validation_rejects_dangling_ability_id() {
    let mut table = racial_stealth_profile_table();
    let idx = table
        .profiles
        .iter()
        .position(|p| p.race_id == "race.elf")
        .unwrap();
    table.profiles[idx].innate_traits[0].ability_id = "ability.does_not_exist".to_string();
    let error = validate_racial_stealth_table(&table).unwrap_err();
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    assert!(error.to_string().contains("ability.does_not_exist"));
}

#[test]
fn issue1603_validation_rejects_dangling_race_id() {
    let mut table = racial_stealth_profile_table();
    table.profiles[0].race_id = "race.not_a_real_race".to_string();
    let error = validate_racial_stealth_table(&table).unwrap_err();
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    assert!(error.to_string().contains("race.not_a_real_race"));
}

#[test]
fn issue1603_persists_and_reads_back_from_calyx() {
    let table = racial_stealth_profile_table();
    let records = racial_stealth_content_records(&table).unwrap();
    // 20 profiles + 1 summary.
    assert_eq!(records.len(), 21);
    let (store, _vault) = content_store("racial-stealth");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    assert_eq!(lowered.row_count, 21);
    assert!(records.iter().all(|r| {
        r.source
            .path
            .starts_with("editor://rpg/ashen_reach/racial_stealth/")
    }));

    let summary = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            "table.ashen_reach.racial_stealth.summary",
        )
        .unwrap();
    assert_eq!(summary.payload["surface_kind"], "racial_stealth_table");
    assert_eq!(summary.payload["profile_count"], 20);

    let elf = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            "table.ashen_reach.racial_stealth.race.elf",
        )
        .unwrap();
    assert_eq!(elf.payload["surface_kind"], "racial_stealth_profile");
    assert_eq!(elf.payload["movement_noise"], "silent");
    assert_eq!(elf.payload["footstep_radius_scale_percent"], 0);
}

fn profile<'a>(
    table: &'a crate::rpg::RacialStealthProfileTable,
    race_id: &str,
) -> &'a crate::rpg::RacialStealthProfile {
    table
        .profiles
        .iter()
        .find(|p| p.race_id == race_id)
        .unwrap()
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1603-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
