#[path = "issue964_themes_fixture.rs"]
#[allow(dead_code)]
mod issue964_themes_fixture;

use issue964_themes_fixture::{happy_catalog, happy_input, happy_rules};
use litd_engine::{
    math::FixedVec3,
    rpg::{
        ASHEN_REACH_HUMAN_IMMORTAL_AVATAR_POLICY_ID, ASHEN_REACH_IMMORTAL_COEMBODIMENT_POLICY_ID,
        HumanImmortalAvatarCommand, HumanImmortalAvatarCommandKind, HumanImmortalAvatarPolicy,
        HumanImmortalAvatarSession, ImmortalActionKind, ImmortalCatalog,
        ImmortalCoembodimentPolicy, ImmortalCoembodimentReadback, ImmortalCoembodimentSession,
        ImmortalDefinition, ImmortalEmbodiedDecision, ImmortalEmbodimentDriver,
        ImmortalEvaluationInput, ImmortalRules, ImmortalSignal, ImmortalSignalKind,
        RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA, RPG_IMMORTAL_COEMBODIMENT_SCHEMA, RPG_IMMORTAL_SCHEMA,
        compose_immortal_coembodiment_session, evaluate_immortal_signal,
    },
};

pub fn ashen_reach_human_immortal_avatar_policy() -> HumanImmortalAvatarPolicy {
    HumanImmortalAvatarPolicy {
        schema: RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA.to_string(),
        policy_id: ASHEN_REACH_HUMAN_IMMORTAL_AVATAR_POLICY_ID.to_string(),
        source_issue: 1045,
        admin_auth_required: true,
        coembodiment_required: true,
        human_override_authoritative: true,
        deterministic_lowering_required: true,
        audit_ledger_required: true,
        revocable_grants_required: true,
        player_visible_single_identity_required: true,
        ai_hot_path_mutation_allowed: false,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
    }
}

pub fn avatar_session() -> HumanImmortalAvatarSession {
    HumanImmortalAvatarSession {
        session_id: "session.immortal.luminara.human_avatar.1".to_string(),
        admin_id: "admin.chris.immortal".to_string(),
        admin_role: "immortal_admin".to_string(),
        immortal_id: "immortal.luminara".to_string(),
        public_identity: "Luminara".to_string(),
        coembodiment_session_id: "session.immortal.luminara.coembody.1".to_string(),
        admin_authenticated: true,
        commands: vec![
            command(
                "cmd.possess.npc",
                HumanImmortalAvatarCommandKind::PossessNpc,
                "npc.ashen_mender",
                Some(42),
                Some(FixedVec3::from_i32s(1, 0, 0)),
                None,
            ),
            command(
                "cmd.possess.boss",
                HumanImmortalAvatarCommandKind::PossessBoss,
                "boss.soot_crowned_judge",
                Some(77),
                Some(FixedVec3::from_i32s(0, 1, 0)),
                None,
            ),
            command(
                "cmd.event.dawn_trial",
                HumanImmortalAvatarCommandKind::RunLiveEvent,
                "scene.ashen_bridge",
                None,
                None,
                Some("event.dawn_trial"),
            ),
            command(
                "cmd.grant.boon",
                HumanImmortalAvatarCommandKind::GrantBoon,
                "char.ashen_mender",
                None,
                None,
                Some("boon.luminara.mercy"),
            ),
            command(
                "cmd.grant.tattoo",
                HumanImmortalAvatarCommandKind::GrantTattoo,
                "char.ashen_mender",
                None,
                None,
                Some("tattoo.dawn.sigils"),
            ),
            command(
                "cmd.grant.title",
                HumanImmortalAvatarCommandKind::GrantTitle,
                "char.ashen_mender",
                None,
                None,
                Some("title.lightbearer"),
            ),
            command(
                "cmd.grant.power",
                HumanImmortalAvatarCommandKind::GrantSuperPower,
                "char.ashen_mender",
                None,
                None,
                Some("power.luminara.flame"),
            ),
            command(
                "cmd.promote.rank",
                HumanImmortalAvatarCommandKind::PromoteRank,
                "char.ashen_mender",
                None,
                None,
                Some("rank.dawn_court.initiate"),
            ),
            command(
                "cmd.ruling.mercy",
                HumanImmortalAvatarCommandKind::MakeRuling,
                "char.ashen_mender",
                None,
                None,
                Some("ruling.dawn_court.mercy"),
            ),
        ],
    }
}

pub fn coembodiment() -> ImmortalCoembodimentReadback {
    compose_immortal_coembodiment_session(
        &ashen_reach_immortal_coembodiment_policy(),
        &ImmortalCoembodimentSession {
            session_id: "session.immortal.luminara.coembody.1".to_string(),
            immortal_id: "immortal.luminara".to_string(),
            public_identity: "Luminara".to_string(),
            embodied_decisions: vec![
                ImmortalEmbodiedDecision {
                    sequence: 1,
                    driver: ImmortalEmbodimentDriver::CalyxAi,
                    human_admin_id: None,
                    override_ai_routine: false,
                    decision: decision(
                        "signal.ai.event",
                        ImmortalSignalKind::EventOpportunity,
                        ImmortalActionKind::RunEvent,
                    ),
                },
                ImmortalEmbodiedDecision {
                    sequence: 2,
                    driver: ImmortalEmbodimentDriver::HumanAdmin,
                    human_admin_id: Some("admin.chris.immortal".to_string()),
                    override_ai_routine: true,
                    decision: decision(
                        "signal.human.prayer",
                        ImmortalSignalKind::Prayer,
                        ImmortalActionKind::GrantEmpowerment,
                    ),
                },
            ],
        },
    )
    .unwrap()
}

fn ashen_reach_immortal_coembodiment_policy() -> ImmortalCoembodimentPolicy {
    ImmortalCoembodimentPolicy {
        schema: RPG_IMMORTAL_COEMBODIMENT_SCHEMA.to_string(),
        policy_id: ASHEN_REACH_IMMORTAL_COEMBODIMENT_POLICY_ID.to_string(),
        source_issue: 1046,
        ai_driver_required: true,
        human_driver_allowed: true,
        human_override_authoritative: true,
        player_visible_single_identity: true,
        deterministic_lowering_required: true,
        ai_hot_path_mutation_allowed: false,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
    }
}

fn command(
    command_id: &str,
    kind: HumanImmortalAvatarCommandKind,
    subject_ref: &str,
    target_body_id: Option<u32>,
    possession_velocity: Option<FixedVec3>,
    artifact_ref: Option<&str>,
) -> HumanImmortalAvatarCommand {
    HumanImmortalAvatarCommand {
        command_id: command_id.to_string(),
        kind,
        subject_ref: subject_ref.to_string(),
        target_body_id,
        possession_velocity,
        artifact_ref: artifact_ref.map(str::to_string),
        summary: format!("audit {command_id}"),
        evidence_refs: vec![format!("evidence://issue1045/{command_id}")],
    }
}

fn decision(
    signal_id: &str,
    kind: ImmortalSignalKind,
    action: ImmortalActionKind,
) -> litd_engine::rpg::ImmortalDecisionReadback {
    evaluate_immortal_signal(
        &ImmortalEvaluationInput {
            catalog: ImmortalCatalog {
                schema: RPG_IMMORTAL_SCHEMA.to_string(),
                catalog_id: "catalog.issue1045.immortals".to_string(),
                immortals: vec![immortal()],
            },
            theme_catalog: happy_catalog(),
            theme_rules: happy_rules(),
            theme_input: happy_input(),
            signal: signal(signal_id, kind, action),
        },
        &ImmortalRules::mud_defaults(),
    )
    .unwrap()
}

fn immortal() -> ImmortalDefinition {
    ImmortalDefinition {
        immortal_id: "immortal.luminara".to_string(),
        display_name: "Luminara".to_string(),
        religion_id: "religion.dawn_court".to_string(),
        allowed_actions: vec![
            ImmortalActionKind::GrantEmpowerment,
            ImmortalActionKind::IssueQuest,
            ImmortalActionKind::RunEvent,
            ImmortalActionKind::EnforceRoleplay,
            ImmortalActionKind::PossessMob,
        ],
        ward_tags: ids(&["compassion", "oathbound", "event", "possession"]),
        lowered_agent_id: 965,
        max_empowerment_tier: 51,
    }
}

fn signal(id: &str, kind: ImmortalSignalKind, action: ImmortalActionKind) -> ImmortalSignal {
    let mut tags = ids(&["compassion", "oathbound"]);
    if action == ImmortalActionKind::RunEvent {
        tags.push("event".to_string());
    }
    if action == ImmortalActionKind::PossessMob {
        tags.push("possession".to_string());
    }
    ImmortalSignal {
        signal_id: id.to_string(),
        immortal_id: "immortal.luminara".to_string(),
        tick: 1045,
        kind,
        requested_action: action,
        strength_milli: 900,
        rp_quality_milli: 850,
        source_cx_id: format!("cx.issue1045.{id}"),
        target_character_id: "char.ashen_mender".to_string(),
        target_body_id: Some(42),
        possession_velocity: Some(FixedVec3::from_i32s(1, 0, 0)),
        tags,
        evidence_refs: vec![format!("evidence://issue1045/{id}")],
    }
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}
