//! #804 trigger-authored placement must use the real deterministic economy world.

use crate::game_runtime::datasets::RuntimeDatasetsManifest;
use crate::game_runtime::lowered::{
    EconomyRulesetAuthor, LoweredResourceKind, LoweredStructureType, LoweredUpkeepTier,
};
use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_tests::{manifest_with, on_trigger};

fn structure_graph(structure: &str, cell_x: i32, cell_z: i32) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_scripted_structure",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![TriggerAction::PlaceStructure {
                structure: structure.to_string(),
                cell_x,
                cell_z,
            }],
        )],
    )
    .expect("lower structure graph")
}

fn economy_manifest(
    graph: LoweredTriggerGraph,
    starting_gold: u64,
    starting_lumber: u64,
) -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(4, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: starting_gold,
        })
        .resource(LoweredResourceKind {
            key: "lumber".to_string(),
            display_name: "Lumber".to_string(),
            starting_balance: starting_lumber,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .structure(LoweredStructureType {
            struct_id: "watch_tower".to_string(),
            gold_cost: 200,
            lumber_cost: 50,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 10,
            trains: Vec::new(),
        })
        .lower()
        .expect("lower economy");
    let mut manifest = manifest_with(Some(graph));
    manifest.economy = economy;
    manifest.datasets_fingerprint = manifest.compute_fingerprint();
    manifest
}

#[test]
fn issue804_map_init_places_structure_with_exact_cost_and_hash() {
    let manifest = economy_manifest(structure_graph("watch_tower", 2, 3), 1_000, 1_000);
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

        session.apply(SessionOrder::Tick).expect("placement tick");

        let structure = session.economy().structure(0).expect("placed structure");
        assert_eq!(structure.instance_id, 0);
        assert_eq!(structure.struct_id, "watch_tower");
        assert_eq!((structure.origin.x, structure.origin.z), (2, 3));
        assert_eq!(structure.build_ticks_remaining, 4);
        assert_eq!(session.economy().accounts().balance("gold"), 800);
        assert_eq!(session.economy().accounts().balance("lumber"), 950);
        assert_eq!(
            session.triggers().fire_count("trg_scripted_structure"),
            Some(1)
        );
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "identical placement must hash equally");
}

#[test]
fn issue804_unknown_structure_rejected_before_play() {
    let manifest = economy_manifest(structure_graph("missing_tower", 2, 3), 1_000, 1_000);
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("unknown structure must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("place_structure references unknown structure \"missing_tower\""),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_off_grid_placement_fails_without_economy_mutation() {
    let manifest = economy_manifest(structure_graph("watch_tower", 15, 15), 1_000, 1_000);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

    let error = session
        .apply(SessionOrder::Tick)
        .expect_err("off-grid footprint must fail loud");
    assert!(
        error.to_string().contains("footprint"),
        "unexpected diagnostic: {error}"
    );
    assert!(session.economy().structure(0).is_none());
    assert_eq!(session.economy().accounts().balance("gold"), 1_000);
    assert_eq!(session.economy().accounts().balance("lumber"), 1_000);
}

#[test]
fn issue804_insufficient_gold_fails_without_economy_mutation() {
    let manifest = economy_manifest(structure_graph("watch_tower", 2, 3), 100, 1_000);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

    let error = session
        .apply(SessionOrder::Tick)
        .expect_err("insufficient gold must fail loud");
    assert!(
        error
            .to_string()
            .contains("cannot spend 200 gold; have 100"),
        "unexpected diagnostic: {error}"
    );
    assert!(session.economy().structure(0).is_none());
    assert_eq!(session.economy().accounts().balance("gold"), 100);
    assert_eq!(session.economy().accounts().balance("lumber"), 1_000);
}

#[test]
fn issue804_insufficient_lumber_rejects_without_any_session_mutation() {
    let manifest = economy_manifest(structure_graph("watch_tower", 2, 3), 1_000, 25);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().len();

    let error = session
        .apply(SessionOrder::PlaceStructure {
            struct_id: "watch_tower".to_string(),
            x: 2,
            z: 3,
        })
        .expect_err("insufficient lumber must fail loud");

    assert!(
        error
            .to_string()
            .contains("cannot spend 50 lumber; have 25"),
        "unexpected diagnostic: {error}"
    );
    assert!(session.economy().structure(0).is_none());
    assert_eq!(session.economy().accounts().balance("gold"), 1_000);
    assert_eq!(session.economy().accounts().balance("lumber"), 25);
    assert_eq!(session.combined_state_hash(), before_hash);
    assert_eq!(session.order_log().len(), before_orders);
}
