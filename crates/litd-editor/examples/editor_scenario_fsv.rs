use litd_engine::fsv_artifacts::env_path;
use std::{fs, path::PathBuf};

use litd_editor::{
    history::{stable_fingerprint, to_stable_ron},
    scenario::{
        DayNightCycle, MapDescription, MapOptions, MapRuleset, ScenarioDialogDraft,
        ScenarioWorldInfo, StartLocationPolicy, build_lobby_summary, build_runtime_init,
        load_scenario_world_info, save_scenario_world_info,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env_path(
        "LITD_EDITOR_SCENARIO_FSV_STATS",
        "tmp/issue1276-map-options/stats.txt",
    );
    let world_info_path = env_path(
        "LITD_EDITOR_SCENARIO_FSV_WORLD_INFO",
        "tmp/issue1276-map-options/world_info.ron",
    );
    let runtime_init_path = env_path(
        "LITD_EDITOR_SCENARIO_FSV_RUNTIME_INIT",
        "tmp/issue1276-map-options/runtime_init.ron",
    );
    let lobby_summary_path = env_path(
        "LITD_EDITOR_SCENARIO_FSV_LOBBY",
        "tmp/issue1276-map-options/lobby_summary.ron",
    );

    let dialog = ScenarioDialogDraft {
        metadata: MapDescription {
            name: "Ashen Reach Siege".to_string(),
            suggested_players_min: 2,
            suggested_players_max: 6,
            author: "Light in the Dark".to_string(),
            description: "A known FSV map description used to prove scenario options persist."
                .to_string(),
            recommended_players: "2v2 or 3v3 roleplay teams".to_string(),
        },
        options: MapOptions {
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
        },
    };
    let world_info = dialog.apply()?;
    let before_fingerprint = world_info.stable_fingerprint()?;
    save_scenario_world_info(&world_info_path, &world_info)?;
    let readback_world_info = load_scenario_world_info(&world_info_path)?;
    let readback_fingerprint = readback_world_info.stable_fingerprint()?;
    let runtime_init = build_runtime_init(&readback_world_info)?;
    let lobby_summary = build_lobby_summary(&readback_world_info)?;
    write_text(&runtime_init_path, &to_stable_ron(&runtime_init)?)?;
    write_text(&lobby_summary_path, &to_stable_ron(&lobby_summary)?)?;

    let runtime_readback_text = fs::read_to_string(&runtime_init_path)?;
    let lobby_readback_text = fs::read_to_string(&lobby_summary_path)?;
    let runtime_readback_hash = stable_fingerprint(&runtime_init)?;
    let lobby_readback_hash = stable_fingerprint(&lobby_summary)?;

    let base_valid = readback_world_info.clone();
    let edge_melee_error = edge_melee_with_fixed_forces(&base_valid);
    let edge_fixed_settings_error = edge_fixed_settings_without_forces(&base_valid);
    let edge_long_recommended_error = edge_long_recommended_players(&base_valid);

    write_text(
        &stats_path,
        &format!(
            "issue=1276\n\
             world_info_file={}\n\
             runtime_init_file={}\n\
             lobby_summary_file={}\n\
             world_info_before_fingerprint={}\n\
             world_info_readback_fingerprint={}\n\
             world_info_readback_matches={}\n\
             map_name={}\n\
             author={}\n\
             description={}\n\
             suggested_players={}\n\
             recommended_players={}\n\
             option_ruleset={:?}\n\
             option_start_locations={:?}\n\
             option_allow_observers={}\n\
             option_hide_minimap_preview={}\n\
             option_mask_partial_vision={}\n\
             option_fixed_player_settings={}\n\
             option_fixed_forces={}\n\
             option_water_waves_cliff_shores={}\n\
             option_water_waves_rolling_shores={}\n\
             option_terrain_fog_enabled={}\n\
             option_day_night_cycle={:?}\n\
             init_randomized_start_locations={}\n\
             init_observers_enabled={}\n\
             init_minimap_preview_visible={}\n\
             init_partial_vision_masked={}\n\
             init_player_settings_locked={}\n\
             init_forces_locked={}\n\
             init_water_wave_layers={}\n\
             init_terrain_fog_enabled={}\n\
             init_day_night_cycle_enabled={}\n\
             lobby_name={}\n\
             lobby_recommended_players={}\n\
             lobby_suggested_players={}\n\
             lobby_minimap_preview_visible={}\n\
             runtime_readback_contains_hidden_preview={}\n\
             lobby_readback_contains_metadata={}\n\
             runtime_readback_fingerprint={}\n\
             lobby_readback_fingerprint={}\n\
             edge_melee_fixed_forces_error={}\n\
             edge_fixed_settings_without_forces_error={}\n\
             edge_long_recommended_error={}\n",
            world_info_path.display(),
            runtime_init_path.display(),
            lobby_summary_path.display(),
            before_fingerprint,
            readback_fingerprint,
            before_fingerprint == readback_fingerprint,
            readback_world_info.metadata.name,
            readback_world_info.metadata.author,
            readback_world_info.metadata.description,
            readback_world_info.metadata.suggested_players_label(),
            readback_world_info.metadata.recommended_players,
            readback_world_info.options.ruleset,
            readback_world_info.options.start_locations,
            readback_world_info.options.allow_observers,
            readback_world_info.options.hide_minimap_preview,
            readback_world_info.options.mask_partial_vision,
            readback_world_info.options.fixed_player_settings,
            readback_world_info.options.fixed_forces,
            readback_world_info.options.water_waves_cliff_shores,
            readback_world_info.options.water_waves_rolling_shores,
            readback_world_info.options.terrain_fog_enabled,
            readback_world_info.options.day_night_cycle,
            runtime_init.randomized_start_locations,
            runtime_init.observers_enabled,
            runtime_init.minimap_preview_visible,
            runtime_init.partial_vision_masked,
            runtime_init.player_settings_locked,
            runtime_init.forces_locked,
            runtime_init.water_wave_layers.join("|"),
            runtime_init.terrain_fog_enabled,
            runtime_init.day_night_cycle_enabled,
            lobby_summary.name,
            lobby_summary.recommended_players,
            lobby_summary.suggested_players,
            lobby_summary.minimap_preview_visible,
            runtime_readback_text.contains("minimap_preview_visible: false"),
            lobby_readback_text.contains("Ashen Reach Siege"),
            runtime_readback_hash,
            lobby_readback_hash,
            edge_melee_error,
            edge_fixed_settings_error,
            edge_long_recommended_error,
        ),
    )?;
    println!("wrote scenario FSV stats to {}", stats_path.display());
    Ok(())
}

fn edge_melee_with_fixed_forces(base: &ScenarioWorldInfo) -> String {
    let mut candidate = base.clone();
    candidate.options.ruleset = MapRuleset::Melee;
    candidate.options.start_locations = StartLocationPolicy::Random;
    candidate.validate().unwrap_err().code().to_string()
}

fn edge_fixed_settings_without_forces(base: &ScenarioWorldInfo) -> String {
    let mut candidate = base.clone();
    candidate.options.fixed_forces = false;
    candidate.validate().unwrap_err().code().to_string()
}

fn edge_long_recommended_players(base: &ScenarioWorldInfo) -> String {
    let mut candidate = base.clone();
    candidate.metadata.recommended_players = "x".repeat(50);
    candidate.validate().unwrap_err().code().to_string()
}

fn write_text(path: &PathBuf, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}
