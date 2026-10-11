//! #1786: unit coverage for the macro-AI order-source. The in-module authoring builders
//! assemble a manifest with one authored profile; production lowering goes through the
//! editor valve. Proves: the AI plays a full match to victory unaided (workers -> build ->
//! train -> wave -> fog-gated strike), reacts to losing a structure (retreat + resume),
//! handles the supply cap without spinning, and folds its state into the session hash
//! (mutation-falsifiable determinism), plus two-execution replay determinism.

use crate::game_runtime::ai_profile::{AiBuildStep, lower_ai_profile};
use crate::game_runtime::blueprint::{AuthoredUnit, MatchBlueprint};
use crate::game_runtime::datasets::{RuntimeDatasetsManifest, lower_runtime_constants};
use crate::game_runtime::groups::{OrderTargetKind, UnitCapability};
use crate::game_runtime::lowered::{
    EconomyRulesetAuthor, LoweredResourceKind, LoweredResourceNode, LoweredStructureType,
    LoweredUnitEconomy, LoweredUpkeepTier,
};
use crate::game_runtime::players::{PlayerController, PlayerSlot};
use crate::game_runtime::victory::{PlayerResult, VictoryCondition};
use crate::game_runtime::{GameRuntimeError, LoweredAiProfile, MatchRuntimeSession, SessionOrder};

const TICK_RATE: u32 = 4;
const GRID: u32 = 16;

fn move_cap() -> Vec<UnitCapability> {
    vec![UnitCapability {
        ability: 1,
        target_kind: OrderTargetKind::Point,
    }]
}

fn ai_manifest(profile: LoweredAiProfile) -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(TICK_RATE, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 10_000,
        })
        .resource(LoweredResourceKind {
            key: "lumber".to_string(),
            display_name: "Lumber".to_string(),
            starting_balance: 10_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 80,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: "soldier".to_string(),
            gold_cost: 100,
            lumber_cost: 10,
            food_cost: 2,
            build_time_secs: 1,
            food_provided: 0,
        })
        .node(LoweredResourceNode {
            node_id: "gold_mine".to_string(),
            resource_key: "gold".to_string(),
            cell_x: 5,
            cell_z: 3,
            amount_total: 5_000,
            yield_per_trip: 10,
        })
        .structure(LoweredStructureType {
            struct_id: "town_hall".to_string(),
            gold_cost: 200,
            lumber_cost: 0,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 20,
            trains: vec!["soldier".to_string()],
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
    let mut blueprint = MatchBlueprint::new("ai_melee", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    // Enemy (player 0): one structure + one field unit.
    blueprint.author_unit(AuthoredUnit {
        unit: 1,
        owner: 0,
        is_structure: true,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 10,
        owner: 0,
        is_structure: false,
        capabilities: move_cap(),
    });
    // AI (player 1): TWO structures (so losing one triggers retreat, not defeat) + a field
    // unit that forms the wave.
    blueprint.author_unit(AuthoredUnit {
        unit: 2,
        owner: 1,
        is_structure: true,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 3,
        owner: 1,
        is_structure: true,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 11,
        owner: 1,
        is_structure: false,
        capabilities: move_cap(),
    });
    let lowered_match = blueprint.lower().expect("lower match");

    let constants = lower_runtime_constants(TICK_RATE, 200, 6_000, 500, 1_000, GRID, GRID, 0x1786)
        .expect("lower constants");

    RuntimeDatasetsManifest::assemble_with_ai(economy, lowered_match, constants, vec![profile])
        .expect("assemble")
}

fn base_profile(wave_size: u32, retreat_below: u32) -> LoweredAiProfile {
    lower_ai_profile(
        1,
        "Aggressor",
        1,
        "gold_mine",
        2,
        vec![AiBuildStep {
            struct_id: "town_hall".to_string(),
            at_gold: 0,
            cell_x: 10,
            cell_z: 10,
        }],
        "soldier",
        wave_size,
        retreat_below,
        3,
        1000,
    )
    .expect("lower profile")
}

/// A profile with a WIDE decision cadence (every 4 ticks) so the reveal (end of the
/// wave-send tick) and the strike (the NEXT decision tick) sit several ticks apart -- a
/// large, unambiguous fog-gated window for the fog-gate falsification test.
fn fog_gate_profile() -> LoweredAiProfile {
    lower_ai_profile(
        1,
        "FogGate",
        4,
        "gold_mine",
        1,
        vec![AiBuildStep {
            struct_id: "town_hall".to_string(),
            at_gold: 0,
            cell_x: 10,
            cell_z: 10,
        }],
        "soldier",
        1,
        0,
        3,
        1000,
    )
    .expect("lower profile")
}

/// A profile whose sole build step targets an off-grid footprint: the town_hall's 2x2
/// footprint at (15,15) spills to x=16, outside the 16x16 navmesh, so the predicted
/// PlaceStructure is rejected at dispatch.
fn bad_placement_profile() -> LoweredAiProfile {
    lower_ai_profile(
        1,
        "BadPlace",
        4,
        "gold_mine",
        1,
        vec![AiBuildStep {
            struct_id: "town_hall".to_string(),
            at_gold: 0,
            cell_x: 15,
            cell_z: 15,
        }],
        "soldier",
        1,
        0,
        3,
        1000,
    )
    .expect("lower profile")
}

fn kinds(session: &MatchRuntimeSession) -> Vec<&'static str> {
    session.ai().timeline().iter().map(|e| e.kind).collect()
}

#[test]
fn issue1786_ai_plays_full_match_to_victory_unaided() {
    let manifest = ai_manifest(base_profile(2, 0));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    // No human input on the AI side: only Ticks are applied; the AI drives everything.
    // #1860: structure_max_hp 3000 / strike 1000 => 3 multi-hit strikes to kill the
    // enemy base. Tick budget covers train + wave + fog + 3+ strikes.
    let mut victory_tick = None;
    for tick in 1..=80 {
        session.apply(SessionOrder::Tick).expect("tick");
        if session.match_state().result(1).expect("result") == PlayerResult::Victory {
            victory_tick = Some(tick);
            break;
        }
    }
    assert!(
        victory_tick.is_some(),
        "AI must reach victory unaided via multi-hit engagement"
    );
    let strike_count = kinds(&session).iter().filter(|k| **k == "STRIKE").count();
    assert!(
        strike_count >= 3,
        "default engagement needs >=3 strikes to kill a structure, got {strike_count}"
    );
    assert_eq!(
        session.match_state().result(0).expect("p0"),
        PlayerResult::Defeat
    );
    // Economy was actually driven: workers harvested gold, an army body was trained.
    assert!(session.economy().accounts().balance("gold") > 0);
    assert!(session.economy().unit_body_count() >= 1);
    let ks = kinds(&session);
    for expected in ["WORKER", "BUILD", "TRAIN", "WAVE_SENT", "STRIKE"] {
        assert!(
            ks.contains(&expected),
            "timeline missing {expected}: {ks:?}"
        );
    }
}

#[test]
fn issue1786_ai_retreats_when_losing_a_structure_then_resumes() {
    let manifest = ai_manifest(base_profile(2, 2));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    // Run a few ticks so the AI is established, then a scripted strike removes one AI
    // structure -> the AI must retreat (structures 2 -> 1 < threshold 2).
    for _ in 0..3 {
        session.apply(SessionOrder::Tick).expect("tick");
    }
    session
        .apply(SessionOrder::DestroyUnit { unit: 3 })
        .expect("scripted strike on AI structure");
    // AI still not defeated (one structure remains).
    assert_ne!(
        session.match_state().result(1).expect("p1"),
        PlayerResult::Defeat
    );
    // Continue; the AI retreats, regroups, and still finishes the match (#1860 multi-hit).
    let mut victory = false;
    for _ in 0..80 {
        session.apply(SessionOrder::Tick).expect("tick");
        if session.match_state().result(1).expect("p1") == PlayerResult::Victory {
            victory = true;
            break;
        }
    }
    let ks = kinds(&session);
    assert!(ks.contains(&"RETREAT"), "must record a retreat: {ks:?}");
    assert!(ks.contains(&"RESUME"), "must resume after cooldown: {ks:?}");
    assert!(victory, "AI recovers and still wins after retreat");
}

#[test]
fn issue1786_ai_respects_supply_cap_without_spinning() {
    // wave_size 50 soldiers @ food 2 vs a single hall's food 20 -> only 10 trainable. The
    // AI must note the block and stop, never erroring/spinning.
    let manifest = ai_manifest(base_profile(50, 0));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    for _ in 0..30 {
        session
            .apply(SessionOrder::Tick)
            .expect("tick must not error at the supply cap");
    }
    let ks = kinds(&session);
    assert!(
        ks.contains(&"SUPPLY_BLOCKED"),
        "AI must visibly handle the supply block: {ks:?}"
    );
    // 20 food cap / 2 food per soldier = 10 trained, and no more (never spinning past the cap).
    let trained = ks.iter().filter(|k| **k == "TRAIN").count();
    assert_eq!(
        trained, 10,
        "supply cap bounds training to 10, not wave_size 50"
    );
}

#[test]
fn issue1786_two_execution_replay_determinism() {
    let manifest = ai_manifest(base_profile(2, 2));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        for _ in 0..20 {
            session.apply(SessionOrder::Tick).expect("tick");
        }
        session.combined_state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn issue1786_ai_state_folds_into_hash_mutation_falsifiable() {
    // If the AI cursors stop being hashed (the reviewer's mutation), bumping a cursor no
    // longer changes combined_state_hash and this assertion fails.
    let manifest = ai_manifest(base_profile(2, 0));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    for _ in 0..5 {
        session.apply(SessionOrder::Tick).expect("tick");
    }
    let before = session.combined_state_hash();
    assert!(
        session.ai.debug_bump_cursor(),
        "an executor exists to mutate"
    );
    let after = session.combined_state_hash();
    assert_ne!(
        before, after,
        "AI cursor state MUST fold into combined_state_hash"
    );
}

#[test]
fn issue1786_strike_waits_for_fog_reveal_of_enemy_base() {
    // The fog gate is load-bearing: the AI may only strike a structure it can actually SEE.
    // With a 4-tick decision cadence, the enemy base is REVEALED at the end of the wave-send
    // tick (the AI's field unit arrives there) but the earliest strike is the NEXT decision
    // tick -- several ticks later. Replacing `self.fog.is_visible(...)` with `true` (the
    // reviewer's mutation) makes the strike fire on the wave-send tick, BEFORE the base is
    // ever in vision, which every assertion below rejects.
    let manifest = ai_manifest(fog_gate_profile());
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    // Enemy base cell = start_cell(player 0) on a 16x16 grid = (2, 2); see session::start_cell.
    // The AI (player 1) starts far away, so its player-1 fog grid masks (2,2) until its wave
    // physically arrives there.
    const ENEMY_X: i32 = 2;
    const ENEMY_Z: i32 = 2;
    let mut reveal_tick: Option<u64> = None;
    for _ in 0..80 {
        session.apply(SessionOrder::Tick).expect("tick");
        if reveal_tick.is_none() && session.fog().is_visible(1, ENEMY_X, ENEMY_Z) {
            reveal_tick = Some(session.applied_tick());
        }
    }
    let reveal_tick =
        reveal_tick.expect("the AI's wave must eventually put the enemy base in vision");
    let wave_tick = session
        .ai()
        .timeline()
        .iter()
        .find(|e| e.kind == "WAVE_SENT")
        .map(|e| e.tick)
        .expect("a wave must be massed and sent");
    let strikes: Vec<u64> = session
        .ai()
        .timeline()
        .iter()
        .filter(|e| e.kind == "STRIKE")
        .map(|e| e.tick)
        .collect();
    assert!(
        !strikes.is_empty(),
        "the wave must strike once the base is revealed"
    );
    // Fog gate: ZERO strikes may land while the base is fogged (at or before the reveal tick).
    assert!(
        strikes.iter().all(|t| *t > reveal_tick),
        "a strike landed while the enemy base was fogged (revealed at tick {reveal_tick}): {strikes:?}"
    );
    // The strike tick strictly FOLLOWS both the wave-send tick and the reveal tick. Under the
    // fog-gate mutation the strike coincides with the wave-send/reveal tick and this fails.
    assert!(
        strikes[0] > wave_tick,
        "strike {} must follow wave-send {wave_tick}",
        strikes[0]
    );
    assert!(
        strikes[0] > reveal_tick,
        "strike {} must follow the fog reveal {reveal_tick}",
        strikes[0]
    );
}

#[test]
fn issue1786_failed_placement_fails_the_tick_loud() {
    // A predicted PlaceStructure that the economy rejects (off-grid footprint) must fail the
    // whole tick LOUD -- ai_step propagates the dispatch Err rather than swallowing it, so
    // there is no silent instance-id shift or misbound follow-up order.
    let manifest = ai_manifest(bad_placement_profile());
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    for _ in 0..3 {
        session
            .apply(SessionOrder::Tick)
            .expect("pre-decision tick");
    }
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().to_vec();
    let before_ai = session.ai().timeline().to_vec();
    let before_gold = session.economy().accounts().balance("gold");
    let before_time = session.time_of_day().time_permille();
    for _ in 0..2 {
        let error = session
            .apply(SessionOrder::Tick)
            .expect_err("the invalid placement must fail the tick");
        assert!(matches!(error, GameRuntimeError::Sim(_)));
        assert!(error.to_string().contains("navmesh"));
        assert_eq!(session.economy().accounts().balance("gold"), before_gold);
        assert_eq!(session.economy().tick_count(), 3);
        assert_eq!(session.match_state().tick(), 3);
        assert_eq!(session.hud().tick(), 3);
        assert_eq!(session.applied_tick(), 3);
        assert_eq!(session.time_of_day().time_permille(), before_time);
        assert_eq!(session.ai().timeline(), before_ai);
        assert!(session.triggers().timeline().is_empty());
        assert_eq!(session.combined_state_hash(), before_hash);
        assert_eq!(session.order_log(), before_orders);
    }
}

#[path = "session_ai_multi_wave_tests.rs"]
mod multi_wave_tests;
