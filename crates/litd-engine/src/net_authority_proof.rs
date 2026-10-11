use crate::{
    EngineError,
    math::FixedVec3,
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
    sim_replay::SimSnapshot,
};

use super::{
    AuthoritativeSimServer, AuthorityClientBinding, AuthorityServerConfig,
    ClientForgedAuthorityClaim, ClientInput, ClientStateClaim, NET_AUTHORITY_SCHEMA,
    NetAuthorityProof, missing_body_id, net_error,
};

pub fn build_net_authority_proof<const MAX_BODIES: usize, const MAX_ORDERS: usize>(
    snapshot: &SimSnapshot,
    client_id: u32,
    body_id: SimBodyId,
) -> Result<NetAuthorityProof, EngineError> {
    let second_client_id = paired_client_id(client_id);
    let second_body_id = second_body_id(snapshot, body_id)?;
    let sim = snapshot.restore::<MAX_BODIES>()?;
    let mut server = AuthoritativeSimServer::<MAX_BODIES, MAX_ORDERS>::new(
        AuthorityServerConfig::default(),
        sim,
        vec![
            proof_binding(client_id, body_id),
            proof_binding(second_client_id, second_body_id),
        ],
    )?;

    let ack = submit_velocity(
        &mut server,
        client_id,
        1,
        1,
        body_id,
        FixedVec3::from_i32s(64, 0, 0),
    )?;
    submit_velocity(
        &mut server,
        second_client_id,
        1,
        2,
        second_body_id,
        FixedVec3::from_i32s(0, 0, 64),
    )?;
    let tick = server.tick_once()?;

    let before_reject_hash = server.state_hash();
    let before_reject_count = server.rejected_input_count();
    let missing_body = missing_body_id(&server.snapshot());
    let rejected_missing_body = submit_velocity(
        &mut server,
        client_id,
        2,
        3,
        missing_body,
        FixedVec3::from_i32s(1, 0, 0),
    );
    if rejected_missing_body.is_ok()
        || server.rejected_input_count() != before_reject_count.saturating_add(1)
    {
        return Err(net_error(
            "proof.rejected_input",
            "missing-body client input was not rejected by the authoritative server",
        ));
    }
    if server.state_hash() != before_reject_hash {
        return Err(net_error(
            "proof.rejected_state",
            "rejected client input changed authoritative server state",
        ));
    }

    let authoritative = server.snapshot();
    let server_truth = vec![
        server.client_truth(client_id)?,
        server.client_truth(second_client_id)?,
    ];
    let forged_claim = ClientForgedAuthorityClaim {
        client_id,
        forged_position: FixedVec3::from_i32s(999, 0, 999),
        forged_hit_points: 9999,
        forged_item_id: "client_minted_legendary_1318".to_string(),
    };
    let before_claim_hash = server.state_hash();
    let before_claim_truth = server.client_truth(client_id)?;
    let forged_rejection = server.reject_authority_claim(forged_claim.clone())?;
    if server.state_hash() != before_claim_hash
        || server.client_truth(client_id)? != before_claim_truth
    {
        return Err(net_error(
            "proof.forged_claim.state",
            "rejected client authority claim changed server-owned state",
        ));
    }

    let mut forged = authoritative.clone();
    let body = forged
        .bodies
        .iter_mut()
        .find(|body| body.id == body_id)
        .ok_or_else(|| net_error("proof.body", "controlled body missing from snapshot"))?;
    body.position = forged_claim.forged_position;
    let reconciliation = server.reconcile(ClientStateClaim {
        client_id,
        acknowledged_sequence: ack.sequence,
        predicted_snapshot: forged,
    })?;
    if reconciliation.accepted || reconciliation.correction.is_none() {
        return Err(net_error(
            "proof.reconciliation",
            "forged client state was not corrected by the authoritative server",
        ));
    }

    let accepted_hash = server.validate_lockstep_hash(client_id, tick.tick, tick.state_hash)?;
    let rejected_hash =
        server.validate_lockstep_hash(client_id, tick.tick, tick.state_hash ^ 0xA5A5_A5A5_A5A5)?;
    if !accepted_hash.accepted || rejected_hash.accepted {
        return Err(net_error(
            "proof.lockstep",
            "lockstep hash validation did not accept exact hash and reject divergence",
        ));
    }

    let connected_clients = u32::try_from(server.connected_client_count()).map_err(|_| {
        net_error(
            "proof.connected_clients",
            "connected client count exceeded u32 proof capacity",
        )
    })?;
    Ok(NetAuthorityProof {
        schema: NET_AUTHORITY_SCHEMA.to_string(),
        authority_mode: "server_authoritative".to_string(),
        transport_contract: "snapshot_delta_stream".to_string(),
        client_contract: "inputs_only".to_string(),
        connected_clients,
        controlled_client_id: client_id,
        controlled_body_id: body_id.index(),
        server_tick: server.tick(),
        server_state_hash: server.state_hash(),
        accepted_inputs: server.accepted_input_count(),
        rejected_inputs: server.rejected_input_count(),
        corrected_forged_states: server.corrected_state_count(),
        rejected_forged_positions: server.rejected_forged_position_count(),
        rejected_forged_hit_points: server.rejected_forged_hit_points_count(),
        rejected_forged_items: server.rejected_forged_item_count(),
        snapshot_deltas: server.snapshot_delta_count(),
        snapshot_delta_changed_bodies: tick.snapshot_delta.changed_body_count,
        lockstep_hash_checks: server.lockstep_hash_checks(),
        lockstep_divergence_rejections: server.lockstep_divergence_rejections(),
        server_truth,
        forged_claim_rejections: vec![forged_rejection],
    })
}

fn submit_velocity<const MAX_BODIES: usize, const MAX_ORDERS: usize>(
    server: &mut AuthoritativeSimServer<MAX_BODIES, MAX_ORDERS>,
    client_id: u32,
    sequence: u64,
    order_id: u64,
    body_id: SimBodyId,
    velocity: FixedVec3,
) -> Result<super::AuthorityInputAck, EngineError> {
    server.submit_input(ClientInput {
        client_id,
        sequence,
        order: SimOrder::new(
            server.tick().saturating_add(1),
            SimOrderSource::human(client_id),
            order_id,
            body_id,
            SimOrderKind::SetVelocity(velocity),
        ),
    })
}

fn paired_client_id(client_id: u32) -> u32 {
    if client_id == 1 { 2 } else { 1 }
}

fn second_body_id(snapshot: &SimSnapshot, body_id: SimBodyId) -> Result<SimBodyId, EngineError> {
    snapshot
        .bodies
        .iter()
        .map(|body| body.id)
        .find(|candidate| *candidate != body_id)
        .ok_or_else(|| {
            net_error(
                "proof.connected_clients",
                "two-client authority proof requires at least two sim bodies",
            )
        })
}

fn proof_binding(client_id: u32, body_id: SimBodyId) -> AuthorityClientBinding {
    AuthorityClientBinding {
        client_id,
        body_id,
        hit_points: 100 + i64::from(client_id) * 25,
        inventory_items: vec![format!("server_owned_item_client_{client_id}")],
    }
}
