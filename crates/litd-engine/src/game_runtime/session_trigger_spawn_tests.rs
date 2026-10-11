//! #804 trigger-authored field-unit spawning over the real lowered economy catalog.

use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::super::session_order_test_support::{GRID, SPAWN_UNIT, manifest_with};
use super::session_trigger_tests::on_trigger;

fn spawn_graph(unit_key: &str, owner: u8, cell_x: i32, cell_z: i32) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_scripted_spawn",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![TriggerAction::SpawnFieldUnit {
                unit_key: unit_key.to_string(),
                owner,
                cell_x,
                cell_z,
            }],
        )],
    )
    .expect("lower spawn graph")
}

#[test]
fn issue804_map_init_spawn_has_exact_identity_vision_and_hash() {
    let manifest = manifest_with(Some(spawn_graph(SPAWN_UNIT, 0, 8, 8)));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        assert!(!session.fog().is_visible(0, 8, 8));

        session.apply(SessionOrder::Tick).expect("spawn tick");

        let spawned = session.field_unit(12).expect("deterministic first spawn");
        assert_eq!((spawned.id, spawned.owner), (12, 0));
        assert_eq!((spawned.cell_x, spawned.cell_z), (8, 8));
        assert!(session.fog().is_visible(0, 8, 8));
        assert_eq!(session.triggers().fire_count("trg_scripted_spawn"), Some(1));
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "identical spawns must hash equally");
}

#[test]
fn issue804_unknown_spawn_unit_rejected_before_play() {
    let manifest = manifest_with(Some(spawn_graph("missing_unit", 0, 8, 8)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("unknown unit must reject package");
    assert!(
        error
            .to_string()
            .contains("spawn_field_unit references unknown unit \"missing_unit\""),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn issue804_unknown_spawn_owner_rejected_before_play() {
    let manifest = manifest_with(Some(spawn_graph(SPAWN_UNIT, 7, 8, 8)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("unknown owner must reject package");
    assert!(
        error
            .to_string()
            .contains("spawn_field_unit owner 7 is absent from the match roster"),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn issue804_out_of_bounds_spawn_does_not_add_body_or_consume_id() {
    let manifest = manifest_with(Some(spawn_graph(SPAWN_UNIT, 0, GRID as i32, 0)));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    let error = session
        .apply(SessionOrder::Tick)
        .expect_err("out-of-bounds spawn must fail");
    assert!(
        error.to_string().contains("outside the 16x16 fog grid"),
        "unexpected diagnostic: {error}"
    );
    assert!(session.field_unit(12).is_none());

    session
        .apply(SessionOrder::SpawnFieldUnit {
            unit_key: SPAWN_UNIT.to_string(),
            owner: 0,
            cell_x: 7,
            cell_z: 7,
        })
        .expect("valid spawn after rejection");
    assert_eq!(session.field_unit(12).expect("unconsumed first id").id, 12);
}
