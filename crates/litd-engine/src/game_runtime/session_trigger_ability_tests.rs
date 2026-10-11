//! #804/#851 residual: scripted no-target abilities must use the same typed
//! unit-order surface as human and AI commands.

use crate::game_runtime::groups::OrderTarget;
use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};
use crate::math::{Fixed, FixedVec3};

use super::session_trigger_tests::{manifest_with, on_trigger};

fn ability_graph(ability: u16) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_scripted_ability",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![TriggerAction::OrderUnitAbilityNoTarget {
                unit: 10,
                ability,
                queued: true,
            }],
        )],
    )
    .expect("lower no-target ability graph")
}

fn point_ability_graph(ability: u16) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_scripted_point_ability",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![TriggerAction::OrderUnitAbilityPoint {
                unit: 10,
                ability,
                cell_x: 7,
                cell_z: -3,
                queued: true,
            }],
        )],
    )
    .expect("lower point-target ability graph")
}

#[test]
fn issue804_map_init_ability_writes_exact_order_deterministically() {
    let manifest = manifest_with(Some(ability_graph(2)));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        assert!(session.match_state().order_book(10).unwrap().is_empty());

        session.apply(SessionOrder::Tick).expect("ability tick");
        let orders = session.match_state().order_book(10).unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].ability, 2);
        assert_eq!(orders[0].target, OrderTarget::None);
        assert!(orders[0].queued);
        assert!(
            session.match_state().order_book(11).unwrap().is_empty(),
            "authored unit order must not spill onto a peer unit"
        );
        assert_eq!(
            session.triggers().fire_count("trg_scripted_ability"),
            Some(1)
        );
        session.combined_state_hash()
    };
    assert_eq!(
        run(),
        run(),
        "identical authored ability orders must hash equally"
    );
}

#[test]
fn issue804_point_target_ability_rejected_as_no_target_before_play() {
    let manifest = manifest_with(Some(ability_graph(1)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("point-target ability must reject no-target action");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("ability 1 on unit 10 requires target kind Point"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_missing_ability_rejected_before_play() {
    let manifest = manifest_with(Some(ability_graph(99)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("missing ability must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("unit 10 without ability 99"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_map_init_point_ability_records_exact_order_without_teleport() {
    let manifest = manifest_with(Some(point_ability_graph(1)));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        let before = session
            .field_unit(10)
            .map(|unit| (unit.cell_x, unit.cell_z))
            .expect("field unit 10");

        session
            .apply(SessionOrder::Tick)
            .expect("point ability tick");
        let orders = session.match_state().order_book(10).unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].ability, 1);
        assert_eq!(
            orders[0].target,
            OrderTarget::Point(FixedVec3::new(
                Fixed::from_i32(7),
                Fixed::ZERO,
                Fixed::from_i32(-3),
            ))
        );
        assert!(orders[0].queued);
        assert!(session.match_state().order_book(11).unwrap().is_empty());
        let after = session
            .field_unit(10)
            .map(|unit| (unit.cell_x, unit.cell_z))
            .expect("field unit 10");
        assert_eq!(after, before, "ability order must not teleport the unit");
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "identical point orders must hash equally");
}

#[test]
fn issue804_no_target_ability_rejected_as_point_target_before_play() {
    let manifest = manifest_with(Some(point_ability_graph(2)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("no-target ability must reject point action");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("ability 2 on unit 10 requires target kind None"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_missing_point_ability_rejected_before_play() {
    let manifest = manifest_with(Some(point_ability_graph(99)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("missing ability must reject point action");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("unit 10 without ability 99"),
        "unexpected diagnostic: {diagnostic}"
    );
}
