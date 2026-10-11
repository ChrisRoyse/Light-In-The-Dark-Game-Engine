use litd_hash::sha256_hex;

use crate::{
    EngineError,
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
};

use super::{
    HumanImmortalAvatarCommand, HumanImmortalAvatarCommandKind, HumanImmortalAvatarEffect,
    HumanImmortalAvatarPolicy, HumanImmortalAvatarReadback, HumanImmortalAvatarSession,
    ImmortalCoembodimentReadback, RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA,
    human_immortal_avatar_support::{audit_ledger_payload, effect_payload, readback_fingerprint},
    human_immortal_avatar_validate::{
        human_immortal_avatar_policy_hash, validate_coembodiment,
        validate_human_immortal_avatar_policy, validate_human_immortal_avatar_readback,
        validate_session,
    },
    serialize_sha256,
};

pub fn lower_human_immortal_avatar_session(
    policy: &HumanImmortalAvatarPolicy,
    session: &HumanImmortalAvatarSession,
    coembodiment: &ImmortalCoembodimentReadback,
) -> Result<HumanImmortalAvatarReadback, EngineError> {
    validate_human_immortal_avatar_policy(policy)?;
    validate_coembodiment(policy, session, coembodiment)?;
    validate_session(policy, session)?;
    let lowered_effects = session
        .commands
        .iter()
        .enumerate()
        .map(|(index, command)| lower_effect(session, command, index as u64 + 1))
        .collect::<Result<Vec<_>, _>>()?;
    let mut readback = HumanImmortalAvatarReadback {
        schema: RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA.to_string(),
        policy_id: policy.policy_id.clone(),
        source_issue: policy.source_issue,
        session_id: session.session_id.clone(),
        admin_id: session.admin_id.clone(),
        admin_role: session.admin_role.clone(),
        immortal_id: session.immortal_id.clone(),
        public_identity: session.public_identity.clone(),
        coembodiment_session_id: session.coembodiment_session_id.clone(),
        coembodiment_result_sha256: coembodiment.result_sha256.clone(),
        admin_authenticated: session.admin_authenticated,
        player_visible_single_identity: true,
        human_override_authoritative: policy.human_override_authoritative,
        command_count: session.commands.len(),
        possession_order_count: count_kind(&session.commands, |kind| kind.is_possession()),
        npc_possession_count: count_kind(&session.commands, |kind| {
            kind == HumanImmortalAvatarCommandKind::PossessNpc
        }),
        boss_possession_count: count_kind(&session.commands, |kind| {
            kind == HumanImmortalAvatarCommandKind::PossessBoss
        }),
        live_event_count: count_kind(&session.commands, |kind| {
            kind == HumanImmortalAvatarCommandKind::RunLiveEvent
        }),
        grant_count: count_kind(&session.commands, |kind| kind.is_grant()),
        promotion_count: count_kind(&session.commands, |kind| {
            kind == HumanImmortalAvatarCommandKind::PromoteRank
        }),
        ruling_count: count_kind(&session.commands, |kind| {
            kind == HumanImmortalAvatarCommandKind::MakeRuling
        }),
        revocable_grant_count: lowered_effects
            .iter()
            .filter(|effect| effect.kind.is_grant() && effect.revocable)
            .count(),
        lowered_effect_count: lowered_effects.len(),
        deterministic_lowering_required: policy.deterministic_lowering_required,
        audit_ledger_required: policy.audit_ledger_required,
        ai_hot_path_mutation_allowed: false,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
        lowered_effects,
        policy_sha256: human_immortal_avatar_policy_hash(policy)?,
        session_sha256: serialize_sha256(session)?,
        audit_ledger_sha256: String::new(),
        result_sha256: String::new(),
    };
    readback.audit_ledger_sha256 = serialize_sha256(&audit_ledger_payload(&readback))?;
    readback.result_sha256 = serialize_sha256(&readback_fingerprint(&readback))?;
    validate_human_immortal_avatar_readback(policy, &readback)?;
    Ok(readback)
}

fn lower_effect(
    session: &HumanImmortalAvatarSession,
    command: &HumanImmortalAvatarCommand,
    sequence: u64,
) -> Result<HumanImmortalAvatarEffect, EngineError> {
    let effect_id = sha256_hex(
        format!(
            "{}:{}:{}",
            session.session_id,
            command.command_id,
            command.kind.as_str()
        )
        .as_bytes(),
    );
    let sim_order = if command.kind.is_possession() {
        Some(possession_order(command, sequence)?)
    } else {
        None
    };
    let mut effect = HumanImmortalAvatarEffect {
        effect_id,
        command_id: command.command_id.clone(),
        kind: command.kind,
        subject_ref: command.subject_ref.clone(),
        artifact_ref: command.artifact_ref.clone(),
        ledger_entry: ledger_entry(session, command),
        audit_actor_id: session.admin_id.clone(),
        public_identity: session.public_identity.clone(),
        revocable: command.kind.is_grant(),
        sim_order,
        evidence_refs: command.evidence_refs.clone(),
        effect_sha256: String::new(),
    };
    effect.effect_sha256 = serialize_sha256(&effect_payload(&effect))?;
    Ok(effect)
}

fn possession_order(
    command: &HumanImmortalAvatarCommand,
    sequence: u64,
) -> Result<SimOrder, EngineError> {
    let target = command
        .target_body_id
        .ok_or_else(|| EngineError::RpgImmortalInvalidConfig {
            field: "human_immortal_avatar.command.target_body_id".to_string(),
            reason: "possession requires target body id".to_string(),
        })?;
    let velocity =
        command
            .possession_velocity
            .ok_or_else(|| EngineError::RpgImmortalInvalidConfig {
                field: "human_immortal_avatar.command.possession_velocity".to_string(),
                reason: "possession requires deterministic velocity".to_string(),
            })?;
    Ok(SimOrder::new(
        1045_u64
            .checked_add(sequence)
            .ok_or(EngineError::SimTickOverflow)?,
        SimOrderSource::lowered_agent(1045),
        sequence,
        SimBodyId::from_raw(target),
        SimOrderKind::SetVelocity(velocity),
    ))
}

fn count_kind(
    commands: &[HumanImmortalAvatarCommand],
    predicate: impl Fn(HumanImmortalAvatarCommandKind) -> bool,
) -> usize {
    commands
        .iter()
        .filter(|command| predicate(command.kind))
        .count()
}

fn ledger_entry(
    session: &HumanImmortalAvatarSession,
    command: &HumanImmortalAvatarCommand,
) -> String {
    format!(
        "human_immortal_avatar:{}:{}:{}:{}:{}",
        session.session_id,
        session.admin_id,
        command.kind.as_str(),
        command.subject_ref,
        command.artifact_ref.as_deref().unwrap_or("none")
    )
}
