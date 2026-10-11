//! Issue #743 no-code tower-defense acceptance proof helpers.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    history::stable_fingerprint,
    mcp_trigger_tools::{EcaGraph, EcaNode},
    starter_templates::{
        StarterTemplateClone, StarterTemplateKind, StarterTemplateRunReport, StarterTerminalResult,
        new_project_from_template, run_starter_template_headless,
    },
};

pub const ISSUE743_TOWER_DEFENSE_SCHEMA: &str = "litd.issue743.tower_defense_fsv.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoCodeEditorOperation {
    pub surface: String,
    pub target_id: String,
    pub action: String,
    pub readback: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue743TowerDefenseAcceptance {
    pub no_lua_or_code_escape: bool,
    pub terrain_path_ready: bool,
    pub asset_products_assigned: bool,
    pub object_roles: BTreeMap<String, usize>,
    pub trigger_ids: Vec<String>,
    pub document_fingerprint: String,
    pub trigger_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue743TowerDefenseBuild {
    pub schema: String,
    pub issue: u32,
    pub source_of_truth: String,
    pub clone: StarterTemplateClone,
    pub operations: Vec<NoCodeEditorOperation>,
    pub acceptance: Issue743TowerDefenseAcceptance,
}

pub fn build_issue743_tower_defense_project(
    project_id: &str,
    title: &str,
    revision_id: &str,
) -> Result<Issue743TowerDefenseBuild, EngineError> {
    let mut clone = new_project_from_template(
        StarterTemplateKind::TowerDefense,
        project_id,
        title,
        revision_id,
    )?;
    clone.project.tags.push("issue743-no-code-td".to_string());
    clone.project.snapshot_label = "issue743-no-code-tower-defense-export".to_string();
    clone.trigger_project.project_id = project_id.replace(['-', '.'], "_");

    set_object_property(
        &mut clone,
        "td_tower_alpha",
        "stat.damage",
        "5",
        "Object",
        "raise alpha tower damage from 4 to 5",
    )?;
    set_object_property(
        &mut clone,
        "td_creep_03",
        "template.wave_label",
        "final_wave",
        "Object",
        "tag final creep wave for non-coder readback",
    )?;
    set_region_property(
        &mut clone,
        "td_lane_start",
        "path.next_region",
        "td_keep",
        "Terrain",
        "author creep path from lane start to keep",
    )?;
    add_defeat_trigger(&mut clone)?;
    refresh_clone(&mut clone, revision_id)?;

    let operations = vec![
        operation(
            "Terrain",
            "td_lane_start",
            "Set path.next_region to td_keep.",
            "Creep path endpoints are td_lane_start -> td_keep.",
        ),
        operation(
            "Objects",
            "td_tower_alpha",
            "Set stat.damage to 5.",
            "Tower alpha balances the first wave without code.",
        ),
        operation(
            "Objects",
            "td_creep_03",
            "Set template.wave_label to final_wave.",
            "Final creep wave is identified in object data.",
        ),
        operation(
            "Triggers",
            "trg_td_defeat",
            "Add game_defeat event and sleep action graph.",
            "Win and lose terminal triggers are both data-only ECA graphs.",
        ),
        operation(
            "Assets",
            "td_tower_alpha",
            "Reuse the assigned vault model product.",
            "All Unit objects retain model_product_id assignments.",
        ),
    ];
    let acceptance = inspect_acceptance(&clone)?;
    Ok(Issue743TowerDefenseBuild {
        schema: ISSUE743_TOWER_DEFENSE_SCHEMA.to_string(),
        issue: 743,
        source_of_truth:
            "editor no-code object/terrain/asset/trigger data plus starter-template headless simulation"
                .to_string(),
        clone,
        operations,
        acceptance,
    })
}

pub fn build_issue743_leak_all_towers_sold_project(
    build: &Issue743TowerDefenseBuild,
) -> Result<StarterTemplateClone, EngineError> {
    let mut clone = build.clone.clone();
    for tower_id in ["td_tower_alpha", "td_tower_beta"] {
        let object = clone
            .project
            .document
            .objects
            .get_mut(tower_id)
            .ok_or_else(|| issue743_error("edge.tower", format!("missing {tower_id}")))?;
        object
            .properties
            .insert("template.sold".to_string(), "true".to_string());
        object
            .properties
            .insert("stat.attack_period_ticks".to_string(), "10000".to_string());
        object
            .properties
            .insert("issue743.edge".to_string(), "all_towers_sold".to_string());
    }
    refresh_clone(&mut clone, "issue743-edge-all-towers-sold")?;
    Ok(clone)
}

pub fn run_issue743_tower_defense_victory(
    clone: &StarterTemplateClone,
    ticks: u64,
) -> Result<StarterTemplateRunReport, EngineError> {
    let report = run_starter_template_headless(clone, ticks)?;
    if report.terminal_result != StarterTerminalResult::Win
        || !report.deterministic
        || report.final_state_hash != report.replay_state_hash
        || !report
            .executed_trigger_ids
            .iter()
            .any(|id| id == "trg_td_victory")
    {
        return Err(issue743_error(
            "victory",
            "tower-defense run did not reach deterministic victory through victory trigger",
        ));
    }
    Ok(report)
}

pub fn run_issue743_leak_loss(
    clone: &StarterTemplateClone,
    ticks: u64,
) -> Result<StarterTemplateRunReport, EngineError> {
    let report = run_starter_template_headless(clone, ticks)?;
    if report.terminal_result != StarterTerminalResult::Loss
        || !report.deterministic
        || report.base_health > 0
        || !report
            .executed_trigger_ids
            .iter()
            .any(|id| id == "trg_td_defeat")
    {
        return Err(issue743_error(
            "leak_loss",
            "sold-tower leak edge did not reach deterministic defeat through defeat trigger",
        ));
    }
    Ok(report)
}

pub fn assert_issue743_windows_reruns_identical(
    clone: &StarterTemplateClone,
    ticks: u64,
    reruns: usize,
) -> Result<Vec<StarterTemplateRunReport>, EngineError> {
    if reruns < 2 {
        return Err(issue743_error("reruns", "at least two reruns are required"));
    }
    let mut reports = Vec::new();
    for _ in 0..reruns {
        reports.push(run_issue743_tower_defense_victory(clone, ticks)?);
    }
    let first_hash = &reports[0].final_state_hash;
    if reports
        .iter()
        .any(|report| &report.final_state_hash != first_hash)
    {
        return Err(issue743_error(
            "reruns.state_hash",
            "Windows deterministic reruns produced different state hashes",
        ));
    }
    Ok(reports)
}

fn inspect_acceptance(
    clone: &StarterTemplateClone,
) -> Result<Issue743TowerDefenseAcceptance, EngineError> {
    clone.project.document.validate()?;
    clone.trigger_project.validate()?;
    let mut roles = BTreeMap::new();
    let mut all_units_have_assets = true;
    for object in clone.project.document.objects.values() {
        if object.kind == "Unit" {
            let role = object
                .properties
                .get("template.role")
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            *roles.entry(role).or_insert(0) += 1;
            all_units_have_assets &= object.properties.contains_key("model_product_id");
        }
    }
    let trigger_ids = clone
        .trigger_project
        .graphs
        .iter()
        .map(|graph| graph.trigger_id.clone())
        .collect::<Vec<_>>();
    let trigger_set = trigger_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let no_lua_or_code_escape = clone
        .trigger_project
        .graphs
        .iter()
        .all(|graph| graph.wasm_escape.is_none())
        && clone.project.document.objects.values().all(|object| {
            object
                .properties
                .iter()
                .all(|(key, value)| !looks_like_code(key) && !looks_like_code(value))
        });
    let terrain_path_ready = clone.project.document.regions.contains_key("td_lane_start")
        && clone.project.document.regions.contains_key("td_keep")
        && clone
            .project
            .document
            .regions
            .get("td_lane_start")
            .and_then(|region| region.properties.get("path.next_region"))
            .map(String::as_str)
            == Some("td_keep");
    let terminal_triggers = trigger_set.contains("trg_td_victory")
        && trigger_set.contains("trg_td_defeat")
        && trigger_set.contains("trg_td_map_init");
    if !(no_lua_or_code_escape
        && terrain_path_ready
        && all_units_have_assets
        && roles.get("base") == Some(&1)
        && roles.get("tower").copied().unwrap_or(0) >= 2
        && roles.get("creep").copied().unwrap_or(0) >= 3
        && terminal_triggers)
    {
        return Err(issue743_error(
            "acceptance",
            "tower-defense no-code world is missing required terrain, object, trigger, or asset data",
        ));
    }
    Ok(Issue743TowerDefenseAcceptance {
        no_lua_or_code_escape,
        terrain_path_ready,
        asset_products_assigned: all_units_have_assets,
        object_roles: roles,
        trigger_ids,
        document_fingerprint: stable_fingerprint(&clone.project.document)?,
        trigger_fingerprint: clone.trigger_project.stable_fingerprint()?,
    })
}

fn set_object_property(
    clone: &mut StarterTemplateClone,
    object_id: &str,
    key: &str,
    value: &str,
    _surface: &str,
    _summary: &str,
) -> Result<(), EngineError> {
    let object = clone
        .project
        .document
        .objects
        .get_mut(object_id)
        .ok_or_else(|| issue743_error("object", format!("missing object {object_id}")))?;
    object.properties.insert(key.to_string(), value.to_string());
    Ok(())
}

fn set_region_property(
    clone: &mut StarterTemplateClone,
    region_id: &str,
    key: &str,
    value: &str,
    _surface: &str,
    _summary: &str,
) -> Result<(), EngineError> {
    let region = clone
        .project
        .document
        .regions
        .get_mut(region_id)
        .ok_or_else(|| issue743_error("region", format!("missing region {region_id}")))?;
    region.properties.insert(key.to_string(), value.to_string());
    Ok(())
}

fn add_defeat_trigger(clone: &mut StarterTemplateClone) -> Result<(), EngineError> {
    clone
        .trigger_project
        .graphs
        .retain(|graph| graph.trigger_id != "trg_td_defeat");
    clone.trigger_project.graphs.push(EcaGraph {
        trigger_id: "trg_td_defeat".to_string(),
        display_name: "Defeat Banner".to_string(),
        enabled: true,
        folder_path: Some("Terminal".to_string()),
        initially_on: true,
        comment: None,
        events: vec![node(
            "evt_defeat",
            "game_defeat",
            1,
            &[("player_id", "player_one")],
        )],
        conditions: Vec::new(),
        actions: vec![node(
            "act_defeat_banner",
            "sleep",
            1,
            &[("duration_ms", "250"), ("reason", "show defeat banner")],
        )],
        wasm_escape: None,
    });
    clone.trigger_project.validate()
}

fn refresh_clone(clone: &mut StarterTemplateClone, revision_id: &str) -> Result<(), EngineError> {
    clone.project.document.revision_id = revision_id.to_string();
    clone.project.document.validate()?;
    clone.trigger_project.validate()?;
    let fingerprint = stable_fingerprint(&clone.project.document)?;
    clone.project.document_fingerprint = fingerprint.clone();
    clone.clone_fingerprint = fingerprint;
    Ok(())
}

fn operation(
    surface: &str,
    target_id: &str,
    action: &str,
    readback: &str,
) -> NoCodeEditorOperation {
    NoCodeEditorOperation {
        surface: surface.to_string(),
        target_id: target_id.to_string(),
        action: action.to_string(),
        readback: readback.to_string(),
    }
}

fn node(id: &str, op: &str, order: u32, args: &[(&str, &str)]) -> EcaNode {
    EcaNode {
        node_id: id.to_string(),
        operation: op.to_string(),
        order,
        args: args
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
        variable_refs: Vec::new(),
    }
}

fn looks_like_code(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("lua:")
        || value.contains("function(")
        || value.contains("eval(")
        || value.contains("source_code")
}

fn issue743_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.issue743_tower_defense.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "tower_defense_fsv_tests.rs"]
mod tests;
