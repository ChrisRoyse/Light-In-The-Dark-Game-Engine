use litd_engine::{EngineError, net_authority::NetAuthorityProof};

use super::{
    mcp_project_multiplayer_types::AuthoritativeServerRecord,
    mcp_project_multiplayer_validation::{
        multiplayer_error, validate_agent_count, validate_capacity, validate_endpoint, validate_id,
        validate_probe_ticks, validate_tick_rate,
    },
};

pub(super) fn validate_server_record(
    record: &AuthoritativeServerRecord,
) -> Result<(), EngineError> {
    validate_id("server.server_id", &record.server_id)?;
    validate_id("server.lobby_id", &record.lobby_id)?;
    validate_id("server.content_address", &record.content_address)?;
    validate_id("server.project_id", &record.project_id)?;
    validate_id("server.package_id", &record.package_id)?;
    validate_id("server.host_player_id", &record.host_player_id)?;
    validate_endpoint(&record.endpoint)?;
    validate_capacity(record.max_players)?;
    validate_tick_rate(record.deterministic_tick_rate_hz)?;
    validate_probe_ticks(record.simulation_probe_ticks)?;
    validate_agent_count(record.calyx_agent_count, record.max_players)?;
    let proof = record.net_authority_proof.as_ref().ok_or_else(|| {
        multiplayer_error(
            "server.net_authority_proof",
            "authoritative server record must persist an inputs-only net-authority proof",
        )
    })?;
    validate_net_authority_proof(proof)?;
    if record.fixed_step_nanos == 0
        || record.simulation_final_tick
            != record
                .simulation_start_tick
                .saturating_add(record.simulation_probe_ticks)
        || record.simulation_final_state_hash == 0
        || record.package_sha256.len() != 64
        || record.installed_package_sha256 != record.package_sha256
        || record.package_bytes == 0
        || record.installed_package_bytes != record.package_bytes
        || !record.runtime_lock_verified
        || !record.playable_on_litd_runtime
        || !record.calyx_agent_fleet_enabled
        || !record.calyx_agent_budget_all_tiers_within_budget
        || record.calyx_agent_budget_tier_count == 0
        || record.calyx_agent_budget_max_elapsed_micros == 0
        || record.source_document_fingerprint.trim().is_empty()
        || record.play_build_fingerprint.trim().is_empty()
        || record.started_by.trim().is_empty()
    {
        return Err(multiplayer_error(
            "server",
            "authoritative server runtime, sim, Calyx budget, or package readback is invalid",
        ));
    }
    Ok(())
}

fn validate_net_authority_proof(proof: &NetAuthorityProof) -> Result<(), EngineError> {
    if !proof.proves_server_authority()
        || proof.transport_contract != "snapshot_delta_stream"
        || proof.controlled_client_id == 0
        || proof.server_tick == 0
    {
        return Err(multiplayer_error(
            "server.net_authority_proof",
            "net-authority proof must show inputs-only acceptance/rejection, correction, snapshot delta, and lockstep divergence rejection",
        ));
    }
    Ok(())
}
