//! Scenario map description and map-options world-info records.

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};

use crate::history::{stable_fingerprint, to_stable_ron};

const SCHEMA_VERSION: u32 = 1;
const SOURCE_ISSUE: u32 = 1276;
const MAX_PLAYERS: u8 = 24;
const MAX_MAP_NAME: usize = 109;
const MAX_AUTHOR: usize = 97;
const MAX_DESCRIPTION: usize = 769;
const MAX_RECOMMENDED_PLAYERS: usize = 49;
const SCENARIO_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCA";
const SCENARIO_VAULT_PURPOSE: &[u8] = b"litd-editor-scenario-issue1363";
const SCENARIO_COLLECTION: &str = "litd_editor_scenario";
const SCENARIO_WORLD_INFO_ROW: &[u8] = b"scenario/world_info/latest";
const SCENARIO_WORLD_INFO_COLUMN: &[u8] = b"world_info_ron";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioWorldInfo {
    pub schema_version: u32,
    pub source_issue: u32,
    pub metadata: MapDescription,
    pub options: MapOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapDescription {
    pub name: String,
    pub suggested_players_min: u8,
    pub suggested_players_max: u8,
    pub author: String,
    pub description: String,
    pub recommended_players: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapRuleset {
    Melee,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartLocationPolicy {
    Fixed,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DayNightCycle {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapOptions {
    pub ruleset: MapRuleset,
    pub start_locations: StartLocationPolicy,
    pub allow_observers: bool,
    pub hide_minimap_preview: bool,
    pub mask_partial_vision: bool,
    pub fixed_player_settings: bool,
    pub fixed_forces: bool,
    pub water_waves_cliff_shores: bool,
    pub water_waves_rolling_shores: bool,
    pub terrain_fog_enabled: bool,
    pub day_night_cycle: DayNightCycle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioDialogDraft {
    pub metadata: MapDescription,
    pub options: MapOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeScenarioInit {
    pub ruleset: MapRuleset,
    pub randomized_start_locations: bool,
    pub observers_enabled: bool,
    pub minimap_preview_visible: bool,
    pub partial_vision_masked: bool,
    pub player_settings_locked: bool,
    pub forces_locked: bool,
    pub water_wave_layers: Vec<String>,
    pub terrain_fog_enabled: bool,
    pub day_night_cycle_enabled: bool,
    pub world_info_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LobbyMapSummary {
    pub name: String,
    pub author: String,
    pub description: String,
    pub recommended_players: String,
    pub suggested_players: String,
    pub ruleset: MapRuleset,
    pub observers_allowed: bool,
    pub minimap_preview_visible: bool,
    pub lobby_metadata: BTreeMap<String, String>,
    pub world_info_fingerprint: String,
}

impl ScenarioWorldInfo {
    pub fn new(metadata: MapDescription, options: MapOptions) -> Result<Self, EngineError> {
        let world_info = Self {
            schema_version: SCHEMA_VERSION,
            source_issue: SOURCE_ISSUE,
            metadata,
            options,
        };
        world_info.validate()?;
        Ok(world_info)
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema_version != SCHEMA_VERSION || self.source_issue != SOURCE_ISSUE {
            return invalid_scenario("schema", "unexpected scenario world-info identity");
        }
        self.metadata.validate()?;
        self.options.validate()?;
        Ok(())
    }

    pub fn stable_fingerprint(&self) -> Result<String, EngineError> {
        self.validate()?;
        stable_fingerprint(self)
    }
}

impl MapDescription {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_required_string("metadata.name", &self.name, MAX_MAP_NAME)?;
        validate_optional_string("metadata.author", &self.author, MAX_AUTHOR)?;
        validate_required_string("metadata.description", &self.description, MAX_DESCRIPTION)?;
        validate_required_string(
            "metadata.recommended_players",
            &self.recommended_players,
            MAX_RECOMMENDED_PLAYERS,
        )?;
        if self.suggested_players_min == 0
            || self.suggested_players_max == 0
            || self.suggested_players_min > self.suggested_players_max
            || self.suggested_players_max > MAX_PLAYERS
        {
            return invalid_scenario(
                "metadata.suggested_players",
                "suggested player range must be 1..=24 and min <= max",
            );
        }
        Ok(())
    }

    pub fn suggested_players_label(&self) -> String {
        if self.suggested_players_min == self.suggested_players_max {
            self.suggested_players_min.to_string()
        } else {
            format!(
                "{}-{}",
                self.suggested_players_min, self.suggested_players_max
            )
        }
    }
}

impl MapOptions {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.ruleset == MapRuleset::Melee {
            if self.start_locations != StartLocationPolicy::Random {
                return invalid_scenario(
                    "options.start_locations",
                    "melee maps require random start locations",
                );
            }
            if self.fixed_forces || self.fixed_player_settings {
                return invalid_scenario(
                    "options.fixed_forces",
                    "melee maps cannot lock custom forces or player settings",
                );
            }
        }
        if self.fixed_player_settings && !self.fixed_forces {
            return invalid_scenario(
                "options.fixed_player_settings",
                "fixed player settings require fixed custom forces",
            );
        }
        Ok(())
    }
}

impl ScenarioDialogDraft {
    pub fn apply(self) -> Result<ScenarioWorldInfo, EngineError> {
        ScenarioWorldInfo::new(self.metadata, self.options)
    }
}

pub fn save_scenario_world_info(
    path: &Path,
    world_info: &ScenarioWorldInfo,
) -> Result<(), EngineError> {
    world_info.validate()?;
    let bytes = to_stable_ron(world_info)?.into_bytes();
    let stored = write_calyx_json_bytes(&scenario_world_info_spec(path), &bytes)
        .map_err(|error| io_scenario_error("save", error))?;
    if stored.bytes != bytes {
        return invalid_scenario(
            "save.readback",
            "Aster scenario world-info bytes did not match serialized RON",
        );
    }
    Ok(())
}

pub fn load_scenario_world_info(path: &Path) -> Result<ScenarioWorldInfo, EngineError> {
    let readback = read_calyx_json_bytes(&scenario_world_info_spec(path))
        .map_err(|error| io_scenario_error("load", error))?
        .ok_or_else(|| {
            io_scenario_error(
                "load",
                format!(
                    "missing Calyx Aster scenario world-info row at {}",
                    path.display()
                ),
            )
        })?;
    let text =
        String::from_utf8(readback.bytes).map_err(|error| EngineError::SerializationFailed {
            format: "ron",
            reason: format!("scenario world-info Aster bytes are not UTF-8: {error}"),
        })?;
    let world_info: ScenarioWorldInfo =
        ron::from_str(&text).map_err(|error| EngineError::SerializationFailed {
            format: "ron",
            reason: error.to_string(),
        })?;
    world_info.validate()?;
    Ok(world_info)
}

pub fn build_runtime_init(
    world_info: &ScenarioWorldInfo,
) -> Result<RuntimeScenarioInit, EngineError> {
    world_info.validate()?;
    let options = &world_info.options;
    let mut water_wave_layers = Vec::new();
    if options.water_waves_cliff_shores {
        water_wave_layers.push("cliff_shores".to_string());
    }
    if options.water_waves_rolling_shores {
        water_wave_layers.push("rolling_shores".to_string());
    }
    Ok(RuntimeScenarioInit {
        ruleset: options.ruleset,
        randomized_start_locations: options.start_locations == StartLocationPolicy::Random,
        observers_enabled: options.allow_observers,
        minimap_preview_visible: !options.hide_minimap_preview,
        partial_vision_masked: options.mask_partial_vision,
        player_settings_locked: options.fixed_player_settings,
        forces_locked: options.fixed_forces,
        water_wave_layers,
        terrain_fog_enabled: options.terrain_fog_enabled,
        day_night_cycle_enabled: options.day_night_cycle == DayNightCycle::Enabled,
        world_info_fingerprint: world_info.stable_fingerprint()?,
    })
}

pub fn build_lobby_summary(world_info: &ScenarioWorldInfo) -> Result<LobbyMapSummary, EngineError> {
    world_info.validate()?;
    let metadata = &world_info.metadata;
    let options = &world_info.options;
    let mut lobby_metadata = BTreeMap::new();
    lobby_metadata.insert("map_name".to_string(), metadata.name.clone());
    lobby_metadata.insert("author".to_string(), metadata.author.clone());
    lobby_metadata.insert(
        "recommended_players".to_string(),
        metadata.recommended_players.clone(),
    );
    lobby_metadata.insert(
        "suggested_players".to_string(),
        metadata.suggested_players_label(),
    );
    lobby_metadata.insert("ruleset".to_string(), format!("{:?}", options.ruleset));
    lobby_metadata.insert(
        "minimap_preview_visible".to_string(),
        (!options.hide_minimap_preview).to_string(),
    );
    Ok(LobbyMapSummary {
        name: metadata.name.clone(),
        author: metadata.author.clone(),
        description: metadata.description.clone(),
        recommended_players: metadata.recommended_players.clone(),
        suggested_players: metadata.suggested_players_label(),
        ruleset: options.ruleset,
        observers_allowed: options.allow_observers,
        minimap_preview_visible: !options.hide_minimap_preview,
        lobby_metadata,
        world_info_fingerprint: world_info.stable_fingerprint()?,
    })
}

fn validate_required_string(field: &str, value: &str, max_chars: usize) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_scenario(field, "must be non-empty");
    }
    validate_optional_string(field, value, max_chars)
}

fn validate_optional_string(field: &str, value: &str, max_chars: usize) -> Result<(), EngineError> {
    if value.chars().count() > max_chars {
        return invalid_scenario(field, format!("must be at most {max_chars} characters"));
    }
    Ok(())
}

fn io_scenario_error(field: &str, error: impl std::fmt::Display) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.scenario.{field}"),
        reason: error.to_string(),
    }
}

fn scenario_world_info_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: SCENARIO_VAULT_ID,
        purpose: SCENARIO_VAULT_PURPOSE,
        collection: SCENARIO_COLLECTION,
        row_key: SCENARIO_WORLD_INFO_ROW,
        column: SCENARIO_WORLD_INFO_COLUMN,
    }
}

fn invalid_scenario<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.scenario.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "scenario_tests.rs"]
mod tests;
