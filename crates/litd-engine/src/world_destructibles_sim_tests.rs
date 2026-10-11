use super::*;
use crate::world_destructibles::tests::field;
use crate::world_destructibles::{DestructibleKind, DestructibleSpec};

fn events() -> Vec<DamageEvent> {
    vec![
        DamageEvent {
            tick: 0,
            sequence: 0,
            target_id: "oak_1".to_string(),
            amount: 30,
        },
        DamageEvent {
            tick: 1,
            sequence: 0,
            target_id: "oak_1".to_string(),
            amount: 30,
        },
        DamageEvent {
            tick: 2,
            sequence: 0,
            target_id: "north_gate".to_string(),
            amount: 5,
        },
    ]
}

#[test]
fn transitions_are_deterministic() {
    let field = field();
    let first = run_destructible_sim(&field, &events(), 7).expect("sim");
    let second = run_destructible_sim(&field, &events(), 7).expect("sim");
    assert_eq!(first, second);
    assert_eq!(first.fingerprint, second.fingerprint);
}

#[test]
fn event_order_independent_of_input_order() {
    let field = field();
    let mut shuffled = events();
    shuffled.reverse();
    let canonical = run_destructible_sim(&field, &events(), 7).expect("sim");
    let reordered = run_destructible_sim(&field, &shuffled, 7).expect("sim");
    assert_eq!(canonical.fingerprint, reordered.fingerprint);
}

#[test]
fn intact_damaged_destroyed_lifecycle() {
    let field = field();
    let report = run_destructible_sim(&field, &events(), 7).expect("sim");
    // oak_1: 50 hp, two 30-damage hits -> damaged then destroyed.
    let oak = &report.transitions[0];
    assert_eq!(oak.state_before, DestructibleState::Intact);
    assert_eq!(oak.state_after, DestructibleState::Damaged);
    assert_eq!(oak.hp_after, 20);
    let oak2 = &report.transitions[1];
    assert_eq!(oak2.state_after, DestructibleState::Destroyed);
    assert_eq!(oak2.hp_after, 0);
    // north_gate: 5 damage - 10 armor = 0 effective -> stays intact.
    let gate = report
        .transitions
        .iter()
        .find(|t| t.target_id == "north_gate")
        .expect("gate event");
    assert_eq!(gate.effective_damage, 0);
    assert_eq!(gate.state_after, DestructibleState::Intact);
    assert_eq!(report.destroyed_count, 1);
}

#[test]
fn edge_overkill_clamps_to_zero_and_stays_destroyed() {
    let field = field();
    let events = vec![
        DamageEvent {
            tick: 0,
            sequence: 0,
            target_id: "oak_1".to_string(),
            amount: 9999,
        },
        DamageEvent {
            tick: 1,
            sequence: 0,
            target_id: "oak_1".to_string(),
            amount: 9999,
        },
    ];
    let report = run_destructible_sim(&field, &events, 7).expect("sim");
    assert_eq!(report.transitions[0].hp_after, 0);
    assert_eq!(report.transitions[0].effective_damage, 50);
    // Second hit on a destroyed object deals nothing.
    assert_eq!(report.transitions[1].effective_damage, 0);
    assert_eq!(
        report.transitions[1].state_after,
        DestructibleState::Destroyed
    );
}

#[test]
fn edge_unknown_target_fails_closed() {
    let field = field();
    let events = vec![DamageEvent {
        tick: 0,
        sequence: 0,
        target_id: "ghost".to_string(),
        amount: 10,
    }];
    let error = run_destructible_sim(&field, &events, 7).unwrap_err();
    assert_eq!(error.code(), "WORLD_DESTRUCTIBLE_SIM_TARGET");
}

#[test]
fn edge_zero_damage_rejected() {
    let field = field();
    let events = vec![DamageEvent {
        tick: 0,
        sequence: 0,
        target_id: "oak_1".to_string(),
        amount: 0,
    }];
    assert_eq!(
        run_destructible_sim(&field, &events, 7).unwrap_err().code(),
        "WORLD_DESTRUCTIBLE_SIM_AMOUNT"
    );
}

#[test]
fn armor_soak_wall_takes_reduced_damage() {
    let field = DestructibleField {
        field_id: "one_wall".to_string(),
        display_name: "One Wall".to_string(),
        destructibles: vec![DestructibleSpec {
            id: "wall".to_string(),
            kind: DestructibleKind::Wall,
            tile_x: 0,
            tile_z: 0,
            yaw_deg: 0,
            max_hp: 100,
            armor: 25,
            model_ref: "models/wall.glb".to_string(),
        }],
    };
    let events = vec![DamageEvent {
        tick: 0,
        sequence: 0,
        target_id: "wall".to_string(),
        amount: 60,
    }];
    let report = run_destructible_sim(&field, &events, 3).expect("sim");
    assert_eq!(report.transitions[0].effective_damage, 35);
    assert_eq!(report.transitions[0].hp_after, 65);
}
