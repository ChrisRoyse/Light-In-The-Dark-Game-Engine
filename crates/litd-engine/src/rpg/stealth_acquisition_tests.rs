use crate::{
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AcquisitionKind, GrantorKind, RPG_STEALTH_ACQUISITION_SCHEMA, StealthAcquisitionEntry,
        build_catalog_matrix_consistency_readback, build_stealth_acquisition_readback,
        persist_and_lower_rpg_authored_content, stealth_ability_catalog,
        stealth_acquisition_content_records, stealth_acquisition_matrix,
        validate_catalog_matrix_level_consistency, validate_stealth_acquisition_matrix,
    },
};

#[test]
fn issue1623_matrix_counts_and_referential_integrity() {
    let readback = build_stealth_acquisition_readback().unwrap();
    assert_eq!(readback.schema, RPG_STEALTH_ACQUISITION_SCHEMA);
    // 41 original rows + 6 #1732 catalog-gap rows (gentle walk, heightened
    // awareness, dark vision x2, mass invis, vanish).
    assert_eq!(readback.entry_count, 47);
    // Every ability id resolved and every non-provisional grantor resolved.
    assert_eq!(readback.abilities_validated, 47);
    assert_eq!(readback.grantors_validated, 47);
    assert_eq!(readback.provisional_entry_count, 0);
    assert!(readback.provisional_grantors.is_empty());
    assert_eq!(
        readback.class_entry_count + readback.race_entry_count,
        readback.entry_count
    );
    // Innate rows: elf/dark-elf sneak, half-elf/half-drow quiet, duergar detect
    // hidden, svirfneblin detect invis, dark-elf faerie fire = 7.
    assert_eq!(readback.acquisition_counts["innate"], 7);
    assert_eq!(readback.race_entry_count, 7);
    assert_eq!(readback.matrix_sha256.len(), 64);
}

#[test]
fn issue1623_known_levels_are_cf_factual() {
    let matrix = stealth_acquisition_matrix();
    assert_eq!(level(&matrix, "ability.sneak", "class.galdmere_shade"), 4);
    assert_eq!(level(&matrix, "ability.sneak", "class.wandersong"), 30);
    assert_eq!(level(&matrix, "ability.hide", "class.galdmere_shade"), 1);
    assert_eq!(level(&matrix, "ability.assassinate", "class.hushblade"), 35);
    assert_eq!(level(&matrix, "ability.track", "class.thornwarden"), 30);
    // Innate rows are level 1.
    assert_eq!(level(&matrix, "ability.sneak", "race.elf"), 1);
    assert_eq!(
        level(&matrix, "ability.detect_invisibility", "race.deep_kin"),
        1
    );

    // Invoker improved-invis is path-gated on the air path.
    let imp = cell(
        &matrix,
        "ability.improved_invisibility",
        "class.currentbraider",
    );
    assert_eq!(imp.acquisition, AcquisitionKind::PathOnly);
    assert_eq!(imp.path_id.as_deref(), Some("path.air"));

    // Thief improved-hide carries the #1623 point cost.
    let imp_hide = cell(&matrix, "ability.improved_hide", "class.galdmere_shade");
    assert_eq!(imp_hide.point_cost, Some(4));

    // #1732 catalog-gap abilities, CF-factual levels.
    assert_eq!(
        level(&matrix, "ability.heightened_awareness", "class.hushblade"),
        7
    );
    assert_eq!(level(&matrix, "ability.dark_vision", "class.hushblade"), 5);
    assert_eq!(
        level(&matrix, "ability.dark_vision", "class.thornwarden"),
        7
    );
    assert_eq!(level(&matrix, "ability.mass_invis", "class.sinewright"), 11);
    assert_eq!(level(&matrix, "ability.vanish", "class.hushblade"), 16);
    // Gentle walk is a thief path skill with a #1623 point cost.
    let gentle = cell(&matrix, "ability.gentle_walk", "class.galdmere_shade");
    assert_eq!(gentle.level, 13);
    assert_eq!(gentle.point_cost, Some(3));
}

#[test]
fn issue1732_catalog_and_matrix_levels_are_consistent() {
    // Single-source cross-check: every catalog ability agrees with the matrix on
    // the level of its primary gating class. The canonical authored pair passes.
    let readback = build_catalog_matrix_consistency_readback().unwrap();
    assert_eq!(readback.catalog_ability_count, 33);
    assert_eq!(readback.matrix_entry_count, 47);
    // 31 of the 33 catalog abilities have a primary-gate matrix cell (observe and
    // smear mud are catalog-only ranger utilities with no acquisition row).
    assert_eq!(readback.cross_checked_count, 31);
    assert_eq!(readback.consistency_sha256.len(), 64);
}

#[test]
fn issue1732_level_drift_fails_loud_with_ability_class_and_both_levels() {
    // Inject the exact drift #1732 fixed (catalog hide at L4 vs matrix L1) and
    // prove the guard names the ability, the class, and both levels.
    let mut catalog = stealth_ability_catalog();
    let hide = catalog
        .abilities
        .iter_mut()
        .find(|a| a.ability_id == "ability.hide")
        .unwrap();
    hide.requirements.min_level = 4;
    let matrix = stealth_acquisition_matrix();
    let error = validate_catalog_matrix_level_consistency(&catalog, &matrix).unwrap_err();
    assert_eq!(error.code(), "LITD-RPG-ABILITY-INVALID-CONFIG");
    let message = error.to_string();
    assert!(message.contains("ability.hide"));
    assert!(message.contains("class.galdmere_shade"));
    assert!(message.contains("min_level=4"));
    assert!(message.contains("level=1"));
}

#[test]
fn issue1623_validation_fails_loud_on_dangling_ability_id() {
    let mut matrix = stealth_acquisition_matrix();
    matrix.entries[0].ability_id = "ability.phantom".to_string();
    let error = validate_stealth_acquisition_matrix(&matrix).unwrap_err();
    assert_eq!(error.code(), "LITD-RPG-ABILITY-INVALID-CONFIG");
    assert!(error.to_string().contains("ability.phantom"));
    assert!(error.to_string().contains("#1604"));
}

#[test]
fn issue1623_validation_fails_loud_on_dangling_grantor_id() {
    let mut matrix = stealth_acquisition_matrix();
    matrix.entries[0].grantor_id = "class.ghost_class".to_string();
    let error = validate_stealth_acquisition_matrix(&matrix).unwrap_err();
    assert_eq!(error.code(), "LITD-RPG-ABILITY-INVALID-CONFIG");
    assert!(error.to_string().contains("class.ghost_class"));
}

#[test]
fn issue1623_provisional_grantor_skips_roster_check() {
    let mut matrix = stealth_acquisition_matrix();
    // A grantor that does not resolve is only allowed when flagged provisional.
    matrix.entries.push(StealthAcquisitionEntry {
        ability_id: "ability.sneak".to_string(),
        grantor_kind: GrantorKind::Class,
        grantor_id: "class.unwritten_spec".to_string(),
        level: 20,
        acquisition: AcquisitionKind::Trained,
        path_id: None,
        point_cost: None,
        terrain_profile: None,
        provisional: true,
        source_note: "provisional row for a not-yet-authored class".to_string(),
    });
    let readback = validate_stealth_acquisition_matrix(&matrix).unwrap();
    assert_eq!(readback.provisional_entry_count, 1);
    assert_eq!(
        readback.provisional_grantors,
        vec!["class.unwritten_spec".to_string()]
    );
}

#[test]
fn issue1623_persists_and_reads_back_from_calyx() {
    let matrix = stealth_acquisition_matrix();
    let records = stealth_acquisition_content_records(&matrix).unwrap();
    assert_eq!(records.len(), matrix.entries.len() + 1);
    let (store, _vault) = content_store("acquisition");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    assert_eq!(lowered.row_count, records.len());
    assert!(records.iter().all(|r| {
        r.source
            .path
            .starts_with("editor://rpg/ashen_reach/stealth_acquisition/")
    }));

    let summary = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            "table.ashen_reach.stealth_acquisition.summary",
        )
        .unwrap();
    assert_eq!(summary.payload["surface_kind"], "stealth_acquisition_table");
    assert_eq!(summary.payload["entry_count"], 47);

    let row = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            "table.ashen_reach.stealth_acquisition.ability.sneak.race.elf",
        )
        .unwrap();
    assert_eq!(row.payload["surface_kind"], "stealth_acquisition_entry");
    assert_eq!(row.payload["acquisition"], "innate");
    assert_eq!(row.payload["level"], 1);
}

fn cell<'a>(
    matrix: &'a crate::rpg::StealthAcquisitionMatrix,
    ability_id: &str,
    grantor_id: &str,
) -> &'a StealthAcquisitionEntry {
    matrix
        .entries
        .iter()
        .find(|e| e.ability_id == ability_id && e.grantor_id == grantor_id)
        .unwrap()
}

fn level(matrix: &crate::rpg::StealthAcquisitionMatrix, ability_id: &str, grantor_id: &str) -> u16 {
    cell(matrix, ability_id, grantor_id).level
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1623-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
