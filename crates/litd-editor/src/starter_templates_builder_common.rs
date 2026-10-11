use std::collections::BTreeMap;

use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, EditorRegion},
    mcp_trigger_tools::{EcaGraph, EcaNode},
    scenario::{
        DayNightCycle, MapDescription, MapOptions, MapRuleset, ScenarioWorldInfo,
        StartLocationPolicy,
    },
    starter_templates::{
        GuidedFirstEditTooltip, STARTER_TEMPLATE_SCHEMA, STARTER_TEMPLATE_SOURCE_ISSUE,
        StarterTemplateDefinition, StarterTemplateKind, starter_error,
    },
    trigger_editor::{
        TRIGGER_EDITOR_SCHEMA, TriggerEditorProject, TriggerEditorRegion, TriggerEditorUnit,
        TriggerEditorUnitType, TriggerEditorVariable, TriggerEditorVariableType,
    },
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
        create_editor_world_record,
    },
};

/// Canonical starter unit mesh: Drive-backed Quaternius animatedmech Stan glTF.
/// Must exist as a download record in the local-asset-vault (fixture for #1543/#858).
/// Never point at packs not rematerialized on this host (e.g. medievaldungeon).
const MODEL_PATH: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";

pub(super) fn finish(
    kind: StarterTemplateKind,
    document: EditorDocument,
    scenario: ScenarioWorldInfo,
    graphs: Vec<EcaGraph>,
    tips: Vec<GuidedFirstEditTooltip>,
) -> Result<StarterTemplateDefinition, EngineError> {
    let trigger_project = trigger_project(&document, kind.template_id(), graphs)?;
    let definition = StarterTemplateDefinition {
        schema: STARTER_TEMPLATE_SCHEMA.to_string(),
        source_issue: STARTER_TEMPLATE_SOURCE_ISSUE,
        kind,
        template_id: kind.template_id().to_string(),
        title: kind.default_title().to_string(),
        description: format!("Opinionated no-code starter world for {}", kind.default_title()),
        copy_tweak_summary: "New from template clones a play-ready world with objects, regions, scenario metadata, ECA graphs, and first-edit tips.".to_string(),
        document,
        scenario,
        trigger_project,
        guided_first_edit_tooltips: tips,
    };
    definition.document.validate()?;
    Ok(definition)
}

pub(super) fn base_document(
    kind: StarterTemplateKind,
    width: u32,
    height: u32,
) -> Result<EditorDocument, EngineError> {
    let spec = EditorWorldCreationSpec {
        calyx_document_id: format!("calyx://template/{}", kind.template_id()),
        revision_id: "starter-template-rev-1".to_string(),
        properties: EditorWorldProperties {
            display_name: kind.default_title().to_string(),
            description: format!("Copy-and-tweak starter world {}", kind.template_id()),
            width_tiles: width,
            height_tiles: height,
            random_height_seed: Some(741),
            playable_bounds: EditorPlayableBounds {
                min_tile_x: 0,
                min_tile_z: 0,
                max_tile_x_exclusive: width as i32,
                max_tile_z_exclusive: height as i32,
            },
            ..EditorWorldProperties::default()
        },
    };
    Ok(create_editor_world_record(spec)?.document)
}

pub(super) fn scenario(
    kind: StarterTemplateKind,
    min_players: u8,
    max_players: u8,
    ruleset: MapRuleset,
) -> Result<ScenarioWorldInfo, EngineError> {
    let start_locations = if ruleset == MapRuleset::Melee {
        StartLocationPolicy::Random
    } else {
        StartLocationPolicy::Fixed
    };
    ScenarioWorldInfo::new(
        MapDescription {
            name: kind.default_title().to_string(),
            suggested_players_min: min_players,
            suggested_players_max: max_players,
            author: "Light in the Dark".to_string(),
            description: format!("Starter template {}", kind.template_id()),
            recommended_players: format!("{min_players}-{max_players}"),
        },
        MapOptions {
            ruleset,
            start_locations,
            allow_observers: true,
            hide_minimap_preview: false,
            mask_partial_vision: true,
            fixed_player_settings: ruleset != MapRuleset::Melee,
            fixed_forces: ruleset != MapRuleset::Melee,
            water_waves_cliff_shores: false,
            water_waves_rolling_shores: false,
            terrain_fog_enabled: false,
            day_night_cycle: DayNightCycle::Disabled,
        },
    )
}

pub(super) fn assign_models(document: &mut EditorDocument) -> Result<(), EngineError> {
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| record.relative_path == MODEL_PATH)
        .cloned()
        .ok_or_else(|| {
            starter_error::<()>(
                "asset_vault",
                format!(
                    "missing starter mesh record relative_path={MODEL_PATH}; \
                     rematerialize Quaternius animatedmech via scripts/assets/intake_cc0_assets.py \
                     --source quaternius --quaternius-pack-slug animatedmech"
                ),
            )
            .unwrap_err()
        })?;
    let unit_ids = document
        .objects
        .values()
        .filter(|object| object.kind == "Unit")
        .map(|object| object.id.clone())
        .collect::<Vec<_>>();
    for unit_id in unit_ids {
        assign_vault_model_to_unit(document, &unit_id, &record)?;
    }
    Ok(())
}

fn trigger_project(
    document: &EditorDocument,
    project_id: &str,
    graphs: Vec<EcaGraph>,
) -> Result<TriggerEditorProject, EngineError> {
    let mut unit_types = BTreeMap::new();
    for object in document
        .objects
        .values()
        .filter(|object| object.kind == "Unit")
    {
        let ty = object.properties["template.unit_type_id"].clone();
        unit_types
            .entry(ty.clone())
            .or_insert(TriggerEditorUnitType {
                unit_type_id: ty.clone(),
                display_name: ty.replace('_', " "),
            });
    }
    for action in graphs.iter().flat_map(|graph| graph.actions.iter()) {
        if let Some(ty) = action.args.get("unit_type_id") {
            unit_types
                .entry(ty.clone())
                .or_insert(TriggerEditorUnitType {
                    unit_type_id: ty.clone(),
                    display_name: ty.replace('_', " "),
                });
        }
    }
    let regions = document
        .regions
        .iter()
        .map(|(id, region)| {
            (
                id.clone(),
                TriggerEditorRegion {
                    region_id: id.clone(),
                    center: region.center,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let initial_units = document
        .objects
        .values()
        .filter(|object| object.kind == "Unit")
        .map(|object| {
            let unit = TriggerEditorUnit {
                unit_id: object.id.clone(),
                unit_type_id: object.properties["template.unit_type_id"].clone(),
                owner_player_id: object.properties["template.owner_player_id"].clone(),
                region_id: object.properties["template.region_id"].clone(),
                alive: true,
                position: object.transform.translation,
            };
            (unit.unit_id.clone(), unit)
        })
        .collect::<BTreeMap<_, _>>();
    let project = TriggerEditorProject {
        schema: TRIGGER_EDITOR_SCHEMA.to_string(),
        project_id: project_id.replace('.', "_"),
        unit_types,
        regions,
        variables: BTreeMap::from([
            flag("td_wave_ready", "true"),
            flag("td_rewards_enabled", "true"),
            flag("melee_opening_enabled", "true"),
            flag("arena_live", "true"),
        ]),
        initial_units,
        graphs,
    };
    project.validate()?;
    Ok(project)
}

pub(super) fn unit(
    id: &str,
    role: &str,
    owner: &str,
    region_id: &str,
    x: f32,
    z: f32,
    stats: &[(&str, &str)],
) -> EditorObject {
    let mut object = EditorObject::new(
        id,
        "Unit",
        Some("world".to_string()),
        Transform::from_translation(Vec3::new(x, 0.0, z)),
    )
    .with_property("faction", owner)
    .with_property("template.role", role)
    .with_property("template.owner_player_id", owner)
    .with_property("template.region_id", region_id)
    .with_property("template.unit_type_id", format!("{role}_type"));
    for (key, value) in stats {
        object = object.with_property(*key, *value);
    }
    object
}

pub(super) fn item(id: &str, x: f32, z: f32) -> EditorObject {
    EditorObject::new(
        id,
        "Item",
        Some("world".to_string()),
        Transform::from_translation(Vec3::new(x, 0.0, z)),
    )
    .with_property("template.role", "healing_font")
}

pub(super) fn add_quest(
    document: &mut EditorDocument,
    id: &str,
    start: &str,
    complete: &str,
) -> Result<(), EngineError> {
    document.upsert_object(
        EditorObject::new(id, "Quest", Some("world".to_string()), Transform::IDENTITY)
            .with_property("start_object", start)
            .with_property("complete_object", complete),
    )
}

pub(super) fn region(id: &str, x: f32, y: f32, z: f32, radius: f32) -> EditorRegion {
    EditorRegion::new(
        id,
        id.replace('_', " "),
        Vec3::new(x, y, z),
        Vec3::new(radius, 4.0, radius),
    )
}

pub(super) fn graph(
    trigger_id: &str,
    display_name: &str,
    folder: &str,
    events: Vec<EcaNode>,
    conditions: Vec<EcaNode>,
    actions: Vec<EcaNode>,
) -> EcaGraph {
    EcaGraph {
        trigger_id: trigger_id.to_string(),
        display_name: display_name.to_string(),
        enabled: true,
        folder_path: Some(folder.to_string()),
        initially_on: true,
        comment: None,
        events,
        conditions,
        actions,
        wasm_escape: None,
    }
}

pub(super) fn node(id: &str, op: &str, order: u32, args: &[(&str, &str)]) -> EcaNode {
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

pub(super) fn sleep(id: &str, order: u32, reason: &str) -> EcaNode {
    node(
        id,
        "sleep",
        order,
        &[("duration_ms", "250"), ("reason", reason)],
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn create_unit(
    id: &str,
    order: u32,
    unit_id: &str,
    unit_type_id: &str,
    owner: &str,
    region: &str,
    x: f32,
    y: f32,
    z: f32,
) -> EcaNode {
    node(
        id,
        "create_unit_at_point",
        order,
        &[
            ("unit_id", unit_id),
            ("unit_type_id", unit_type_id),
            ("owner_player_id", owner),
            ("region_id", region),
            ("x", &format!("{x}")),
            ("y", &format!("{y}")),
            ("z", &format!("{z}")),
        ],
    )
}

pub(super) fn tip(
    surface: &str,
    target_id: &str,
    prompt: &str,
    suggested_edit: &str,
) -> GuidedFirstEditTooltip {
    GuidedFirstEditTooltip {
        surface: surface.to_string(),
        target_id: target_id.to_string(),
        prompt: prompt.to_string(),
        suggested_edit: suggested_edit.to_string(),
    }
}

fn flag(name: &str, initial_value: &str) -> (String, TriggerEditorVariable) {
    (
        name.to_string(),
        TriggerEditorVariable {
            name: name.to_string(),
            value_type: TriggerEditorVariableType::Boolean,
            initial_value: initial_value.to_string(),
            array_values: Vec::new(),
        },
    )
}
