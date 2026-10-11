use litd_engine::rpg::{
    ASHEN_REACH_INFERNAL_ACCESS_POLICY_ID, InfernalAccessCostKind, InfernalAccessPolicy,
    InfernalCircleId, InfernalEntryPath, InfernalEntryRule, InfernalEscapeKind,
    RPG_INFERNAL_ACCESS_SCHEMA,
};

pub(crate) fn ashen_reach_infernal_access_policy() -> InfernalAccessPolicy {
    InfernalAccessPolicy {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_INFERNAL_ACCESS_POLICY_ID.to_string(),
        source_issue: 1066,
        destination_circle: InfernalCircleId::Threshold,
        no_recall_inside: true,
        required_gate_id: "gate.cinders".to_string(),
        required_long_descent_ref: "quest.long_descent.sunken_vaults".to_string(),
        required_earned_cindermark_ref: "sigil.cindermark.earned".to_string(),
        required_hollow_pact_faction_id: "faction.hollow_pact".to_string(),
        required_hollow_pact_power_id: "power.hollow_pact.infernal_seam".to_string(),
        max_access_history: 64,
        entry_rules: vec![
            rule(
                InfernalEntryPath::LongDescentEarnedCindermark,
                InfernalAccessCostKind::CindermarkTrialScar,
            ),
            rule(
                InfernalEntryPath::LongDescentDevilBrandedCindermark,
                InfernalAccessCostKind::DevilBrandDebt,
            ),
            rule(
                InfernalEntryPath::LesserDevilPact,
                InfernalAccessCostKind::PactDebt,
            ),
            rule(
                InfernalEntryPath::HollowPactSeam,
                InfernalAccessCostKind::FactionSeamDebt,
            ),
            rule(
                InfernalEntryPath::DraggedUnderAccident,
                InfernalAccessCostKind::DraggedUnderTrauma,
            ),
        ],
        denied_escape_kinds: vec![
            InfernalEscapeKind::Recall,
            InfernalEscapeKind::Teleport,
            InfernalEscapeKind::GatePortal,
            InfernalEscapeKind::HearthRite,
            InfernalEscapeKind::ExtractionRitual,
        ],
    }
}

fn rule(path: InfernalEntryPath, cost_kind: InfernalAccessCostKind) -> InfernalEntryRule {
    InfernalEntryRule { path, cost_kind }
}
