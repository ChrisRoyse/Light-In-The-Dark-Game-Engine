//! Shared test-support for the #1855/#1857/#1854/#1859/#1861 session-order unit
//! tests: one deterministic 2-player runtime manifest (economy + blueprint +
//! constants) the effect/fog/cinematic/spawn/weather tests build sessions from.
//!
//! Builders are TEST-ONLY (production data is lowered by the editor valves, #1312/
//! #1783). The manifest carries a custom-trigger victory for player 0 (trigger id 7)
//! and one economy unit ("grunt") so the #1859 spawn tests have a real lowered unit
//! record to validate a `SpawnFieldUnit` against.

use crate::game_runtime::blueprint::{AuthoredUnit, MatchBlueprint};
use crate::game_runtime::datasets::{RuntimeDatasetsManifest, lower_runtime_constants};
use crate::game_runtime::groups::{OrderTargetKind, UnitCapability};
use crate::game_runtime::lowered::{
    EconomyRulesetAuthor, LoweredResourceKind, LoweredUnitEconomy, LoweredUpkeepTier,
};
use crate::game_runtime::players::{PlayerController, PlayerSlot};
use crate::game_runtime::trigger_graph::LoweredTriggerGraph;
use crate::game_runtime::victory::VictoryCondition;

pub(super) const TICK_RATE: u32 = 4;
pub(super) const GRID: u32 = 16;
pub(super) const VICTORY_TRIGGER: u32 = 7;
/// The economy unit id the #1859 spawn tests place; present in the lowered economy.
pub(super) const SPAWN_UNIT: &str = "grunt";
/// Blueprint field-unit ids (mobile, point-move capable) for both owners.
pub(super) const FIELD_UNIT_P0: u32 = 10;
pub(super) const FIELD_UNIT_P1: u32 = 11;

/// Assemble a manifest carrying the given optional lowered trigger graph.
pub(super) fn manifest_with(triggers: Option<LoweredTriggerGraph>) -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(TICK_RATE, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 1_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: SPAWN_UNIT.to_string(),
            gold_cost: 100,
            lumber_cost: 0,
            food_cost: 2,
            build_time_secs: 5,
            food_provided: 0,
        })
        .lower()
        .expect("lower economy");
    let slots = vec![
        PlayerSlot {
            id: 0,
            controller: PlayerController::Human,
            force: 0,
            color: 0,
            handicap_pct: 100,
        },
        PlayerSlot {
            id: 1,
            controller: PlayerController::Computer,
            force: 1,
            color: 1,
            handicap_pct: 100,
        },
    ];
    let mut blueprint = MatchBlueprint::new("effect_melee", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    blueprint.author_condition(VictoryCondition::CustomTrigger {
        player: 0,
        trigger: VICTORY_TRIGGER,
    });
    for (unit, owner, is_structure) in [(1_u32, 0_u8, true), (2, 1, true)] {
        blueprint.author_unit(AuthoredUnit {
            unit,
            owner,
            is_structure,
            capabilities: Vec::new(),
        });
    }
    for (unit, owner) in [(FIELD_UNIT_P0, 0_u8), (FIELD_UNIT_P1, 1)] {
        blueprint.author_unit(AuthoredUnit {
            unit,
            owner,
            is_structure: false,
            capabilities: vec![UnitCapability {
                ability: 1,
                target_kind: OrderTargetKind::Point,
            }],
        });
    }
    let lowered_match = blueprint.lower().expect("lower match");
    let constants = lower_runtime_constants(TICK_RATE, 200, 6_000, 500, 1_000, GRID, GRID, 0x1855)
        .expect("lower constants");
    RuntimeDatasetsManifest::assemble_with_triggers(
        economy,
        lowered_match,
        constants,
        Vec::new(),
        triggers,
    )
    .expect("assemble")
}

/// The default no-trigger manifest for the effect/fog/spawn tests.
pub(super) fn effect_manifest() -> RuntimeDatasetsManifest {
    manifest_with(None)
}
