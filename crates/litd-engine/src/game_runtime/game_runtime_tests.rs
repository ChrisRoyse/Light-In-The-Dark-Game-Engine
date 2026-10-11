//! Unit tests for the match-structure core (#836 players/alliances, #851 groups/
//! selection/orders, #838 victory/defeat). Every test authors a blueprint via the
//! real builder, lowers + verifies it, and drives the deterministic runtime.

use crate::game_runtime::{
    AllianceCell, AllianceState, AuthoredUnit, DEFAULT_STRIKE_DAMAGE_MILLI,
    DEFAULT_STRUCTURE_MAX_HP_MILLI, MatchBlueprint, MatchOrder, MatchOrderKind, MatchOrderQueue,
    MatchState, OrderTargetKind, PlayerController, PlayerResult, PlayerSlot, UnitCapability,
    UnitOrder, VictoryCondition,
};
use crate::math::FixedVec3;

const MOVE: u16 = 1;
const ATTACK: u16 = 2;
const HOLD: u16 = 3;

fn slot(id: u8, controller: PlayerController, force: u8) -> PlayerSlot {
    PlayerSlot {
        id,
        controller,
        force,
        color: id,
        handicap_pct: 100,
    }
}

fn unit(id: u32, owner: u8, is_structure: bool) -> AuthoredUnit {
    AuthoredUnit {
        unit: id,
        owner,
        is_structure,
        capabilities: vec![
            UnitCapability {
                ability: MOVE,
                target_kind: OrderTargetKind::Point,
            },
            UnitCapability {
                ability: ATTACK,
                target_kind: OrderTargetKind::Unit,
            },
            UnitCapability {
                ability: HOLD,
                target_kind: OrderTargetKind::None,
            },
        ],
    }
}

/// A 2-player-plus-neutral melee map: P0 (force 0), P1 (force 1), P2 neutral.
fn two_player_neutral_blueprint() -> MatchBlueprint {
    let slots = vec![
        slot(0, PlayerController::Human, 0),
        slot(1, PlayerController::Computer, 1),
        slot(2, PlayerController::NeutralPassive, 2),
    ];
    let mut bp = MatchBlueprint::new("test_arena", slots, 3);
    bp.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    bp.set_observers_after_defeat(true);
    // P0 units: a structure + a soldier. P1: one structure. P2: a critter.
    bp.author_unit(unit(10, 0, true));
    bp.author_unit(unit(11, 0, false));
    bp.author_unit(unit(20, 1, true));
    bp.author_unit(unit(30, 2, false));
    bp
}

fn lowered_state() -> MatchState {
    let lowered = two_player_neutral_blueprint().lower().expect("lower");
    MatchState::from_lowered(&lowered).expect("materialize")
}

#[test]
fn alliance_matrix_defaults_and_runtime_change() {
    let mut state = lowered_state();
    // #836: default cross-force players are enemies; neutral-passive is neutral.
    assert_eq!(
        state.roster().alliance(0, 1).unwrap().state,
        AllianceState::Enemy
    );
    assert_eq!(
        state.roster().alliance(0, 2).unwrap().state,
        AllianceState::Neutral
    );
    assert!(state.roster().are_enemies(0, 1).unwrap());
    // Runtime alliance-change order flips P0->P1 to ally.
    state
        .set_alliance(0, 1, AllianceState::Ally, true, false)
        .unwrap();
    assert_eq!(
        state.roster().alliance(0, 1).unwrap().state,
        AllianceState::Ally
    );
    assert!(!state.roster().are_enemies(0, 1).unwrap());
}

#[test]
fn locked_alliance_fails_loud() {
    let slots = vec![
        slot(0, PlayerController::Human, 0),
        slot(1, PlayerController::Computer, 1),
    ];
    let mut bp = MatchBlueprint::new("locked", slots, 2);
    bp.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    bp.author_unit(unit(1, 0, true));
    bp.author_unit(unit(2, 1, true));
    bp.author_alliance(
        0,
        1,
        AllianceCell {
            state: AllianceState::Enemy,
            shared_vision: false,
            shared_control: false,
            locked: true,
        },
    );
    let lowered = bp.lower().unwrap();
    let mut state = MatchState::from_lowered(&lowered).unwrap();
    let err = state
        .set_alliance(0, 1, AllianceState::Ally, false, false)
        .unwrap_err();
    assert_eq!(err.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");
}

#[test]
fn groups_filter_and_selection_deterministic() {
    let mut state = lowered_state();
    // #851: build P0's group with both its units; filter to structures only.
    state.add_to_group(0, 10).unwrap();
    state.add_to_group(0, 11).unwrap();
    let group = state.player_groups().group(0).unwrap();
    assert_eq!(group.members(), &[10, 11]);
    let structures_only = group.filter(|u| u == 10);
    assert_eq!(structures_only.members(), &[10]);
    // Selection is order-independent: [11,10] selects the same set as [10,11].
    state.select(0, &[11, 10]).unwrap();
    assert_eq!(state.selection().selection(0).unwrap().members(), &[10, 11]);
}

#[test]
fn order_validation_against_capabilities() {
    let mut state = lowered_state();
    // A point order for the move ability is valid.
    let ok = UnitOrder::point(MOVE, FixedVec3::from_i32s(5, 0, 0), false);
    assert_eq!(state.issue_order(0, &[11], ok).unwrap(), 1);
    // Wrong target shape (unit-target for a point ability) fails loud.
    let wrong = UnitOrder::unit_target(MOVE, 20, false);
    let err = state.issue_order(0, &[11], wrong).unwrap_err();
    assert_eq!(err.code(), "LITD-SIM-INVALID-CONFIG");
    // Unknown ability fails loud.
    let missing = UnitOrder::no_target(99, false);
    assert!(state.issue_order(0, &[11], missing).is_err());
    // Issuing an order for a unit you do not own fails loud.
    assert!(state.issue_order(0, &[20], ok).is_err());
}

#[test]
fn shift_queue_semantics() {
    let mut state = lowered_state();
    let a = UnitOrder::point(MOVE, FixedVec3::from_i32s(1, 0, 0), false);
    let b = UnitOrder::point(MOVE, FixedVec3::from_i32s(2, 0, 0), true);
    let c = UnitOrder::no_target(HOLD, false);
    state.issue_order(0, &[11], a).unwrap();
    state.issue_order(0, &[11], b).unwrap();
    assert_eq!(state.order_book(11).unwrap().len(), 2);
    // A non-queued order clears the shift-queue.
    state.issue_order(0, &[11], c).unwrap();
    assert_eq!(state.order_book(11).unwrap(), &[c]);
}

#[test]
fn victory_flips_on_last_enemy_structure_destroyed() {
    let mut state = lowered_state();
    assert_eq!(state.result(0).unwrap(), PlayerResult::Playing);
    assert_eq!(state.result(1).unwrap(), PlayerResult::Playing);
    // Destroy P1's only structure -> P1 defeated, P0 victorious (no enemy left).
    let events = state.destroy_unit(20).unwrap();
    assert_eq!(state.result(1).unwrap(), PlayerResult::Defeat);
    assert_eq!(state.result(0).unwrap(), PlayerResult::Victory);
    // Neutral-passive P2 is not an enemy, so it never blocks victory.
    assert_eq!(state.result(2).unwrap(), PlayerResult::Playing);
    assert!(
        events
            .iter()
            .any(|e| e.player == 1 && e.result == PlayerResult::Defeat)
    );
    assert!(
        events
            .iter()
            .any(|e| e.player == 0 && e.result == PlayerResult::Victory)
    );
    // observers-after-defeat: defeated P1 becomes an observer.
    assert!(state.is_observer(1).unwrap());
}

#[test]
fn custom_trigger_condition_grants_victory() {
    let slots = vec![
        slot(0, PlayerController::Human, 0),
        slot(1, PlayerController::Computer, 1),
    ];
    let mut bp = MatchBlueprint::new("trigger_map", slots, 2);
    bp.author_condition(VictoryCondition::CustomTrigger {
        player: 1,
        trigger: 42,
    });
    bp.author_unit(unit(1, 0, true));
    bp.author_unit(unit(2, 1, true));
    let lowered = bp.lower().unwrap();
    let mut state = MatchState::from_lowered(&lowered).unwrap();
    assert_eq!(state.result(1).unwrap(), PlayerResult::Playing);
    let events = state.fire_trigger(42).unwrap();
    assert_eq!(state.result(1).unwrap(), PlayerResult::Victory);
    assert_eq!(events.len(), 1);
}

#[test]
fn replay_determinism_identical_hash() {
    let orders = || {
        vec![
            MatchOrder::new(
                1,
                0,
                0,
                MatchOrderKind::SetAlliance {
                    from: 0,
                    to: 1,
                    state: AllianceState::Ally,
                    shared_vision: true,
                    shared_control: false,
                },
            ),
            MatchOrder::new(
                1,
                0,
                1,
                MatchOrderKind::Select {
                    units: vec![11, 10],
                },
            ),
            MatchOrder::new(
                2,
                0,
                0,
                MatchOrderKind::IssueOrder {
                    units: vec![11],
                    order: UnitOrder::point(MOVE, FixedVec3::from_i32s(3, 0, 0), false),
                },
            ),
            MatchOrder::new(3, 0, 0, MatchOrderKind::DestroyUnit { unit: 20 }),
        ]
    };
    let run = || {
        let lowered = two_player_neutral_blueprint().lower().unwrap();
        let mut state = MatchState::from_lowered(&lowered).unwrap();
        let mut queue = MatchOrderQueue::new();
        for order in orders() {
            queue.enqueue(order).unwrap();
        }
        queue.run_ticks(&mut state, 4).unwrap()
    };
    assert_eq!(run(), run());
}

#[test]
fn sabotaged_condition_fails_loud() {
    // A custom condition referencing a non-existent player must fail at authoring.
    let slots = vec![
        slot(0, PlayerController::Human, 0),
        slot(1, PlayerController::Computer, 1),
    ];
    let mut bp = MatchBlueprint::new("bad", slots, 2);
    bp.author_condition(VictoryCondition::CustomTrigger {
        player: 9,
        trigger: 1,
    });
    bp.author_unit(unit(1, 0, true));
    let lowered = bp.lower().unwrap();
    let err = MatchState::from_lowered(&lowered).unwrap_err();
    assert_eq!(err.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");
}

#[test]
fn tampered_lowered_artifact_fails_loud() {
    let mut lowered = two_player_neutral_blueprint().lower().unwrap();
    // Corrupt the canonical bytes without updating the fingerprint.
    lowered.canonical_ron.push(' ');
    let err = lowered.verify().unwrap_err();
    assert_eq!(err.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");
}

#[test]
fn structure_double_destroy_fails_loud() {
    let mut state = lowered_state();
    state.destroy_unit(20).unwrap();
    let err = state.destroy_unit(20).unwrap_err();
    assert_eq!(err.code(), "LITD-SIM-INVALID-CONFIG");
}

/// #1860: multi-hit engagement — structure survives partial damage, dies on third
/// default strike (3000 HP / 1000 damage).
#[test]
fn issue1860_apply_damage_kills_after_multiple_hits() {
    let mut state = lowered_state();
    let max = state.max_hp_milli(20).unwrap();
    assert_eq!(max, DEFAULT_STRUCTURE_MAX_HP_MILLI);
    assert_eq!(state.hp_milli(20).unwrap(), max);
    assert!(state.is_alive(20).unwrap());

    let r1 = state
        .apply_damage(20, DEFAULT_STRIKE_DAMAGE_MILLI)
        .expect("hit 1");
    assert!(!r1.destroyed);
    assert_eq!(r1.hp_after, max - DEFAULT_STRIKE_DAMAGE_MILLI);
    assert!(state.is_alive(20).unwrap());
    assert_eq!(state.hp_milli(20).unwrap(), r1.hp_after);

    let r2 = state
        .apply_damage(20, DEFAULT_STRIKE_DAMAGE_MILLI)
        .expect("hit 2");
    assert!(!r2.destroyed);
    assert!(state.is_alive(20).unwrap());

    let r3 = state
        .apply_damage(20, DEFAULT_STRIKE_DAMAGE_MILLI)
        .expect("hit 3 kill");
    assert!(r3.destroyed);
    assert_eq!(r3.hp_after, 0);
    assert!(!state.is_alive(20).unwrap());
    assert_eq!(state.hp_milli(20).unwrap(), 0);
    // Victory cascade: P1's only structure gone => P1 defeat, P0 victory.
    assert_eq!(state.result(1).unwrap(), PlayerResult::Defeat);
    assert_eq!(state.result(0).unwrap(), PlayerResult::Victory);
}

#[test]
fn issue1860_zero_damage_fails_loud() {
    let mut state = lowered_state();
    let err = state.apply_damage(20, 0).unwrap_err();
    assert_eq!(err.code(), "LITD-SIM-INVALID-CONFIG");
    assert!(state.is_alive(20).unwrap());
}

#[test]
fn issue1860_damage_dead_target_fails_loud() {
    let mut state = lowered_state();
    state.destroy_unit(20).unwrap();
    let err = state.apply_damage(20, 100).unwrap_err();
    assert_eq!(err.code(), "LITD-SIM-INVALID-CONFIG");
}

#[test]
fn issue1860_hp_folds_into_state_hash() {
    let mut a = lowered_state();
    let mut b = lowered_state();
    assert_eq!(a.state_hash(), b.state_hash());
    a.apply_damage(20, 500).unwrap();
    assert_ne!(
        a.state_hash(),
        b.state_hash(),
        "HP change must fold into MatchState::state_hash"
    );
    b.apply_damage(20, 500).unwrap();
    assert_eq!(a.state_hash(), b.state_hash());
}
