//! Editor module rebase manifest for WC3 parity, story, RPG, economy, and lore.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::EditorMcpError;

#[path = "modules_catalog.rs"]
mod modules_catalog;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorModuleFamily {
    Wc3TerrainWorld,
    Wc3ObjectData,
    Wc3TriggerEca,
    Wc3RuntimeSystems,
    Wc3PlayDistribute,
    VisualStorytelling,
    RpgSystems,
    ScarcityEconomy,
    LoreWorldbuilding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineBinding {
    EditorDocument,
    AppRuntime,
    SceneGraph,
    Ecs,
    DeterministicSim,
    PresentationFrame,
    RendererBatches,
    AssetPipeline,
    CameraSystem,
    RuntimeSystems,
    Serialization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalyxBinding {
    AsterStorage,
    LedgerProvenance,
    LoweringValve,
    SemanticSearch,
    MultiLensPanel,
    WardGrounding,
    LoomReactive,
    AssayOracle,
    AgentRuntime,
    LoreConstellations,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorModuleRebaseManifest {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub root_editor_epic: u32,
    pub engine_crate: String,
    pub editor_crate: String,
    pub calyx_backend: String,
    pub shared_capability_layer: String,
    pub modules: Vec<EditorModuleRebase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorModuleRebase {
    pub id: String,
    pub family: EditorModuleFamily,
    pub source_issue: u32,
    pub source_title: String,
    pub gui_surface: String,
    pub capability_namespace: String,
    pub mcp_tool_group: String,
    pub engine_bindings: Vec<EngineBinding>,
    pub calyx_bindings: Vec<CalyxBinding>,
    pub lowered_outputs: Vec<String>,
    /// Documentation pointers into `docs/readme/*.png` for composition/quality
    /// context ONLY — never a behavioral spec. Legacy-composite HUD elements
    /// (e.g. `ED12-play-topdown.png` minimap/quest trackers/player marker) are
    /// OBSOLETE per ADR-944 / #944 / #1510 and must not be implemented in
    /// packaged player runtime.
    pub readme_refs: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorModuleRebaseReport {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub root_editor_epic: u32,
    pub module_count: usize,
    pub source_issues: Vec<u32>,
    pub families: Vec<EditorModuleFamily>,
    pub gui_surface_count: usize,
    pub mcp_tool_group_count: usize,
    pub rust_engine_bound_count: usize,
    pub calyx_bound_count: usize,
    pub lowering_valve_count: usize,
    pub shared_capability_count: usize,
    pub readme_refs: Vec<String>,
}

pub const REQUIRED_MODULE_IDS: [&str; 9] = [
    "wc3.terrain_world",
    "wc3.object_data",
    "wc3.trigger_eca",
    "wc3.runtime_systems",
    "wc3.play_distribute",
    "story.visual_director",
    "rpg.flagship_systems",
    "economy.scarcity_full_loot",
    "lore.worldbuilding",
];

pub fn default_module_rebase_manifest() -> Result<EditorModuleRebaseManifest, EditorMcpError> {
    let manifest = EditorModuleRebaseManifest {
        schema_version: 1,
        parent_issue: 1142,
        root_editor_epic: 1137,
        engine_crate: "litd-engine".to_string(),
        editor_crate: "litd-editor".to_string(),
        calyx_backend: "Calyx Aster + Ledger + lowered deterministic data".to_string(),
        shared_capability_layer: "editor.capabilities.shared".to_string(),
        modules: modules_catalog::default_modules(),
    };
    validate_module_rebase_manifest(&manifest)?;
    Ok(manifest)
}

pub fn build_module_rebase_report(
    manifest: &EditorModuleRebaseManifest,
) -> Result<EditorModuleRebaseReport, EditorMcpError> {
    validate_module_rebase_manifest(manifest)?;
    let readme_refs = manifest
        .modules
        .iter()
        .flat_map(|module| module.readme_refs.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(EditorModuleRebaseReport {
        schema_version: manifest.schema_version,
        parent_issue: manifest.parent_issue,
        root_editor_epic: manifest.root_editor_epic,
        module_count: manifest.modules.len(),
        source_issues: manifest
            .modules
            .iter()
            .map(|module| module.source_issue)
            .collect(),
        families: manifest
            .modules
            .iter()
            .map(|module| module.family)
            .collect(),
        gui_surface_count: manifest
            .modules
            .iter()
            .filter(|module| !module.gui_surface.trim().is_empty())
            .count(),
        mcp_tool_group_count: manifest
            .modules
            .iter()
            .filter(|module| !module.mcp_tool_group.trim().is_empty())
            .count(),
        rust_engine_bound_count: manifest
            .modules
            .iter()
            .filter(|module| !module.engine_bindings.is_empty())
            .count(),
        calyx_bound_count: manifest
            .modules
            .iter()
            .filter(|module| !module.calyx_bindings.is_empty())
            .count(),
        lowering_valve_count: manifest
            .modules
            .iter()
            .filter(|module| module.calyx_bindings.contains(&CalyxBinding::LoweringValve))
            .count(),
        shared_capability_count: manifest
            .modules
            .iter()
            .filter(|module| module.capability_namespace.starts_with("editor."))
            .count(),
        readme_refs,
    })
}

pub fn validate_module_rebase_manifest(
    manifest: &EditorModuleRebaseManifest,
) -> Result<(), EditorMcpError> {
    if manifest.schema_version != 1 {
        return invalid_modules("schema_version", "expected schema version 1");
    }
    if manifest.engine_crate != "litd-engine" || manifest.editor_crate != "litd-editor" {
        return invalid_modules(
            "crate_bindings",
            "manifest must bind to litd-engine and litd-editor",
        );
    }
    if !manifest
        .calyx_backend
        .to_ascii_lowercase()
        .contains("calyx")
    {
        return invalid_modules("calyx_backend", "Calyx backend binding is required");
    }
    let mut by_id = BTreeMap::<&str, &EditorModuleRebase>::new();
    let mut source_issues = BTreeSet::new();
    for module in &manifest.modules {
        if by_id.insert(module.id.as_str(), module).is_some() {
            return invalid_modules("modules.id", format!("duplicate module id {}", module.id));
        }
        if !source_issues.insert(module.source_issue) {
            return invalid_modules(
                "modules.source_issue",
                format!("duplicate source issue {}", module.source_issue),
            );
        }
        validate_module(module)?;
    }
    for required in REQUIRED_MODULE_IDS {
        if !by_id.contains_key(required) {
            return invalid_modules("modules.required", format!("missing {required}"));
        }
    }
    Ok(())
}

fn validate_module(module: &EditorModuleRebase) -> Result<(), EditorMcpError> {
    if module.id.trim().is_empty()
        || module.source_title.trim().is_empty()
        || module.gui_surface.trim().is_empty()
        || module.capability_namespace.trim().is_empty()
        || module.mcp_tool_group.trim().is_empty()
    {
        return invalid_modules("module.required_fields", "module strings must be non-empty");
    }
    if module.source_issue == 0 {
        return invalid_modules("module.source_issue", "source issue must be non-zero");
    }
    if !module.capability_namespace.starts_with("editor.")
        || !module.mcp_tool_group.starts_with("editor.")
    {
        return invalid_modules(
            "module.capability_layer",
            "GUI and MCP surfaces must use the shared editor capability namespace",
        );
    }
    if module.engine_bindings.is_empty() {
        return invalid_modules("module.engine_bindings", "Rust engine binding is required");
    }
    if !module.calyx_bindings.contains(&CalyxBinding::AsterStorage)
        || !module
            .calyx_bindings
            .contains(&CalyxBinding::LedgerProvenance)
        || !module.calyx_bindings.contains(&CalyxBinding::LoweringValve)
    {
        return invalid_modules(
            "module.calyx_bindings",
            "Aster storage, Ledger provenance, and the lowering valve are required",
        );
    }
    if module.lowered_outputs.is_empty() {
        return invalid_modules("module.lowered_outputs", "lowered outputs are required");
    }
    if module
        .readme_refs
        .iter()
        .any(|path| !path.starts_with("docs/readme/") || !path.ends_with(".png"))
    {
        return invalid_modules(
            "module.readme_refs",
            "README visual refs must point at docs/readme PNGs",
        );
    }
    if module.status != "rebased_to_rust_engine_calyx" {
        return invalid_modules("module.status", "module must be marked rebased");
    }
    let legacy_text = format!(
        "{} {} {} {} {}",
        module.source_title,
        module.gui_surface,
        module.capability_namespace,
        module.mcp_tool_group,
        module.status
    )
    .to_ascii_lowercase();
    if legacy_text.contains("g3n") || legacy_text.contains("legacy go") {
        return invalid_modules("module.legacy", "legacy engine references are not allowed");
    }
    Ok(())
}

/// Cohesive descriptor for one editor-module rebase catalog entry (#1545). Groups
/// the eleven fields `module()` previously took positionally so catalog call sites
/// read as named fields and the builder stays under the clippy argument threshold.
struct ModuleSpec<'a> {
    id: &'a str,
    family: EditorModuleFamily,
    source_issue: u32,
    source_title: &'a str,
    gui_surface: &'a str,
    capability_namespace: &'a str,
    mcp_tool_group: &'a str,
    engine_bindings: &'a [EngineBinding],
    calyx_bindings: &'a [CalyxBinding],
    lowered_outputs: &'a [&'a str],
    readme_refs: &'a [&'a str],
}

fn module(spec: ModuleSpec) -> EditorModuleRebase {
    let ModuleSpec {
        id,
        family,
        source_issue,
        source_title,
        gui_surface,
        capability_namespace,
        mcp_tool_group,
        engine_bindings,
        calyx_bindings,
        lowered_outputs,
        readme_refs,
    } = spec;
    EditorModuleRebase {
        id: id.to_string(),
        family,
        source_issue,
        source_title: source_title.to_string(),
        gui_surface: gui_surface.to_string(),
        capability_namespace: capability_namespace.to_string(),
        mcp_tool_group: mcp_tool_group.to_string(),
        engine_bindings: engine_bindings.to_vec(),
        calyx_bindings: calyx_bindings.to_vec(),
        lowered_outputs: strings(lowered_outputs),
        readme_refs: strings(readme_refs),
        status: "rebased_to_rust_engine_calyx".to_string(),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn invalid_modules<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EditorMcpError> {
    Err(EditorMcpError::InvalidConfig {
        field: format!("editor.modules.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "modules_tests.rs"]
mod tests;
