//! Opinionated starter worlds for copy-and-tweak onboarding.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    mcp_project_tools::{LitdWorldProject, MCP_LITDWORLD_SCHEMA, MCP_LITDWORLD_SOURCE_OF_TRUTH},
    scenario::ScenarioWorldInfo,
    trigger_editor::TriggerEditorProject,
    world_validation::{WorldValidationReport, WorldValidationUseCase, require_editor_world_valid},
};

#[path = "starter_templates_builders.rs"]
mod builders;
#[path = "starter_templates_sim.rs"]
mod sim;

pub use sim::{StarterTemplateRunReport, StarterTerminalResult, run_starter_template_headless};

pub const STARTER_TEMPLATE_SCHEMA: &str = "litd.editor.starter_template.v1";
pub const STARTER_TEMPLATE_SOURCE_ISSUE: u32 = 741;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarterTemplateKind {
    TowerDefense,
    MeleeSkirmish,
    RpgArena,
    Orpg,
    MobaAos,
    HeroDefenseSurvival,
    HeroArena,
    SurvivalChaosLineWars,
    MiniGameParty,
    MazeEscape,
}

pub const STARTER_TEMPLATE_KINDS: &[StarterTemplateKind] = &[
    StarterTemplateKind::TowerDefense,
    StarterTemplateKind::MeleeSkirmish,
    StarterTemplateKind::RpgArena,
    StarterTemplateKind::Orpg,
    StarterTemplateKind::MobaAos,
    StarterTemplateKind::HeroDefenseSurvival,
    StarterTemplateKind::HeroArena,
    StarterTemplateKind::SurvivalChaosLineWars,
    StarterTemplateKind::MiniGameParty,
    StarterTemplateKind::MazeEscape,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuidedFirstEditTooltip {
    pub surface: String,
    pub target_id: String,
    pub prompt: String,
    pub suggested_edit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StarterTemplateDefinition {
    pub schema: String,
    pub source_issue: u32,
    pub kind: StarterTemplateKind,
    pub template_id: String,
    pub title: String,
    pub description: String,
    pub copy_tweak_summary: String,
    pub document: EditorDocument,
    pub scenario: ScenarioWorldInfo,
    pub trigger_project: TriggerEditorProject,
    pub guided_first_edit_tooltips: Vec<GuidedFirstEditTooltip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StarterTemplateClone {
    pub template_id: String,
    pub template_fingerprint: String,
    pub clone_fingerprint: String,
    pub clone_independent: bool,
    pub project: LitdWorldProject,
    pub scenario: ScenarioWorldInfo,
    pub trigger_project: TriggerEditorProject,
    pub guided_first_edit_tooltips: Vec<GuidedFirstEditTooltip>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterTemplateValidation {
    pub template_id: String,
    pub supported_data_only: bool,
    pub no_wasm_escape: bool,
    pub object_count: usize,
    pub unit_count: usize,
    pub region_count: usize,
    pub graph_count: usize,
    pub event_count: usize,
    pub condition_count: usize,
    pub action_count: usize,
    pub supported_action_counts: BTreeMap<String, usize>,
    pub tooltip_count: usize,
    pub document_fingerprint: String,
    pub trigger_fingerprint: String,
    pub scenario_fingerprint: String,
    pub world_validation: WorldValidationReport,
}

impl StarterTemplateKind {
    pub fn template_id(self) -> &'static str {
        match self {
            Self::TowerDefense => "starter.tower_defense.cinder_lane",
            Self::MeleeSkirmish => "starter.melee_skirmish.frost_crossing",
            Self::RpgArena => "starter.rpg_arena.crypt_ring",
            Self::Orpg => "starter.orpg.silverleaf_chronicle",
            Self::MobaAos => "starter.moba_aos.dawnward_crossing",
            Self::HeroDefenseSurvival => "starter.hero_defense.last_brazier",
            Self::HeroArena => "starter.hero_arena.sunspire_duel",
            Self::SurvivalChaosLineWars => "starter.survival_chaos.obsidian_lanes",
            Self::MiniGameParty => "starter.mini_game_party.crystal_dash",
            Self::MazeEscape => "starter.maze_escape.gloomroot_path",
        }
    }

    pub fn default_title(self) -> &'static str {
        match self {
            Self::TowerDefense => "Cinder Lane Tower Defense",
            Self::MeleeSkirmish => "Frost Crossing Melee",
            Self::RpgArena => "Crypt Ring RPG Arena",
            Self::Orpg => "Silverleaf Chronicle ORPG",
            Self::MobaAos => "Dawnward Crossing MOBA",
            Self::HeroDefenseSurvival => "Last Brazier Hero Defense",
            Self::HeroArena => "Sunspire Hero Arena",
            Self::SurvivalChaosLineWars => "Obsidian Lanes Survival Chaos",
            Self::MiniGameParty => "Crystal Dash Mini-Game",
            Self::MazeEscape => "Gloomroot Maze Escape",
        }
    }
}

pub fn starter_template_catalog() -> Result<Vec<StarterTemplateDefinition>, EngineError> {
    STARTER_TEMPLATE_KINDS
        .iter()
        .copied()
        .map(starter_template)
        .collect()
}

pub fn starter_template(
    kind: StarterTemplateKind,
) -> Result<StarterTemplateDefinition, EngineError> {
    match kind {
        StarterTemplateKind::TowerDefense => builders::tower_defense_template(),
        StarterTemplateKind::MeleeSkirmish => builders::melee_skirmish_template(),
        StarterTemplateKind::RpgArena => builders::rpg_arena_template(),
        StarterTemplateKind::Orpg => builders::orpg_template(),
        StarterTemplateKind::MobaAos => builders::moba_aos_template(),
        StarterTemplateKind::HeroDefenseSurvival => builders::hero_defense_template(),
        StarterTemplateKind::HeroArena => builders::hero_arena_template(),
        StarterTemplateKind::SurvivalChaosLineWars => builders::survival_chaos_template(),
        StarterTemplateKind::MiniGameParty => builders::mini_game_template(),
        StarterTemplateKind::MazeEscape => builders::maze_escape_template(),
    }
}

pub fn new_project_from_template(
    kind: StarterTemplateKind,
    project_id: &str,
    title: &str,
    revision_id: &str,
) -> Result<StarterTemplateClone, EngineError> {
    validate_single_line("project_id", project_id)?;
    validate_single_line("title", title)?;
    validate_single_line("revision_id", revision_id)?;
    let definition = starter_template(kind)?;
    let template_validation = validate_starter_template(&definition)?;
    let mut document = definition.document.clone();
    document.calyx_document_id = format!("calyx://world/{project_id}");
    document.revision_id = revision_id.to_string();
    document.world_properties.display_name = title.to_string();
    document.validate()?;
    let project = LitdWorldProject {
        schema: MCP_LITDWORLD_SCHEMA.to_string(),
        issue: 926,
        project_id: project_id.to_string(),
        title: title.to_string(),
        version: "0.1.0".to_string(),
        tags: vec![
            "starter-template".to_string(),
            kind.template_id().replace('.', "-"),
        ],
        source_of_truth: MCP_LITDWORLD_SOURCE_OF_TRUTH.to_string(),
        snapshot_label: format!("{project_id}-from-template"),
        document_fingerprint: stable_fingerprint(&document)?,
        document,
    };
    let clone_fingerprint = stable_fingerprint(&project.document)?;
    let clone_independent = clone_fingerprint != template_validation.document_fingerprint
        && project.document.calyx_document_id != definition.document.calyx_document_id
        && project.document.revision_id != definition.document.revision_id;
    if !clone_independent {
        return starter_error(
            "clone",
            "new-from-template clone did not get independent identity",
        );
    }
    Ok(StarterTemplateClone {
        template_id: definition.template_id,
        template_fingerprint: template_validation.document_fingerprint,
        clone_fingerprint,
        clone_independent,
        project,
        scenario: definition.scenario,
        trigger_project: definition.trigger_project,
        guided_first_edit_tooltips: definition.guided_first_edit_tooltips,
    })
}

pub fn validate_starter_template(
    definition: &StarterTemplateDefinition,
) -> Result<StarterTemplateValidation, EngineError> {
    if definition.schema != STARTER_TEMPLATE_SCHEMA
        || definition.source_issue != STARTER_TEMPLATE_SOURCE_ISSUE
        || definition.template_id != definition.kind.template_id()
    {
        return starter_error("identity", "starter template schema, issue, or id mismatch");
    }
    validate_single_line("title", &definition.title)?;
    validate_single_line("description", &definition.description)?;
    validate_single_line("copy_tweak_summary", &definition.copy_tweak_summary)?;
    definition.document.validate()?;
    definition.scenario.validate()?;
    definition.trigger_project.validate()?;
    validate_template_objects(&definition.document)?;
    validate_tooltips(definition)?;
    let world_validation =
        require_editor_world_valid(&definition.document, WorldValidationUseCase::Play)?;
    let mut supported_action_counts = BTreeMap::new();
    let mut event_count = 0;
    let mut condition_count = 0;
    let mut action_count = 0;
    for graph in &definition.trigger_project.graphs {
        if graph.wasm_escape.is_some() {
            return starter_error(
                "eca.wasm_escape",
                "starter templates must not use WASM escapes",
            );
        }
        event_count += graph.events.len();
        condition_count += graph.conditions.len();
        action_count += graph.actions.len();
        for action in &graph.actions {
            if !matches!(action.operation.as_str(), "create_unit_at_point" | "sleep") {
                return starter_error(
                    "eca.action",
                    format!("unsupported starter action {}", action.operation),
                );
            }
            *supported_action_counts
                .entry(action.operation.clone())
                .or_insert(0) += 1;
        }
    }
    let unit_count = definition
        .document
        .objects
        .values()
        .filter(|object| object.kind == "Unit")
        .count();
    Ok(StarterTemplateValidation {
        template_id: definition.template_id.clone(),
        supported_data_only: true,
        no_wasm_escape: true,
        object_count: definition.document.objects.len(),
        unit_count,
        region_count: definition.document.regions.len(),
        graph_count: definition.trigger_project.graphs.len(),
        event_count,
        condition_count,
        action_count,
        supported_action_counts,
        tooltip_count: definition.guided_first_edit_tooltips.len(),
        document_fingerprint: stable_fingerprint(&definition.document)?,
        trigger_fingerprint: stable_fingerprint(&definition.trigger_project)?,
        scenario_fingerprint: definition.scenario.stable_fingerprint()?,
        world_validation,
    })
}

fn validate_template_objects(document: &EditorDocument) -> Result<(), EngineError> {
    let allowed = BTreeSet::from(["World", "Unit", "Quest", "Item", "Doodad"]);
    for object in document.objects.values() {
        if !allowed.contains(object.kind.as_str()) {
            return starter_error(
                "object.kind",
                format!("unsupported starter object kind {}", object.kind),
            );
        }
        for (key, value) in &object.properties {
            let key = key.to_ascii_lowercase();
            let value = value.to_ascii_lowercase();
            let script_key = key.contains("lua")
                || key == "script"
                || key.ends_with(".script")
                || key.contains("source_code");
            let script_value = value.contains("lua:")
                || value.contains("function(")
                || value.contains("eval(")
                || value.contains("source_code");
            if script_key || script_value {
                return starter_error(
                    "object.properties",
                    format!("starter data must not embed script/code property {key}"),
                );
            }
        }
    }
    Ok(())
}

fn validate_tooltips(definition: &StarterTemplateDefinition) -> Result<(), EngineError> {
    if definition.guided_first_edit_tooltips.len() < 3 {
        return starter_error(
            "tooltips",
            "starter templates require at least three first-edit tips",
        );
    }
    let graph_ids = definition
        .trigger_project
        .graphs
        .iter()
        .map(|graph| graph.trigger_id.as_str())
        .collect::<BTreeSet<_>>();
    for tip in &definition.guided_first_edit_tooltips {
        validate_single_line("tooltip.surface", &tip.surface)?;
        validate_single_line("tooltip.target_id", &tip.target_id)?;
        validate_single_line("tooltip.prompt", &tip.prompt)?;
        validate_single_line("tooltip.suggested_edit", &tip.suggested_edit)?;
        let target_known = definition.document.objects.contains_key(&tip.target_id)
            || definition.document.regions.contains_key(&tip.target_id)
            || graph_ids.contains(tip.target_id.as_str());
        if !target_known {
            return starter_error(
                "tooltip.target_id",
                format!(
                    "tooltip target {} does not exist in template",
                    tip.target_id
                ),
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_single_line(
    field: impl Into<String>,
    value: &str,
) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\r', '\n', '\t']) || value.len() > 2048 {
        return starter_error(field, "value must be non-empty bounded single-line text");
    }
    Ok(())
}

pub(crate) fn starter_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.starter_templates.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "starter_templates_tests.rs"]
mod tests;
