use crate::{
    EngineError,
    math::FixedVec3,
    net_authority::{
        AuthoritativeSimServer, AuthorityClientBinding, AuthorityServerConfig,
        ClientForgedAuthorityClaim, ClientInput, ClientStateClaim, LockstepHashAction,
        build_net_authority_proof,
    },
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
    sim_replay::SimSnapshot,
};

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn binding(client_id: u32, body_id: SimBodyId) -> AuthorityClientBinding {
    AuthorityClientBinding {
        client_id,
        body_id,
        hit_points: 100 + i64::from(client_id) * 25,
        inventory_items: vec![format!("server_owned_item_client_{client_id}")],
    }
}

fn two_client_server() -> (AuthoritativeSimServer<4, 8>, SimBodyId, SimBodyId) {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(64).unwrap(), 42).unwrap();
    let first = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let second = sim.spawn_body(v(4, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let server = AuthoritativeSimServer::new(
        AuthorityServerConfig::default(),
        sim,
        vec![binding(1, first), binding(2, second)],
    )
    .unwrap();
    (server, first, second)
}

#[test]
fn server_accepts_input_and_corrects_forged_client_state() {
    let (mut server, first, _) = two_client_server();
    let ack = server
        .submit_input(ClientInput {
            client_id: 1,
            sequence: 1,
            order: SimOrder::new(
                1,
                SimOrderSource::human(1),
                1,
                first,
                SimOrderKind::SetVelocity(v(64, 0, 0)),
            ),
        })
        .unwrap();
    let tick = server.tick_once().unwrap();
    let authoritative = server.snapshot();
    let mut forged = authoritative.clone();
    forged.bodies[0].position = v(999, 0, 999);

    let reconciliation = server
        .reconcile(ClientStateClaim {
            client_id: 1,
            acknowledged_sequence: ack.sequence,
            predicted_snapshot: forged,
        })
        .unwrap();
    let correction = reconciliation.correction.unwrap();

    assert_eq!(ack.sequence, 1);
    assert_eq!(tick.orders_applied, 1);
    assert_eq!(tick.snapshot_delta.changed_body_count, 1);
    assert!(!reconciliation.accepted);
    assert_eq!(correction, authoritative);
    assert_eq!(server.corrected_state_count(), 1);
}

#[test]
fn client_cannot_drive_unbound_body_or_replay_sequence() {
    let (mut server, first, second) = two_client_server();
    let before = server.state_hash();
    let bad_target = server
        .submit_input(ClientInput {
            client_id: 1,
            sequence: 1,
            order: SimOrder::new(
                1,
                SimOrderSource::human(1),
                1,
                second,
                SimOrderKind::SetVelocity(v(64, 0, 0)),
            ),
        })
        .unwrap_err()
        .to_string();

    server
        .submit_input(ClientInput {
            client_id: 1,
            sequence: 1,
            order: SimOrder::new(
                1,
                SimOrderSource::human(1),
                1,
                first,
                SimOrderKind::SetVelocity(v(64, 0, 0)),
            ),
        })
        .unwrap();
    let replay = server
        .submit_input(ClientInput {
            client_id: 1,
            sequence: 1,
            order: SimOrder::new(
                2,
                SimOrderSource::human(1),
                2,
                first,
                SimOrderKind::SetVelocity(v(1, 0, 0)),
            ),
        })
        .unwrap_err()
        .to_string();

    assert_eq!(server.state_hash(), before);
    assert!(bad_target.contains("target"));
    assert!(replay.contains("sequence"));
    assert_eq!(server.rejected_input_count(), 2);
}

#[test]
fn snapshot_delta_and_lockstep_hash_validate_authority() {
    let (mut server, first, _) = two_client_server();
    server
        .submit_input(ClientInput {
            client_id: 1,
            sequence: 1,
            order: SimOrder::new(
                1,
                SimOrderSource::human(1),
                1,
                first,
                SimOrderKind::SetVelocity(v(64, 0, 0)),
            ),
        })
        .unwrap();
    let tick = server.tick_once().unwrap();
    let good = server
        .validate_lockstep_hash(1, tick.tick, tick.state_hash)
        .unwrap();
    let bad = server
        .validate_lockstep_hash(1, tick.tick, tick.state_hash ^ 0x55)
        .unwrap();

    assert_eq!(server.snapshot_history_len(), 2);
    assert_eq!(tick.snapshot_delta.from_tick, 0);
    assert_eq!(tick.snapshot_delta.to_tick, 1);
    assert_eq!(tick.snapshot_delta.changed_bodies[0].id, first);
    assert!(good.accepted);
    assert_eq!(good.action, LockstepHashAction::Accept);
    assert!(!bad.accepted);
    assert_eq!(bad.action, LockstepHashAction::RejectDivergentHash);
    assert_eq!(server.lockstep_divergence_rejections(), 1);
}

#[test]
fn config_and_binding_edges_fail_loud() {
    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 7).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let empty = match AuthoritativeSimServer::<1, 1>::new(
        AuthorityServerConfig::default(),
        sim,
        Vec::new(),
    ) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("expected empty binding set to fail"),
    };

    let mut second = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 7).unwrap();
    second
        .spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))
        .unwrap();
    let bad_config = match AuthoritativeSimServer::<1, 1>::new(
        AuthorityServerConfig {
            snapshot_interval_ticks: 0,
            ..AuthorityServerConfig::default()
        },
        second,
        vec![binding(1, body)],
    ) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("expected zero snapshot interval to fail"),
    };

    assert!(empty.contains("bindings"));
    assert!(bad_config.contains("snapshot_interval_ticks"));
}

#[test]
fn proof_covers_inputs_reconciliation_delta_and_lockstep() {
    let mut sim = FixedTickSim::<2>::new(SimConfig::new(64).unwrap(), 9).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    sim.spawn_body(v(4, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let snapshot = SimSnapshot::capture(&sim);

    let proof = build_net_authority_proof::<2, 4>(&snapshot, 1, body).unwrap();

    assert!(proof.proves_server_authority());
    assert_eq!(proof.connected_clients, 2);
    assert_eq!(proof.controlled_client_id, 1);
    assert_eq!(proof.controlled_body_id, body.index());
    assert_eq!(proof.accepted_inputs, 2);
    assert_eq!(proof.rejected_forged_positions, 1);
    assert_eq!(proof.rejected_forged_hit_points, 1);
    assert_eq!(proof.rejected_forged_items, 1);
    assert_eq!(proof.server_truth.len(), 2);
    let rejection = &proof.forged_claim_rejections[0];
    assert!(rejection.position_rejected);
    assert!(rejection.hit_points_rejected);
    assert!(rejection.item_rejected);
    assert!(
        !rejection
            .server_inventory_items
            .contains(&"client_minted_legendary_1318".to_string())
    );
}

#[test]
fn missing_body_binding_reports_sim_config_error() {
    let sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 11).unwrap();
    let error = match AuthoritativeSimServer::<1, 1>::new(
        AuthorityServerConfig::default(),
        sim,
        vec![binding(1, SimBodyId::from_raw(99))],
    ) {
        Err(error) => error,
        Ok(_) => panic!("expected missing body binding to fail"),
    };

    match error {
        EngineError::SimInvalidConfig { field, .. } => {
            assert_eq!(field, "net_authority.bindings.body_id");
        }
        other => panic!("expected SimInvalidConfig, got {other:?}"),
    }
}

#[test]
fn binding_truth_is_server_owned_and_validated() {
    let (server, first, _) = two_client_server();
    let truth = server.client_truth(1).unwrap();

    assert_eq!(truth.client_id, 1);
    assert_eq!(truth.body_id, first.index());
    assert_eq!(truth.hit_points, 125);
    assert_eq!(
        truth.inventory_items,
        vec!["server_owned_item_client_1".to_string()]
    );
}

#[test]
fn authority_claim_path_rejects_matching_truth_without_mutating_server() {
    let (mut server, _, _) = two_client_server();
    let truth = server.client_truth(1).unwrap();
    let before_hash = server.state_hash();
    let rejection = server
        .reject_authority_claim(ClientForgedAuthorityClaim {
            client_id: truth.client_id,
            forged_position: truth.position,
            forged_hit_points: truth.hit_points,
            forged_item_id: truth.inventory_items[0].clone(),
        })
        .unwrap();

    assert!(rejection.position_rejected);
    assert!(rejection.hit_points_rejected);
    assert!(rejection.item_rejected);
    assert_eq!(server.connected_client_count(), 2);
    assert_eq!(server.rejected_authority_claim_count(), 1);
    assert_eq!(server.rejected_forged_position_count(), 1);
    assert_eq!(server.rejected_forged_hit_points_count(), 1);
    assert_eq!(server.rejected_forged_item_count(), 1);
    assert_eq!(server.state_hash(), before_hash);
    assert_eq!(server.client_truth(1).unwrap(), truth);
}

#[test]
fn binding_truth_rejects_invalid_server_owned_inventory() {
    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 15).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let error = match AuthoritativeSimServer::<1, 1>::new(
        AuthorityServerConfig::default(),
        sim,
        vec![AuthorityClientBinding {
            client_id: 1,
            body_id: body,
            hit_points: 125,
            inventory_items: Vec::new(),
        }],
    ) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("expected invalid server-owned inventory to fail"),
    };

    assert!(error.contains("inventory_items"));
}
