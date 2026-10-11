// FSV edge-case fixtures: each helper threads the full set of Infernal Deeps policy structs
// positionally into a derive/validate call. A struct-of-args would only re-spell those same
// domain structs, so the positional form is clearer here. Scoped to this fixtures module.
#![allow(clippy::too_many_arguments)]

use litd_engine::{
    EngineError,
    rpg::{
        DeriveInfernalDeepsPackageArgs, InfernalAccessPolicy, InfernalAccessState,
        InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalDeepsPackagePolicy,
        InfernalEscapeAttempt, InfernalEscapeKind, InfernalRelicVaultPolicy, InfernalZonePolicy,
        RPG_INFERNAL_ACCESS_SCHEMA, UndeathPolicy, WitheringRaidPathPolicy,
        derive_infernal_deeps_package, resolve_infernal_escape_attempt,
        validate_infernal_deeps_package_policy,
    },
};
use serde_json::{Value, json};

pub fn infernal_deeps_edges(
    package: &InfernalDeepsPackagePolicy,
    access: &InfernalAccessPolicy,
    attrition: &InfernalAttritionPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    relics: &InfernalRelicVaultPolicy,
    withering: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> Value {
    json!({
        "package_map_rejected": package_map_edge(package),
        "missing_child_issue_rejected": missing_child_issue_edge(package),
        "access_recall_drift_rejected": access_recall_edge(package, access, attrition, zones, bosses, relics, withering, undeath),
        "zone_count_drift_rejected": zone_count_edge(package, access, attrition, zones, bosses, relics, withering, undeath),
        "boss_hot_path_drift_rejected": boss_hot_path_edge(package, access, attrition, zones, bosses, relics, withering, undeath),
        "relic_map_grant_rejected": relic_map_edge(package, access, attrition, zones, bosses, relics, withering, undeath),
        "withering_ui_reveal_rejected": withering_ui_edge(package, access, attrition, zones, bosses, relics, withering, undeath),
        "outside_escape_rejected": outside_escape_edge(access),
    })
}

fn package_map_edge(package: &InfernalDeepsPackagePolicy) -> Value {
    let mut invalid = package.clone();
    invalid.grants_map_or_minimap = true;
    let before = value_hash(&invalid);
    let result = validate_infernal_deeps_package_policy(&invalid);
    edge_from_result(result, before, value_hash(&invalid))
}

fn missing_child_issue_edge(package: &InfernalDeepsPackagePolicy) -> Value {
    let mut invalid = package.clone();
    invalid.required_child_issues.pop();
    let before = value_hash(&invalid);
    let result = validate_infernal_deeps_package_policy(&invalid);
    edge_from_result(result, before, value_hash(&invalid))
}

fn access_recall_edge(
    package: &InfernalDeepsPackagePolicy,
    access: &InfernalAccessPolicy,
    attrition: &InfernalAttritionPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    relics: &InfernalRelicVaultPolicy,
    withering: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> Value {
    let mut invalid_access = access.clone();
    invalid_access.no_recall_inside = false;
    edge_from_package(
        package,
        &invalid_access,
        attrition,
        zones,
        bosses,
        relics,
        withering,
        undeath,
    )
}

fn zone_count_edge(
    package: &InfernalDeepsPackagePolicy,
    access: &InfernalAccessPolicy,
    attrition: &InfernalAttritionPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    relics: &InfernalRelicVaultPolicy,
    withering: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> Value {
    let mut invalid_zones = zones.clone();
    invalid_zones.zones.pop();
    edge_from_package(
        package,
        access,
        attrition,
        &invalid_zones,
        bosses,
        relics,
        withering,
        undeath,
    )
}

fn boss_hot_path_edge(
    package: &InfernalDeepsPackagePolicy,
    access: &InfernalAccessPolicy,
    attrition: &InfernalAttritionPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    relics: &InfernalRelicVaultPolicy,
    withering: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> Value {
    let mut invalid_bosses = bosses.clone();
    invalid_bosses.calyx_live_query_allowed_in_hot_path = true;
    edge_from_package(
        package,
        access,
        attrition,
        zones,
        &invalid_bosses,
        relics,
        withering,
        undeath,
    )
}

fn relic_map_edge(
    package: &InfernalDeepsPackagePolicy,
    access: &InfernalAccessPolicy,
    attrition: &InfernalAttritionPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    relics: &InfernalRelicVaultPolicy,
    withering: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> Value {
    let mut invalid_relics = relics.clone();
    invalid_relics.item_set[0].grants_map_or_minimap = true;
    edge_from_package(
        package,
        access,
        attrition,
        zones,
        bosses,
        &invalid_relics,
        withering,
        undeath,
    )
}

fn withering_ui_edge(
    package: &InfernalDeepsPackagePolicy,
    access: &InfernalAccessPolicy,
    attrition: &InfernalAttritionPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    relics: &InfernalRelicVaultPolicy,
    withering: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> Value {
    let mut invalid_withering = withering.clone();
    invalid_withering.questline.revealed_by_default_ui = true;
    edge_from_package(
        package,
        access,
        attrition,
        zones,
        bosses,
        relics,
        &invalid_withering,
        undeath,
    )
}

fn outside_escape_edge(access: &InfernalAccessPolicy) -> Value {
    let state = outside_state();
    let attempt = escape_attempt(InfernalEscapeKind::Recall);
    let before = value_hash(&json!({"state": &state, "access": access, "attempt": &attempt}));
    let result = resolve_infernal_escape_attempt(&state, access, &attempt);
    let after = value_hash(&json!({"state": &state, "access": access, "attempt": &attempt}));
    edge_from_result(result, before, after)
}

fn edge_from_package(
    package: &InfernalDeepsPackagePolicy,
    access: &InfernalAccessPolicy,
    attrition: &InfernalAttritionPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    relics: &InfernalRelicVaultPolicy,
    withering: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> Value {
    let before = package_input_hash(
        package, access, attrition, zones, bosses, relics, withering, undeath,
    );
    let result = derive_infernal_deeps_package(DeriveInfernalDeepsPackageArgs {
        package,
        access,
        attrition,
        zones,
        bosses,
        relics,
        withering,
        undeath,
    });
    let after = package_input_hash(
        package, access, attrition, zones, bosses, relics, withering, undeath,
    );
    edge_from_result(result, before, after)
}

fn edge_from_result<T>(result: Result<T, EngineError>, before: String, after: String) -> Value {
    let state_unchanged = before == after;
    match result {
        Ok(_) => {
            json!({"unexpected_success": true, "before_sha256": before, "after_sha256": after, "state_unchanged": state_unchanged})
        }
        Err(error) => {
            json!({"error_code": error.code(), "error": error.to_string(), "before_sha256": before, "after_sha256": after, "state_unchanged": state_unchanged})
        }
    }
}

fn package_input_hash(
    package: &InfernalDeepsPackagePolicy,
    access: &InfernalAccessPolicy,
    attrition: &InfernalAttritionPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    relics: &InfernalRelicVaultPolicy,
    withering: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> String {
    value_hash(&json!({
        "package": package,
        "access": access,
        "attrition": attrition,
        "zones": zones,
        "bosses": bosses,
        "relics": relics,
        "withering": withering,
        "undeath": undeath,
    }))
}

fn value_hash(value: &impl serde::Serialize) -> String {
    litd_hash::sha256_hex(&serde_json::to_vec(value).expect("edge input must serialize"))
}

fn outside_state() -> InfernalAccessState {
    InfernalAccessState {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        character_id: "char.morvane_ash".to_string(),
        party_id: "party.black_reliquary.1".to_string(),
        location_id: "gate.cinders".to_string(),
        faction_id: None,
        inside_infernal_deeps: false,
        current_circle: None,
        no_recall_lock_active: false,
        infernal_debt_count: 0,
        access_history: Vec::new(),
    }
}

fn escape_attempt(kind: InfernalEscapeKind) -> InfernalEscapeAttempt {
    InfernalEscapeAttempt {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        attempt_id: "escape.recall.morvane.1".to_string(),
        character_id: "char.morvane_ash".to_string(),
        kind,
        declared_destination_id: "safe.galdmere.shrine".to_string(),
    }
}
