//! #1843: the editor object-data lowering valve that emits the RUNTIME DATASETS the
//! `litd-game` host instantiates the Wave-4 systems from.
//!
//! This is the single production path from the real authored records to the lowered
//! economy ruleset (#818 unit econ fields), the match blueprint (players/forces/victory
//! from map data, #836/#838), and the runtime constants (#827 gameplay constants +
//! #848 day/night). The in-engine authoring builders (`EconomyRulesetAuthor`,
//! `MatchBlueprint`) are populated HERE from the authored `EditorDocument` -- they are no
//! longer the production SOURCE (they stay for engine unit tests only, #1312/#1783).
//!
//! Authoring conventions (kind + property bag, the editor's object-data model):
//! - A runtime-config object (`kind = "GameplayConstant"`) carries the `runtime.*` tuning.
//! - Unit-type objects (`kind = "Unit"`) carry the #818 `economy.*` cost fields plus a
//!   `runtime.role` of `worker` | `soldier` | `structure`; a structure additionally
//!   declares `runtime.footprint_w/h` and a comma-separated `runtime.trains`.
//! - Resource-node objects (`kind = "ResourceNode"`) carry `resource.*` fields; the node
//!   cell comes from the object transform.
//! - A unit's `template.owner_player_id` (`player_one`/`player_two`/... or a number)
//!   assigns it to a player slot; optional `template.force_id` teams owners into shared
//!   forces (2v2). Absent force ids → each owner is its own force (classic melee).

use std::collections::BTreeMap;

use litd_engine::EngineError;
use litd_engine::game_runtime::{
    AuthoredUnit, EconomyRulesetAuthor, LoweredResourceKind, LoweredResourceNode,
    LoweredStructureType, LoweredUnitEconomy, LoweredUpkeepTier, MatchBlueprint, OrderTargetKind,
    RuntimeDatasetsManifest, UnitCapability, VictoryCondition, lower_runtime_constants,
};

/// The lowered "move" ability id every mobile unit gets, so the match core can validate a
/// point-target move order issued through a group (#851).
pub const MOVE_ABILITY: u16 = 1;

use crate::history::{EditorDocument, EditorObject};
use crate::quick_share::{
    QuickShareBuildRequest, QuickSharePackage, build_quick_share_package,
    compute_package_manifest_fingerprint, validate_quick_share_package,
};

#[path = "runtime_datasets_lowering_ai.rs"]
mod runtime_datasets_lowering_ai;
use runtime_datasets_lowering_ai::lower_ai_profiles;

#[path = "runtime_datasets_lowering_regions.rs"]
mod runtime_datasets_lowering_regions;
use runtime_datasets_lowering_regions::lower_document_regions;

#[path = "runtime_datasets_lowering_slots.rs"]
mod runtime_datasets_lowering_slots;
use runtime_datasets_lowering_slots::{
    build_player_slots, dense_owner, note_owner_force, owner_to_player_id, parse_force_id,
};

const RUNTIME_CONFIG_KIND: &str = "GameplayConstant";
const UNIT_KIND: &str = "Unit";
const RESOURCE_NODE_KIND: &str = "ResourceNode";

fn lower_error<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::GamePackageInvalidConfig {
        field: format!("litd_editor.runtime_datasets_lowering.{field}"),
        reason: reason.into(),
    })
}

fn str_prop<'a>(object: &'a EditorObject, key: &str) -> Option<&'a str> {
    object.properties.get(key).map(String::as_str)
}

fn u32_prop(object: &EditorObject, key: &str) -> Result<u32, EngineError> {
    match str_prop(object, key) {
        Some(value) => {
            value
                .trim()
                .parse::<u32>()
                .map_err(|error| EngineError::GamePackageInvalidConfig {
                    field: format!("litd_editor.runtime_datasets_lowering.{}.{key}", object.id),
                    reason: format!("expected u32, got {value:?}: {error}"),
                })
        }
        None => lower_error(
            &format!("{}.{key}", object.id),
            format!("required property {key} is absent"),
        ),
    }
}

fn u32_prop_or(object: &EditorObject, key: &str, default: u32) -> Result<u32, EngineError> {
    if object.properties.contains_key(key) {
        u32_prop(object, key)
    } else {
        Ok(default)
    }
}

fn u16_prop(object: &EditorObject, key: &str) -> Result<u16, EngineError> {
    let value = u32_prop(object, key)?;
    u16::try_from(value).map_err(|_| EngineError::GamePackageInvalidConfig {
        field: format!("litd_editor.runtime_datasets_lowering.{}.{key}", object.id),
        reason: format!("value {value} exceeds u16"),
    })
}

fn u64_prop(object: &EditorObject, key: &str) -> Result<u64, EngineError> {
    match str_prop(object, key) {
        Some(value) => {
            value
                .trim()
                .parse::<u64>()
                .map_err(|error| EngineError::GamePackageInvalidConfig {
                    field: format!("litd_editor.runtime_datasets_lowering.{}.{key}", object.id),
                    reason: format!("expected u64, got {value:?}: {error}"),
                })
        }
        None => lower_error(
            &format!("{}.{key}", object.id),
            format!("required property {key} is absent"),
        ),
    }
}

/// Lower an authored document into the runtime datasets the game host consumes. Fails
/// loud on any missing/invalid authored field -- no defaults papering over bad data.
pub fn lower_runtime_datasets(
    document: &EditorDocument,
) -> Result<RuntimeDatasetsManifest, EngineError> {
    lower_runtime_datasets_with_triggers(document, None)
}

/// #1851: lower the runtime datasets INCLUDING the authored ECA trigger graphs from the
/// persisted trigger state. Trigger unit references resolve against the SAME object-id ->
/// handle assignment the blueprint uses, so a trigger naming an unplaced unit fails loud
/// here (never a dangling runtime reference).
pub fn lower_runtime_datasets_with_triggers(
    document: &EditorDocument,
    trigger_state: Option<&crate::mcp_trigger_tools::McpTriggerRuntimeState>,
) -> Result<RuntimeDatasetsManifest, EngineError> {
    let config = document
        .objects
        .values()
        .find(|object| object.kind == RUNTIME_CONFIG_KIND)
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: "litd_editor.runtime_datasets_lowering.config".to_string(),
            reason: format!("no {RUNTIME_CONFIG_KIND} runtime-config object in the document"),
        })?;

    let tick_rate = u32_prop(config, "runtime.tick_rate")?;
    let worker_cells_per_sec = u32_prop(config, "runtime.worker_cells_per_sec")?;
    let deposit_ticks = u32_prop(config, "runtime.deposit_ticks")?;
    let start_gold = u64_prop(config, "runtime.start_gold")?;
    let start_lumber = u64_prop(config, "runtime.start_lumber")?;
    let food_cap = u32_prop(config, "runtime.upkeep_food_cap")?;

    // Iterate objects in id order for deterministic authoring-order-independence.
    let mut objects: Vec<&EditorObject> = document.objects.values().collect();
    objects.sort_by(|a, b| a.id.cmp(&b.id));

    let mut units: Vec<LoweredUnitEconomy> = Vec::new();
    let mut structures: Vec<LoweredStructureType> = Vec::new();
    let mut nodes: Vec<LoweredResourceNode> = Vec::new();
    let mut authored_units: Vec<AuthoredUnit> = Vec::new();
    let mut owners: Vec<u8> = Vec::new();
    let mut force_hints: BTreeMap<u8, u8> = BTreeMap::new();
    let mut next_unit_handle: u32 = 1;
    // #1851: authored object id -> runtime unit handle, for trigger unit references.
    let mut unit_handles: std::collections::BTreeMap<String, u32> =
        std::collections::BTreeMap::new();

    for object in &objects {
        match object.kind.as_str() {
            UNIT_KIND => {
                let mut state = UnitLoweringState {
                    units: &mut units,
                    structures: &mut structures,
                    authored_units: &mut authored_units,
                    owners: &mut owners,
                    force_hints: &mut force_hints,
                    next_unit_handle: &mut next_unit_handle,
                    unit_handles: &mut unit_handles,
                };
                lower_unit_object(object, &mut state)?;
            }
            RESOURCE_NODE_KIND => nodes.push(lower_resource_node(object)?),
            _ => {}
        }
    }

    if authored_units.is_empty() {
        return lower_error("units", "document authored no runtime units");
    }
    owners.sort_unstable();
    owners.dedup();
    let (slots, force_count) = build_player_slots(&owners, &force_hints)?;

    let ruleset = EconomyRulesetAuthor::new(tick_rate, worker_cells_per_sec, deposit_ticks)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: start_gold,
        })
        .resource(LoweredResourceKind {
            key: "lumber".to_string(),
            display_name: "Lumber".to_string(),
            starting_balance: start_lumber,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: food_cap,
            income_numerator: 1,
            income_denominator: 1,
        });
    let ruleset = units
        .into_iter()
        .fold(ruleset, |author, unit| author.unit(unit));
    let ruleset = nodes
        .into_iter()
        .fold(ruleset, |author, node| author.node(node));
    let ruleset = structures
        .into_iter()
        .fold(ruleset, |author, structure| author.structure(structure))
        .lower()?;

    // Remap authored unit owners to dense slot ids.
    for authored in &mut authored_units {
        authored.owner = dense_owner(&owners, authored.owner)?;
    }

    let map_id = str_prop(config, "runtime.map_id").unwrap_or("mini_melee");
    let mut blueprint = MatchBlueprint::new(map_id, slots, force_count);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    // #1851: an authored custom-trigger victory (fired by the `fire_trigger` action).
    // Both properties or neither -- a half-authored condition fails loud.
    let victory_player = str_prop(config, "runtime.custom_victory_player");
    let victory_trigger = str_prop(config, "runtime.custom_victory_trigger");
    match (victory_player, victory_trigger) {
        (Some(player), Some(_)) => {
            let authored = owner_to_player_id(player)?;
            let dense = dense_owner(&owners, authored).map_err(|_| {
                EngineError::GamePackageInvalidConfig {
                    field: "litd_editor.runtime_datasets_lowering.custom_victory_player"
                        .to_string(),
                    reason: format!("player {player:?} owns no placed units"),
                }
            })?;
            blueprint.author_condition(VictoryCondition::CustomTrigger {
                player: dense,
                trigger: u32_prop(config, "runtime.custom_victory_trigger")?,
            });
        }
        (None, None) => {}
        _ => {
            return lower_error(
                "custom_victory",
                "runtime.custom_victory_player and runtime.custom_victory_trigger \
                 must be authored together",
            );
        }
    }
    for authored in authored_units {
        blueprint.author_unit(authored);
    }
    let lowered_match = blueprint.lower()?;

    let constants = lower_runtime_constants(
        tick_rate,
        u64_prop(config, "runtime.day_length_ticks")?,
        u32_prop(config, "runtime.base_sight_radius_mm")?,
        u16::try_from(u32_prop(config, "runtime.night_sight_penalty_permille")?).map_err(|_| {
            EngineError::GamePackageInvalidConfig {
                field: "litd_editor.runtime_datasets_lowering.night_sight_penalty_permille"
                    .to_string(),
                reason: "value exceeds u16".to_string(),
            }
        })?,
        u32_prop(config, "runtime.fog_cell_size_mm")?,
        u32_prop(config, "runtime.fog_grid_width")?,
        u32_prop(config, "runtime.fog_grid_height")?,
        u64_prop(config, "runtime.match_seed")?,
    )?;

    // #1786: lower any authored AiProfile objects, remapping their owner to the dense slot.
    let ai_profiles = lower_ai_profiles(&objects, &owners)?;

    // #1856: lower authored EditorRegion objects to integer cell rects + handle map.
    let (regions, region_handles) = lower_document_regions(document)?;

    // #1851/#1856: lower the authored ECA trigger graphs (if a trigger state is supplied),
    // resolving unit + region references against the handle maps assigned above.
    let triggers = trigger_state
        .map(|state| {
            crate::trigger_runtime_lowering::lower_trigger_graphs_for_runtime(
                state,
                tick_rate,
                &unit_handles,
                &region_handles,
            )
        })
        .transpose()?;

    RuntimeDatasetsManifest::assemble_with_regions(
        ruleset,
        lowered_match,
        constants,
        ai_profiles,
        triggers,
        regions,
    )
}

struct UnitLoweringState<'a> {
    units: &'a mut Vec<LoweredUnitEconomy>,
    structures: &'a mut Vec<LoweredStructureType>,
    authored_units: &'a mut Vec<AuthoredUnit>,
    owners: &'a mut Vec<u8>,
    force_hints: &'a mut BTreeMap<u8, u8>,
    next_unit_handle: &'a mut u32,
    unit_handles: &'a mut std::collections::BTreeMap<String, u32>,
}

fn lower_unit_object(
    object: &EditorObject,
    state: &mut UnitLoweringState<'_>,
) -> Result<(), EngineError> {
    let role = str_prop(object, "runtime.role").unwrap_or("soldier");
    let is_structure = role == "structure";

    // A unit object WITH an owner is a PLACED starting unit (match blueprint); a unit
    // object WITHOUT an owner is a unit/structure TYPE in the economy catalog. This keeps
    // the trainable-type catalog and the placed victory/vision units cleanly separated.
    if let Some(owner) = str_prop(object, "template.owner_player_id") {
        let owner_id = owner_to_player_id(owner)?;
        state.owners.push(owner_id);
        if let Some(raw_force) = str_prop(object, "template.force_id") {
            let force = parse_force_id(raw_force, &object.id)?;
            note_owner_force(state.force_hints, owner_id, force, &object.id)?;
        }
        let handle = *state.next_unit_handle;
        *state.next_unit_handle += 1;
        state.unit_handles.insert(object.id.clone(), handle);
        // Mobile units get a point-target move capability so orders can be issued through
        // groups (#851); structures get none.
        let capabilities = if is_structure {
            Vec::new()
        } else {
            vec![UnitCapability {
                ability: MOVE_ABILITY,
                target_kind: OrderTargetKind::Point,
            }]
        };
        state.authored_units.push(AuthoredUnit {
            unit: handle,
            owner: owner_id,
            is_structure,
            capabilities,
        });
        return Ok(());
    }

    // #818 econ fields lowered directly from the authored unit-type record.
    let gold_cost = u32_prop(object, "economy.gold_cost")?;
    let lumber_cost = u32_prop_or(object, "economy.lumber_cost", 0)?;
    let food_cost = u32_prop_or(object, "economy.food_cost", 0)?;
    let build_time_secs = u32_prop_or(object, "economy.build_time_secs", 0)?;
    let food_provided = u32_prop_or(object, "economy.food_provided", 0)?;

    state.units.push(LoweredUnitEconomy {
        unit_id: object.id.clone(),
        gold_cost,
        lumber_cost,
        food_cost,
        build_time_secs,
        food_provided,
    });

    if is_structure {
        let trains = str_prop(object, "runtime.trains")
            .map(|value| {
                value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        state.structures.push(LoweredStructureType {
            struct_id: object.id.clone(),
            gold_cost,
            lumber_cost,
            build_time_secs,
            footprint_w: u32_prop(object, "runtime.footprint_w")?,
            footprint_h: u32_prop(object, "runtime.footprint_h")?,
            food_provided,
            trains,
        });
    }
    Ok(())
}

fn lower_resource_node(object: &EditorObject) -> Result<LoweredResourceNode, EngineError> {
    let cell_x = object.transform.translation.x.floor() as i32;
    let cell_z = object.transform.translation.z.floor() as i32;
    Ok(LoweredResourceNode {
        node_id: object.id.clone(),
        resource_key: str_prop(object, "resource.key")
            .unwrap_or("gold")
            .to_string(),
        cell_x,
        cell_z,
        amount_total: u64_prop(object, "resource.amount_total")?,
        yield_per_trip: u64_prop(object, "resource.yield_per_trip")?,
    })
}

/// Attach lowered runtime datasets to a base quick-share package. The datasets are bound
/// into the envelope `manifest_fingerprint` (via their self-derived `datasets_fingerprint`)
/// and the WHOLE package is re-validated at PACKAGE VALIDATION time -- the same check
/// `litd-game` runs on load -- so a bad dataset never reaches disk. Mirrors the #853
/// campaign attach idiom (`build_campaign_litdgame`).
pub fn attach_runtime_datasets(
    package: QuickSharePackage,
    manifest: RuntimeDatasetsManifest,
) -> Result<QuickSharePackage, EngineError> {
    litd_engine::game_runtime::validate_runtime_datasets(&manifest)?;
    // Rebind the envelope fingerprint over the base package fields PLUS the runtime
    // datasets fingerprint (#1843/MINOR-1), so a substituted runtime section is rejected.
    let manifest_fingerprint = compute_package_manifest_fingerprint(
        &package.share_id,
        &package.created_by,
        &package.runtime_id,
        &package.endpoint,
        &package.document_fingerprint,
        &package.play_build_fingerprint,
        &package.assets,
        Some(manifest.datasets_fingerprint.as_str()),
    )?;
    let package = QuickSharePackage {
        runtime: Some(manifest),
        manifest_fingerprint,
        ..package
    };
    validate_quick_share_package(&package)?;
    Ok(package)
}

/// Build a runtime `.litdgame` package from a single authored world document: lower the
/// world through the REAL quick-share build path (real document/play fingerprints, verified
/// assets), lower the runtime datasets from the SAME document, and attach them. This is the
/// production packaging path an editor-built runtime map flows through (the #1843 analogue of
/// `build_campaign_litdgame`), so a packaged `.litdgame` genuinely carries the runtime
/// datasets bound to the authored document.
pub fn build_runtime_litdgame(
    document: &EditorDocument,
    request: &QuickShareBuildRequest,
) -> Result<QuickSharePackage, EngineError> {
    build_runtime_litdgame_with_triggers(document, request, None)
}

/// #1851: the packaging path for a runtime map WITH authored ECA triggers -- the same
/// production quick-share build plus the trigger graph lowered from the persisted
/// trigger state and bound into the datasets/envelope fingerprints.
pub fn build_runtime_litdgame_with_triggers(
    document: &EditorDocument,
    request: &QuickShareBuildRequest,
    trigger_state: Option<&crate::mcp_trigger_tools::McpTriggerRuntimeState>,
) -> Result<QuickSharePackage, EngineError> {
    let base = build_quick_share_package(document, request)?;
    let manifest = lower_runtime_datasets_with_triggers(document, trigger_state)?;
    attach_runtime_datasets(base, manifest)
}
