use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AttributeScores, HallDeed, HallDeedKind, HallMemorialDossier, HallTribute,
        MortalityCharacterState, MortalityEvent, MortalityEventKind, MortalityPolicy,
        RPG_MORTALITY_SCHEMA, hall_memorial_content_records,
        persist_and_lower_rpg_authored_content, record_hall_memorial,
        validate_hall_of_remembered_policy, validate_rpg_authored_content_records,
    },
    rpg_hall_of_remembered_test_fixture::ashen_reach_hall_of_remembered_policy,
    rpg_mortality_test_fixture::ashen_reach_mortality_policy,
};

#[test]
fn issue1065_permanent_death_creates_public_hall_memorial() {
    let policy = ashen_reach_hall_of_remembered_policy();
    let mortality = ashen_reach_mortality_policy();
    let death = permanent_death(&mortality);
    let readback =
        record_hall_memorial(&policy, &mortality, &death, &dossier(), &tributes()).unwrap();

    assert_eq!(readback.memorial_id, "memorial.hall.char.aren_vael.3");
    assert_eq!(
        readback.mourning_wall_entry_id,
        "mourning.wall.char.aren_vael.3"
    );
    assert_eq!(readback.character_name, "Aren Vael");
    assert_eq!(readback.race_id, "race.ashen_human");
    assert_eq!(readback.class_id, "class.warden");
    assert_eq!(readback.faction_id, "organization.dawnward");
    assert_eq!(readback.lifespan_years, 44);
    assert_eq!(readback.deed_count, 4);
    assert_eq!(readback.title_count, 2);
    assert_eq!(readback.tribute_count, 2);
    assert!(readback.persistent_across_server);
    assert!(readback.surfaced_in_galdmere);
    assert!(readback.browsable_online);
    assert!(!readback.built_in_map_required);
    assert!(!readback.grants_map_or_minimap);
    assert!(!readback.grants_gps_markers);
}

#[test]
fn issue1065_issue1455_persists_memorial_surfaces_as_authored_calyx_content() {
    let policy = ashen_reach_hall_of_remembered_policy();
    let mortality = ashen_reach_mortality_policy();
    let death = permanent_death(&mortality);
    let readback =
        record_hall_memorial(&policy, &mortality, &death, &dossier(), &tributes()).unwrap();
    let records = hall_memorial_content_records(&policy, &readback).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault) = content_store("hall");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 3);
    assert_eq!(lowered.row_count, 3);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/hall_of_remembered/")
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
            .any(|row| row.kind == EngineContentKind::Story)
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.id == "surface.galdmere.hall.char.aren_vael.3")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.id == "surface.online.hall.char.aren_vael.3")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.surface_kind == "hall_memorial_record")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.surface_kind == "galdmere_mourning_wall_browser")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.surface_kind == "online_hall_browser")
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
fn issue1065_rejects_non_permanent_or_mismatched_mortality() {
    let policy = ashen_reach_hall_of_remembered_policy();
    let mortality = ashen_reach_mortality_policy();
    let living_death =
        crate::rpg::record_mortality_event(&living_state(16), &mortality, &combat_event()).unwrap();
    assert_code(
        record_hall_memorial(&policy, &mortality, &living_death, &dossier(), &tributes()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut mismatch = dossier();
    mismatch.faction_id = "organization.ironbound".to_string();
    let death = permanent_death(&mortality);
    let readback =
        record_hall_memorial(&policy, &mortality, &death, &mismatch, &tributes()).unwrap();
    assert_eq!(readback.faction_id, "organization.ironbound");

    mismatch.race_id = "race.ashen_elf".to_string();
    assert_code(
        record_hall_memorial(&policy, &mortality, &death, &mismatch, &tributes()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1065_rejects_map_drift_and_missing_browse_contracts() {
    let mut map = ashen_reach_hall_of_remembered_policy();
    map.built_in_map_required = true;
    assert_code(
        validate_hall_of_remembered_policy(&map),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut gps = ashen_reach_hall_of_remembered_policy();
    gps.grants_gps_markers = true;
    assert_code(
        validate_hall_of_remembered_policy(&gps),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut offline = ashen_reach_hall_of_remembered_policy();
    offline.browsable_online = false;
    assert_code(
        validate_hall_of_remembered_policy(&offline),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1065_rejects_bad_deeds_and_non_opt_in_tributes() {
    let policy = ashen_reach_hall_of_remembered_policy();
    let mortality = ashen_reach_mortality_policy();
    let death = permanent_death(&mortality);
    let mut thin_dossier = dossier();
    thin_dossier.deeds.truncate(2);
    assert_code(
        record_hall_memorial(&policy, &mortality, &death, &thin_dossier, &tributes()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut private = tributes();
    private[0].opted_in_publicly = false;
    assert_code(
        record_hall_memorial(&policy, &mortality, &death, &dossier(), &private),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn permanent_death(mortality: &MortalityPolicy) -> crate::rpg::MortalityEventReadback {
    let mut state = living_state(4);
    state.total_deaths = 2;
    state.deaths_since_constitution_loss = 2;
    crate::rpg::record_mortality_event(
        &state,
        mortality,
        &combat_event().witnessed_by("player.mira_steadfast"),
    )
    .unwrap()
}

fn living_state(current_constitution: i16) -> MortalityCharacterState {
    MortalityCharacterState {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        character_id: "char.aren_vael".to_string(),
        character_name: "Aren Vael".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 43,
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

fn dossier() -> HallMemorialDossier {
    HallMemorialDossier {
        character_id: "char.aren_vael".to_string(),
        character_name: "Aren Vael".to_string(),
        race_id: "race.ashen_human".to_string(),
        class_id: "class.warden".to_string(),
        faction_id: "organization.dawnward".to_string(),
        lifespan_years: 44,
        titles: vec![
            "Gatekeeper of Cinders".to_string(),
            "High Lantern".to_string(),
        ],
        deeds: vec![
            HallDeed::new(
                "deed.boss.cinder_judge",
                HallDeedKind::BossSlain,
                "Broke the Cinder Judge before dawn.",
                "boss.cinder_judge",
                950,
            ),
            HallDeed::new(
                "deed.item.unfaltering_lamp",
                HallDeedKind::LegendaryItemHeld,
                "Carried the Unfaltering Lamp through Hollow rain.",
                "item.dawnward.unfaltering_lamp",
                980,
            ),
            HallDeed::new(
                "deed.raid.ember_gate",
                HallDeedKind::RaidWon,
                "Won the Ember Gate defense with seven living witnesses.",
                "raid.ember_gate_defense",
                1000,
            ),
            HallDeed::new(
                "deed.rank.high_lantern",
                HallDeedKind::RankReached,
                "Earned High Lantern by public service.",
                "rank.dawnward.high_lantern",
                900,
            ),
        ],
        memorable_quotes: vec![
            "Hold the road, not the glory.".to_string(),
            "Let them remember the crossing, not the fear.".to_string(),
        ],
    }
}

fn tributes() -> Vec<HallTribute> {
    vec![
        HallTribute::opted_in(
            "tribute.mira.aren.1",
            "player.mira",
            "char.mira_steadfast",
            "Aren spent the last shield on strangers and never asked their names.",
            "tribute.public.mira.1",
        ),
        HallTribute::opted_in(
            "tribute.orren.aren.1",
            "player.orren",
            "char.orren_vale",
            "The gate held because Aren made fear feel smaller than duty.",
            "tribute.public.orren.1",
        ),
    ]
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
    crate::test_temp::TestTempDir::new(&format!("issue1065-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
