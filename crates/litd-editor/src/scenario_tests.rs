use super::*;

#[test]
fn world_info_persists_and_projects_to_init_and_lobby() {
    let world_info = fixture_world_info();
    let init = build_runtime_init(&world_info).unwrap();
    let lobby = build_lobby_summary(&world_info).unwrap();

    assert_eq!(world_info.schema_version, 1);
    assert_eq!(world_info.source_issue, 1276);
    assert_eq!(world_info.metadata.name, "Ashen Reach FSV");
    assert!(!init.minimap_preview_visible);
    assert!(init.partial_vision_masked);
    assert!(init.player_settings_locked);
    assert!(init.forces_locked);
    assert_eq!(
        init.water_wave_layers,
        vec!["cliff_shores".to_string(), "rolling_shores".to_string()]
    );
    assert!(!init.day_night_cycle_enabled);
    assert_eq!(lobby.name, "Ashen Reach FSV");
    assert_eq!(lobby.suggested_players, "2-6");
    assert_eq!(
        lobby.lobby_metadata.get("recommended_players"),
        Some(&"2v2 or 3v3 roleplay teams".to_string())
    );
    assert_eq!(init.world_info_fingerprint, lobby.world_info_fingerprint);
}

#[test]
fn map_description_dialog_draft_applies_to_world_info() {
    let draft = ScenarioDialogDraft {
        metadata: fixture_metadata(),
        options: fixture_options(),
    };
    let applied = draft.apply().unwrap();

    assert_eq!(applied.metadata.author, "Light in the Dark");
    assert_eq!(applied.options.ruleset, MapRuleset::Custom);
}

#[test]
fn melee_with_fixed_custom_forces_fails_loud() {
    let mut options = fixture_options();
    options.ruleset = MapRuleset::Melee;
    options.start_locations = StartLocationPolicy::Random;
    let error = ScenarioWorldInfo::new(fixture_metadata(), options).unwrap_err();

    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
}

#[test]
fn fixed_player_settings_require_fixed_forces() {
    let mut options = fixture_options();
    options.fixed_forces = false;
    let error = ScenarioWorldInfo::new(fixture_metadata(), options).unwrap_err();

    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
}

#[test]
fn invalid_metadata_range_and_lengths_fail_loud() {
    let mut metadata = fixture_metadata();
    metadata.suggested_players_min = 8;
    metadata.suggested_players_max = 2;
    let error = ScenarioWorldInfo::new(metadata, fixture_options()).unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");

    let mut metadata = fixture_metadata();
    metadata.recommended_players = "x".repeat(50);
    let error = ScenarioWorldInfo::new(metadata, fixture_options()).unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
}

fn fixture_world_info() -> ScenarioWorldInfo {
    ScenarioWorldInfo::new(fixture_metadata(), fixture_options()).unwrap()
}

fn fixture_metadata() -> MapDescription {
    MapDescription {
        name: "Ashen Reach FSV".to_string(),
        suggested_players_min: 2,
        suggested_players_max: 6,
        author: "Light in the Dark".to_string(),
        description: "A deterministic scenario metadata record for runtime and lobby readback."
            .to_string(),
        recommended_players: "2v2 or 3v3 roleplay teams".to_string(),
    }
}

fn fixture_options() -> MapOptions {
    MapOptions {
        ruleset: MapRuleset::Custom,
        start_locations: StartLocationPolicy::Fixed,
        allow_observers: true,
        hide_minimap_preview: true,
        mask_partial_vision: true,
        fixed_player_settings: true,
        fixed_forces: true,
        water_waves_cliff_shores: true,
        water_waves_rolling_shores: true,
        terrain_fog_enabled: false,
        day_night_cycle: DayNightCycle::Disabled,
    }
}
