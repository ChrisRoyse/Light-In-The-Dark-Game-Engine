use litd_engine::game_runtime::{
    MatchRuntimeSession, SESSION_REPLAY_ENCODING_BYTE_INDEX, SESSION_REPLAY_FORMAT_VERSION,
    SESSION_REPLAY_VERSION_BYTE_INDEX, SessionOrder, SessionReplay,
    blueprint::{AuthoredUnit, MatchBlueprint},
    datasets::{RuntimeDatasetsManifest, lower_runtime_constants},
    lowered::{
        EconomyRulesetAuthor, LoweredResourceKind, LoweredResourceNode, LoweredStructureType,
        LoweredUnitEconomy, LoweredUpkeepTier,
    },
    players::{PlayerController, PlayerSlot},
    victory::VictoryCondition,
};
use litd_engine::input::ACTION_CAMERA_FORWARD;
use winit::keyboard::KeyCode;

use super::replay_viewer::{ReplaySpeed, ReplayViewer, ReplayViewerError};
use super::replay_viewer_input::ReplayViewerInput;

#[test]
fn pause_speed_and_completion_preserve_recorded_hash() {
    let manifest = sample_manifest();
    let save = recorded_match(&manifest);
    let expected_tick = save.applied_tick;
    let mut viewer = ReplayViewer::new(&manifest, save).expect("viewer");

    viewer.set_paused(true);
    let paused = viewer.advance_host_frame().expect("paused snapshot");
    assert_eq!(paused.applied_tick, 0);
    assert_eq!(paused.order_cursor, 0);

    viewer.toggle_paused();
    viewer.set_speed(ReplaySpeed::Quad);
    let done = viewer.run_to_completion().expect("complete replay");
    assert!(done.complete);
    assert!(done.final_hash_matches);
    assert_eq!(done.applied_tick, expected_tick);
    assert_eq!(done.order_cursor, done.order_count);
}

#[test]
fn half_speed_batches_one_recorded_tick_every_two_host_frames() {
    let manifest = sample_manifest();
    let save = recorded_match(&manifest);
    let mut viewer = ReplayViewer::new(&manifest, save).expect("viewer");
    viewer.set_speed(ReplaySpeed::Half);

    assert_eq!(
        viewer.advance_host_frame().expect("frame one").applied_tick,
        1
    );
    assert_eq!(
        viewer.advance_host_frame().expect("frame two").applied_tick,
        1
    );
    assert_eq!(
        viewer
            .advance_host_frame()
            .expect("frame three")
            .applied_tick,
        2
    );
}

#[test]
fn perspective_switch_changes_only_presentation_readback() {
    let manifest = sample_manifest();
    let save = recorded_match(&manifest);
    let mut viewer = ReplayViewer::new(&manifest, save).expect("viewer");
    viewer.set_speed(ReplaySpeed::Octuple);
    let done = viewer.run_to_completion().expect("complete replay");
    let hash = done.state_hash;
    let applied_tick = done.applied_tick;
    let order_cursor = done.order_cursor;
    let player_zero_fog = done.perspective_fog;
    let player_zero_cells = viewer
        .perspective_grid()
        .expect("player 0 grid")
        .states()
        .to_vec();

    viewer.set_perspective(1).expect("player 1");
    let player_one = viewer.snapshot().expect("perspective snapshot");
    let player_one_cells = viewer.perspective_grid().expect("player 1 grid").states();
    assert_eq!(player_one.state_hash, hash);
    assert_eq!(player_one.applied_tick, applied_tick);
    assert_eq!(player_one.order_cursor, order_cursor);
    assert_ne!(player_one.perspective_fog, player_zero_fog);
    assert_ne!(player_one_cells, player_zero_cells);

    let error = viewer.set_perspective(2).unwrap_err();
    assert!(matches!(
        error,
        ReplayViewerError::InvalidPerspective {
            requested: 2,
            player_count: 2
        }
    ));
}

#[test]
fn viewer_keymap_controls_preserve_sim_state_and_exclude_camera_actions() {
    let manifest = sample_manifest();
    let save = recorded_match(&manifest);
    let mut viewer = ReplayViewer::new(&manifest, save).expect("viewer");
    let before = viewer.snapshot().expect("before controls");
    let mut input = ReplayViewerInput::new();
    assert_eq!(input.bindings().action_count(), 8);
    assert!(
        input
            .bindings()
            .bindings_for(ACTION_CAMERA_FORWARD)
            .is_empty()
    );

    input.set_key(KeyCode::Space, true);
    assert!(input.apply(&mut viewer).expect("pause"));
    release_key(&mut input, KeyCode::Space);
    input.set_key(KeyCode::Digit4, true);
    assert!(input.apply(&mut viewer).expect("quad speed"));
    release_key(&mut input, KeyCode::Digit4);
    input.set_key(KeyCode::Period, true);
    assert!(input.apply(&mut viewer).expect("next perspective"));

    let after = viewer.snapshot().expect("after controls");
    assert!(after.paused);
    assert_eq!(after.speed, ReplaySpeed::Quad);
    assert_eq!(after.perspective, 1);
    assert_eq!(after.applied_tick, before.applied_tick);
    assert_eq!(after.order_cursor, before.order_cursor);
    assert_eq!(after.state_hash, before.state_hash);
}

#[test]
fn replay_header_refuses_unknown_encoding_and_version_bytes() {
    let manifest = sample_manifest();
    let replay = recorded_match(&manifest);

    let mut unknown_encoding = replay.to_bytes().expect("encoded replay");
    unknown_encoding[SESSION_REPLAY_ENCODING_BYTE_INDEX] = 0xff;
    let encoding_error = SessionReplay::from_bytes(&unknown_encoding).unwrap_err();
    assert!(
        encoding_error
            .to_string()
            .contains("unsupported replay encoding byte 255")
    );

    let mut unknown_version = replay.to_bytes().expect("encoded replay");
    unknown_version[SESSION_REPLAY_VERSION_BYTE_INDEX] =
        SESSION_REPLAY_FORMAT_VERSION.saturating_add(1);
    let version_error = SessionReplay::from_bytes(&unknown_version).unwrap_err();
    assert!(
        version_error
            .to_string()
            .contains("unsupported replay version byte 2")
    );
}

fn release_key(input: &mut ReplayViewerInput, key: KeyCode) {
    input.finish_frame();
    input.set_key(key, false);
    input.finish_frame();
}

fn recorded_match(manifest: &RuntimeDatasetsManifest) -> SessionReplay {
    let mut session = MatchRuntimeSession::from_manifest(manifest).expect("session");
    session
        .apply(SessionOrder::MoveFieldUnit {
            field_id: 10,
            cell_x: 7,
            cell_z: 7,
        })
        .expect("move");
    for _ in 0..9 {
        session.apply(SessionOrder::Tick).expect("tick");
    }
    SessionReplay::capture(manifest, &session).expect("replay")
}

fn sample_manifest() -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(4, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 10_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "none".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: "soldier".to_string(),
            gold_cost: 100,
            lumber_cost: 0,
            food_cost: 1,
            build_time_secs: 1,
            food_provided: 0,
        })
        .node(LoweredResourceNode {
            node_id: "mine".to_string(),
            resource_key: "gold".to_string(),
            cell_x: 5,
            cell_z: 3,
            amount_total: 5_000,
            yield_per_trip: 10,
        })
        .structure(LoweredStructureType {
            struct_id: "hall".to_string(),
            gold_cost: 200,
            lumber_cost: 0,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 20,
            trains: vec!["soldier".to_string()],
        })
        .lower()
        .expect("economy");
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
    let mut blueprint = MatchBlueprint::new("replay", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    for (unit, owner, is_structure) in [(1, 0, true), (2, 1, true), (10, 0, false), (11, 1, false)]
    {
        blueprint.author_unit(AuthoredUnit {
            unit,
            owner,
            is_structure,
            capabilities: Vec::new(),
        });
    }
    let lowered_match = blueprint.lower().expect("match");
    let constants =
        lower_runtime_constants(4, 200, 6_000, 500, 1_000, 16, 16, 83).expect("constants");
    RuntimeDatasetsManifest::assemble(economy, lowered_match, constants).expect("manifest")
}
