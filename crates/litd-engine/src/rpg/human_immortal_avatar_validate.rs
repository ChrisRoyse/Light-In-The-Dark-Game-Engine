use std::collections::BTreeSet;

use crate::{EngineError, sim_order::SimOrderSourceKind};

use super::{
    HumanImmortalAvatarCommand, HumanImmortalAvatarCommandKind, HumanImmortalAvatarPolicy,
    HumanImmortalAvatarReadback, HumanImmortalAvatarSession, ImmortalCoembodimentReadback,
    RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA, RPG_IMMORTAL_COEMBODIMENT_SCHEMA,
    human_immortal_avatar_support::{
        audit_ledger_payload, effect_payload, invalid, readback_fingerprint, validate_display,
        validate_hash, validate_ref, validate_text, validate_token,
    },
    serialize_sha256,
};

pub fn validate_human_immortal_avatar_policy(
    policy: &HumanImmortalAvatarPolicy,
) -> Result<(), EngineError> {
    if policy.schema != RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA}"),
        );
    }
    validate_token("policy.policy_id", &policy.policy_id)?;
    if policy.source_issue != 1045 {
        return invalid("policy.source_issue", "must point at issue 1045");
    }
    if !policy.admin_auth_required
        || !policy.coembodiment_required
        || !policy.human_override_authoritative
        || !policy.deterministic_lowering_required
        || !policy.audit_ledger_required
        || !policy.revocable_grants_required
        || !policy.player_visible_single_identity_required
        || policy.ai_hot_path_mutation_allowed
    {
        return invalid(
            "policy.flags",
            "human Immortal avatars require auth, co-embodiment, audit, revocable grants, and deterministic lowering",
        );
    }
    if policy.built_in_map_required || policy.grants_map_or_minimap || policy.grants_gps_markers {
        return invalid(
            "policy.map_flags",
            "human Immortal avatars cannot grant map/GPS",
        );
    }
    Ok(())
}

pub fn validate_human_immortal_avatar_readback(
    policy: &HumanImmortalAvatarPolicy,
    readback: &HumanImmortalAvatarReadback,
) -> Result<(), EngineError> {
    validate_human_immortal_avatar_policy(policy)?;
    if readback.schema != RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA {
        return invalid(
            "readback.schema",
            format!("expected {RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA}"),
        );
    }
    if readback.policy_id != policy.policy_id || readback.source_issue != 1045 {
        return invalid("readback.identity", "must match policy and issue 1045");
    }
    if readback.policy_sha256 != human_immortal_avatar_policy_hash(policy)? {
        return invalid("readback.policy_sha256", "does not match active policy");
    }
    if !readback.admin_authenticated
        || !readback.player_visible_single_identity
        || !readback.human_override_authoritative
        || !readback.deterministic_lowering_required
        || !readback.audit_ledger_required
        || readback.ai_hot_path_mutation_allowed
    {
        return invalid("readback.flags", "readback violates avatar policy flags");
    }
    if readback.built_in_map_required
        || readback.grants_map_or_minimap
        || readback.grants_gps_markers
    {
        return invalid("readback.map_flags", "readback cannot grant map/GPS");
    }
    validate_counts(policy, readback)?;
    validate_hash(
        "readback.coembodiment_result_sha256",
        &readback.coembodiment_result_sha256,
    )?;
    validate_hash("readback.session_sha256", &readback.session_sha256)?;
    validate_hash(
        "readback.audit_ledger_sha256",
        &readback.audit_ledger_sha256,
    )?;
    validate_hash("readback.result_sha256", &readback.result_sha256)?;
    if readback.audit_ledger_sha256 != serialize_sha256(&audit_ledger_payload(readback))? {
        return invalid("readback.audit_ledger_sha256", "does not match effects");
    }
    if readback.result_sha256 != serialize_sha256(&readback_fingerprint(readback))? {
        return invalid("readback.result_sha256", "does not match readback");
    }
    Ok(())
}

pub fn human_immortal_avatar_policy_hash(
    policy: &HumanImmortalAvatarPolicy,
) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

pub(super) fn validate_coembodiment(
    policy: &HumanImmortalAvatarPolicy,
    session: &HumanImmortalAvatarSession,
    coembodiment: &ImmortalCoembodimentReadback,
) -> Result<(), EngineError> {
    if !policy.coembodiment_required {
        return invalid("coembodiment.policy", "co-embodiment must be required");
    }
    if coembodiment.schema != RPG_IMMORTAL_COEMBODIMENT_SCHEMA || coembodiment.source_issue != 1046
    {
        return invalid("coembodiment.schema", "must use issue 1046 co-embodiment");
    }
    if session.coembodiment_session_id != coembodiment.session_id
        || session.immortal_id != coembodiment.immortal_id
        || session.public_identity != coembodiment.public_identity
    {
        return invalid(
            "coembodiment.identity",
            "must match avatar session identity",
        );
    }
    if !coembodiment.player_visible_single_identity
        || !coembodiment.human_driver_present
        || !coembodiment.human_override_authoritative
        || coembodiment.handoff_count == 0
        || coembodiment.human_override_count == 0
        || !coembodiment.deterministic_lowering_required
        || coembodiment.ai_hot_path_mutation_allowed
    {
        return invalid(
            "coembodiment.coverage",
            "must prove a human-admin override under one public Immortal identity",
        );
    }
    if coembodiment.built_in_map_required
        || coembodiment.grants_map_or_minimap
        || coembodiment.grants_gps_markers
    {
        return invalid(
            "coembodiment.map_flags",
            "co-embodiment cannot grant map/GPS",
        );
    }
    validate_hash("coembodiment.result_sha256", &coembodiment.result_sha256)
}

pub(super) fn validate_session(
    policy: &HumanImmortalAvatarPolicy,
    session: &HumanImmortalAvatarSession,
) -> Result<(), EngineError> {
    validate_token("session.session_id", &session.session_id)?;
    validate_token("session.admin_id", &session.admin_id)?;
    validate_token("session.admin_role", &session.admin_role)?;
    validate_token("session.immortal_id", &session.immortal_id)?;
    validate_display("session.public_identity", &session.public_identity)?;
    validate_token(
        "session.coembodiment_session_id",
        &session.coembodiment_session_id,
    )?;
    if policy.admin_auth_required && !session.admin_authenticated {
        return invalid("session.admin_authenticated", "admin auth is required");
    }
    if !matches!(session.admin_role.as_str(), "immortal_admin" | "operator") {
        return invalid("session.admin_role", "must be immortal_admin or operator");
    }
    if session.commands.is_empty() || session.commands.len() > 32 {
        return invalid("session.commands", "requires 1..=32 commands");
    }
    let mut seen = BTreeSet::new();
    for command in &session.commands {
        if !seen.insert(command.command_id.as_str()) {
            return invalid("command.command_id", "duplicate command");
        }
        validate_command(command)?;
    }
    Ok(())
}

fn validate_command(command: &HumanImmortalAvatarCommand) -> Result<(), EngineError> {
    validate_token("command.command_id", &command.command_id)?;
    validate_ref("command.subject_ref", &command.subject_ref)?;
    validate_text("command.summary", &command.summary, 180)?;
    if command.evidence_refs.is_empty() || command.evidence_refs.len() > 8 {
        return invalid("command.evidence_refs", "requires 1..=8 evidence refs");
    }
    for evidence in &command.evidence_refs {
        validate_ref("command.evidence_ref", evidence)?;
    }
    match command.kind {
        HumanImmortalAvatarCommandKind::PossessNpc => validate_possession(command, "npc."),
        HumanImmortalAvatarCommandKind::PossessBoss => validate_possession(command, "boss."),
        HumanImmortalAvatarCommandKind::RunLiveEvent => require_artifact(command, "event."),
        HumanImmortalAvatarCommandKind::GrantBoon => require_artifact(command, "boon."),
        HumanImmortalAvatarCommandKind::GrantTattoo => require_artifact(command, "tattoo."),
        HumanImmortalAvatarCommandKind::GrantTitle => require_artifact(command, "title."),
        HumanImmortalAvatarCommandKind::GrantSuperPower => require_artifact(command, "power."),
        HumanImmortalAvatarCommandKind::PromoteRank => require_artifact(command, "rank."),
        HumanImmortalAvatarCommandKind::MakeRuling => require_artifact(command, "ruling."),
    }
}

fn validate_possession(
    command: &HumanImmortalAvatarCommand,
    prefix: &str,
) -> Result<(), EngineError> {
    if !command.subject_ref.starts_with(prefix) {
        return invalid("command.subject_ref", format!("must start with {prefix}"));
    }
    if command.target_body_id.is_none() || command.possession_velocity.is_none() {
        return invalid(
            "command.possession",
            "possession requires target body and deterministic velocity",
        );
    }
    if command.artifact_ref.is_some() {
        return invalid(
            "command.artifact_ref",
            "possession cannot carry artifact ref",
        );
    }
    Ok(())
}

fn require_artifact(command: &HumanImmortalAvatarCommand, prefix: &str) -> Result<(), EngineError> {
    let Some(artifact) = &command.artifact_ref else {
        return invalid("command.artifact_ref", "artifact ref is required");
    };
    validate_ref("command.artifact_ref", artifact)?;
    if !artifact.starts_with(prefix) {
        return invalid("command.artifact_ref", format!("must start with {prefix}"));
    }
    if command.target_body_id.is_some() || command.possession_velocity.is_some() {
        return invalid(
            "command.possession",
            "non-possession commands cannot carry body controls",
        );
    }
    Ok(())
}

fn validate_counts(
    policy: &HumanImmortalAvatarPolicy,
    readback: &HumanImmortalAvatarReadback,
) -> Result<(), EngineError> {
    if readback.command_count == 0
        || readback.command_count != readback.lowered_effect_count
        || readback.lowered_effect_count != readback.lowered_effects.len()
    {
        return invalid("readback.counts", "command and effect counts must match");
    }
    let npc = count_effects(readback, HumanImmortalAvatarCommandKind::PossessNpc);
    let boss = count_effects(readback, HumanImmortalAvatarCommandKind::PossessBoss);
    let live_event = count_effects(readback, HumanImmortalAvatarCommandKind::RunLiveEvent);
    let promotion = count_effects(readback, HumanImmortalAvatarCommandKind::PromoteRank);
    let ruling = count_effects(readback, HumanImmortalAvatarCommandKind::MakeRuling);
    let grants = readback
        .lowered_effects
        .iter()
        .filter(|effect| effect.kind.is_grant())
        .count();
    let revocable_grants = readback
        .lowered_effects
        .iter()
        .filter(|effect| effect.kind.is_grant() && effect.revocable)
        .count();
    if readback.npc_possession_count != npc
        || readback.boss_possession_count != boss
        || readback.live_event_count != live_event
        || readback.grant_count != grants
        || readback.promotion_count != promotion
        || readback.ruling_count != ruling
        || readback.revocable_grant_count != revocable_grants
    {
        return invalid("readback.kind_counts", "summary counts must match effects");
    }
    let possession_orders = readback
        .lowered_effects
        .iter()
        .filter(|effect| effect.sim_order.is_some())
        .count();
    if possession_orders != readback.possession_order_count {
        return invalid("readback.possession_order_count", "does not match effects");
    }
    if policy.revocable_grants_required && readback.grant_count != readback.revocable_grant_count {
        return invalid("readback.revocable_grants", "all grants must be revocable");
    }
    for effect in &readback.lowered_effects {
        if effect.effect_sha256 != serialize_sha256(&effect_payload(effect))? {
            return invalid("effect.effect_sha256", "does not match effect payload");
        }
        if let Some(order) = effect.sim_order
            && (order.source.kind != SimOrderSourceKind::LoweredAgent || order.source.id != 1045)
        {
            return invalid(
                "effect.sim_order.source",
                "avatar possession must lower through issue 1045 agent source",
            );
        }
    }
    Ok(())
}

fn count_effects(
    readback: &HumanImmortalAvatarReadback,
    kind: HumanImmortalAvatarCommandKind,
) -> usize {
    readback
        .lowered_effects
        .iter()
        .filter(|effect| effect.kind == kind)
        .count()
}
