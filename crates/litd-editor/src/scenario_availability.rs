//! Per-player scenario availability gates and placement priorities.

use std::{collections::BTreeSet, path::Path};

use serde::{Deserialize, Serialize};

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};

use crate::history::{stable_fingerprint, to_stable_ron};

const SCHEMA_VERSION: u32 = 1;
const SOURCE_ISSUE: u32 = 1275;
const MAX_PLAYERS: u8 = 24;
const AVAILABILITY_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCB";
const AVAILABILITY_VAULT_PURPOSE: &[u8] = b"litd-editor-scenario-availability-issue1363";
const AVAILABILITY_COLLECTION: &str = "litd_editor_scenario_availability";
const AVAILABILITY_ROW: &[u8] = b"scenario/availability/latest";
const AVAILABILITY_COLUMN: &[u8] = b"availability_ron";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioAvailabilityRecord {
    pub schema_version: u32,
    pub source_issue: u32,
    pub player_count: u8,
    pub player_overrides: Vec<PlayerAvailabilityOverrides>,
    #[serde(default)]
    pub tech_dependencies: Vec<TechDependencyRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerAvailabilityOverrides {
    pub player_id: u8,
    #[serde(default)]
    pub disabled_tech: Vec<String>,
    #[serde(default)]
    pub disabled_abilities: Vec<String>,
    #[serde(default)]
    pub upgrades: Vec<PlayerUpgradeOverride>,
    pub placement_priorities: PlayerPlacementPriorities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerUpgradeOverride {
    pub upgrade_id: String,
    pub availability: UpgradeAvailability,
    pub starting_level: u8,
    pub max_level: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpgradeAvailability {
    Unavailable,
    Available,
    Researched,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerPlacementPriorities {
    #[serde(default)]
    pub ally_low: Vec<u8>,
    #[serde(default)]
    pub ally_high: Vec<u8>,
    #[serde(default)]
    pub enemy_low: Vec<u8>,
    #[serde(default)]
    pub enemy_high: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechDependencyRule {
    pub required_tech_id: String,
    pub dependent_tech_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeScenarioAvailability {
    pub player_count: u8,
    pub player_states: Vec<PlayerRuntimeAvailability>,
    pub blocked_dependents: Vec<BlockedDependentTech>,
    pub clamped_upgrades: Vec<ClampedUpgradeLevel>,
    pub placement_plan: Vec<PlayerPlacementPlan>,
    pub source_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerRuntimeAvailability {
    pub player_id: u8,
    pub unavailable_tech: Vec<String>,
    pub unavailable_abilities: Vec<String>,
    pub upgrades: Vec<RuntimeUpgradeState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeUpgradeState {
    pub upgrade_id: String,
    pub availability: UpgradeAvailability,
    pub starting_level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedDependentTech {
    pub player_id: u8,
    pub required_tech_id: String,
    pub dependent_tech_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClampedUpgradeLevel {
    pub player_id: u8,
    pub upgrade_id: String,
    pub requested_level: u8,
    pub clamped_level: u8,
    pub max_level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerPlacementPlan {
    pub player_id: u8,
    pub near_allies: Vec<u8>,
    pub soft_near_allies: Vec<u8>,
    pub far_enemies: Vec<u8>,
    pub soft_far_enemies: Vec<u8>,
}

impl ScenarioAvailabilityRecord {
    pub fn new(
        player_count: u8,
        player_overrides: Vec<PlayerAvailabilityOverrides>,
        tech_dependencies: Vec<TechDependencyRule>,
    ) -> Result<Self, EngineError> {
        let record = Self {
            schema_version: SCHEMA_VERSION,
            source_issue: SOURCE_ISSUE,
            player_count,
            player_overrides,
            tech_dependencies,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema_version != SCHEMA_VERSION || self.source_issue != SOURCE_ISSUE {
            return invalid_availability("schema", "unexpected availability record identity");
        }
        validate_player_count(self.player_count)?;
        let mut player_ids = BTreeSet::new();
        for player in &self.player_overrides {
            validate_player_id(player.player_id, self.player_count, "player.player_id")?;
            if !player_ids.insert(player.player_id) {
                return invalid_availability(
                    "player.player_id",
                    format!("duplicate player {}", player.player_id),
                );
            }
            player.validate(self.player_count)?;
        }
        for dependency in &self.tech_dependencies {
            validate_rawcode("tech_dependencies.required", &dependency.required_tech_id)?;
            validate_rawcode("tech_dependencies.dependent", &dependency.dependent_tech_id)?;
            if dependency.required_tech_id == dependency.dependent_tech_id {
                return invalid_availability(
                    "tech_dependencies",
                    "required and dependent tech must differ",
                );
            }
        }
        Ok(())
    }

    pub fn stable_fingerprint(&self) -> Result<String, EngineError> {
        self.validate()?;
        stable_fingerprint(self)
    }
}

impl PlayerAvailabilityOverrides {
    pub fn validate(&self, player_count: u8) -> Result<(), EngineError> {
        validate_rawcode_list("player.disabled_tech", &self.disabled_tech)?;
        validate_rawcode_list("player.disabled_abilities", &self.disabled_abilities)?;
        let mut upgrade_ids = BTreeSet::new();
        for upgrade in &self.upgrades {
            upgrade.validate()?;
            if !upgrade_ids.insert(upgrade.upgrade_id.as_str()) {
                return invalid_availability(
                    "player.upgrades",
                    format!("duplicate upgrade {}", upgrade.upgrade_id),
                );
            }
        }
        self.placement_priorities
            .validate(self.player_id, player_count)
    }
}

impl PlayerUpgradeOverride {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_rawcode("player.upgrade.id", &self.upgrade_id)?;
        if self.max_level == 0 {
            return invalid_availability("player.upgrade.max_level", "must be greater than zero");
        }
        Ok(())
    }
}

impl PlayerPlacementPriorities {
    pub fn validate(&self, player_id: u8, player_count: u8) -> Result<(), EngineError> {
        validate_priority_group(player_id, player_count, "ally_low", &self.ally_low)?;
        validate_priority_group(player_id, player_count, "ally_high", &self.ally_high)?;
        validate_priority_group(player_id, player_count, "enemy_low", &self.enemy_low)?;
        validate_priority_group(player_id, player_count, "enemy_high", &self.enemy_high)?;
        reject_overlap("ally", &self.ally_low, &self.ally_high)?;
        reject_overlap("enemy", &self.enemy_low, &self.enemy_high)?;
        Ok(())
    }
}

pub fn save_scenario_availability_record(
    path: &Path,
    record: &ScenarioAvailabilityRecord,
) -> Result<(), EngineError> {
    record.validate()?;
    let bytes = to_stable_ron(record)?.into_bytes();
    let stored = write_calyx_json_bytes(&availability_spec(path), &bytes)
        .map_err(|error| io_availability_error("save", error))?;
    if stored.bytes != bytes {
        return invalid_availability(
            "save.readback",
            "Aster scenario availability bytes did not match serialized RON",
        );
    }
    Ok(())
}

pub fn load_scenario_availability_record(
    path: &Path,
) -> Result<ScenarioAvailabilityRecord, EngineError> {
    let readback = read_calyx_json_bytes(&availability_spec(path))
        .map_err(|error| io_availability_error("load", error))?
        .ok_or_else(|| {
            io_availability_error(
                "load",
                format!(
                    "missing Calyx Aster scenario availability row at {}",
                    path.display()
                ),
            )
        })?;
    let text =
        String::from_utf8(readback.bytes).map_err(|error| EngineError::SerializationFailed {
            format: "ron",
            reason: format!("scenario availability Aster bytes are not UTF-8: {error}"),
        })?;
    let record: ScenarioAvailabilityRecord =
        ron::from_str(&text).map_err(|error| EngineError::SerializationFailed {
            format: "ron",
            reason: error.to_string(),
        })?;
    record.validate()?;
    Ok(record)
}

pub fn build_runtime_availability(
    record: &ScenarioAvailabilityRecord,
) -> Result<RuntimeScenarioAvailability, EngineError> {
    record.validate()?;
    let mut player_states = Vec::new();
    let mut blocked_dependents = Vec::new();
    let mut clamped_upgrades = Vec::new();
    let mut placement_plan = Vec::new();
    for player in &record.player_overrides {
        let mut unavailable_tech = player
            .disabled_tech
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for dependency in &record.tech_dependencies {
            if unavailable_tech.contains(&dependency.required_tech_id)
                && unavailable_tech.insert(dependency.dependent_tech_id.clone())
            {
                blocked_dependents.push(BlockedDependentTech {
                    player_id: player.player_id,
                    required_tech_id: dependency.required_tech_id.clone(),
                    dependent_tech_id: dependency.dependent_tech_id.clone(),
                });
            }
        }
        let mut upgrades = Vec::new();
        for upgrade in &player.upgrades {
            let clamped_level = upgrade.starting_level.min(upgrade.max_level);
            if clamped_level != upgrade.starting_level {
                clamped_upgrades.push(ClampedUpgradeLevel {
                    player_id: player.player_id,
                    upgrade_id: upgrade.upgrade_id.clone(),
                    requested_level: upgrade.starting_level,
                    clamped_level,
                    max_level: upgrade.max_level,
                });
            }
            upgrades.push(RuntimeUpgradeState {
                upgrade_id: upgrade.upgrade_id.clone(),
                availability: upgrade.availability,
                starting_level: clamped_level,
            });
        }
        player_states.push(PlayerRuntimeAvailability {
            player_id: player.player_id,
            unavailable_tech: unavailable_tech.into_iter().collect(),
            unavailable_abilities: sorted_strings(&player.disabled_abilities),
            upgrades,
        });
        placement_plan.push(PlayerPlacementPlan {
            player_id: player.player_id,
            near_allies: sorted_players(&player.placement_priorities.ally_high),
            soft_near_allies: sorted_players(&player.placement_priorities.ally_low),
            far_enemies: sorted_players(&player.placement_priorities.enemy_high),
            soft_far_enemies: sorted_players(&player.placement_priorities.enemy_low),
        });
    }
    Ok(RuntimeScenarioAvailability {
        player_count: record.player_count,
        player_states,
        blocked_dependents,
        clamped_upgrades,
        placement_plan,
        source_fingerprint: record.stable_fingerprint()?,
    })
}

fn validate_player_count(player_count: u8) -> Result<(), EngineError> {
    if player_count == 0 || player_count > MAX_PLAYERS {
        return invalid_availability("player_count", "must be in 1..=24");
    }
    Ok(())
}

fn validate_player_id(player_id: u8, player_count: u8, field: &str) -> Result<(), EngineError> {
    if player_id == 0 || player_id > player_count {
        return invalid_availability(
            field,
            format!("player {player_id} outside 1..={player_count}"),
        );
    }
    Ok(())
}

fn validate_rawcode(field: &str, value: &str) -> Result<(), EngineError> {
    if value.len() != 4 || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        return invalid_availability(
            field,
            format!("{value:?} must be four printable ASCII bytes"),
        );
    }
    Ok(())
}

fn validate_rawcode_list(field: &str, values: &[String]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for value in values {
        validate_rawcode(field, value)?;
        if !seen.insert(value.as_str()) {
            return invalid_availability(field, format!("duplicate rawcode {value}"));
        }
    }
    Ok(())
}

fn validate_priority_group(
    player_id: u8,
    player_count: u8,
    field: &str,
    players: &[u8],
) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for target in players {
        validate_player_id(*target, player_count, field)?;
        if *target == player_id {
            return invalid_availability(field, "player cannot prioritize self");
        }
        if !seen.insert(*target) {
            return invalid_availability(field, format!("duplicate target player {target}"));
        }
    }
    Ok(())
}

fn reject_overlap(label: &str, low: &[u8], high: &[u8]) -> Result<(), EngineError> {
    let low = low.iter().copied().collect::<BTreeSet<_>>();
    for player in high {
        if low.contains(player) {
            return invalid_availability(
                format!("placement.{label}"),
                format!("player {player} cannot be both low and high priority"),
            );
        }
    }
    Ok(())
}

fn sorted_strings(values: &[String]) -> Vec<String> {
    values
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sorted_players(values: &[u8]) -> Vec<u8> {
    values
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn io_availability_error(field: &str, error: impl std::fmt::Display) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.scenario_availability.{field}"),
        reason: error.to_string(),
    }
}

fn availability_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: AVAILABILITY_VAULT_ID,
        purpose: AVAILABILITY_VAULT_PURPOSE,
        collection: AVAILABILITY_COLLECTION,
        row_key: AVAILABILITY_ROW,
        column: AVAILABILITY_COLUMN,
    }
}

fn invalid_availability<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.scenario_availability.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "scenario_availability_tests.rs"]
mod tests;
