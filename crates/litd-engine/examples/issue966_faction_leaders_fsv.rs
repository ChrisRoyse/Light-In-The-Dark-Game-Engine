#[path = "fixtures/issue964_themes_fixture.rs"]
#[allow(dead_code)]
mod issue964_themes_fixture;

use std::fs;

use issue964_themes_fixture::{happy_catalog, happy_input, happy_rules};
use litd_engine::{
    EngineError,
    calyx_content::EngineCalyxContentStore,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        FactionLeaderActionKind, FactionLeaderCatalog, FactionLeaderDecisionReadback,
        FactionLeaderEvaluationInput, FactionLeaderRules, FactionLeaderSignal,
        FactionLeaderSignalKind, FactionOrganizationDefinition, FactionOrganizationKind,
        FactionRankDefinition, RPG_FACTION_LEADER_SCHEMA, evaluate_faction_leader_signal,
        faction_leader_content_records, persist_and_lower_rpg_authored_content,
        validate_rpg_authored_content_records,
    },
};
use serde_json::{Value, json};

const FACTION_LEADERS_PATH: &str = "crates/litd-engine/src/rpg/faction_leaders.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/faction_leaders_content.rs";
const READBACK_PATH: &str = "crates/litd-engine/src/rpg/faction_leaders_readback.rs";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/faction_leaders_types.rs";
const VALIDATE_PATH: &str = "crates/litd-engine/src/rpg/faction_leaders_validate.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/faction_leaders_tests.rs";
const THEMES_FIXTURE_PATH: &str = "crates/litd-engine/examples/fixtures/issue964_themes_fixture.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue966_faction_leaders_fsv.rs";
const SOURCE_PATHS: [&str; 8] = [
    FACTION_LEADERS_PATH,
    CONTENT_PATH,
    READBACK_PATH,
    TYPES_PATH,
    VALIDATE_PATH,
    TEST_PATH,
    THEMES_FIXTURE_PATH,
    EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue966_faction_leaders")?;
    let rules = FactionLeaderRules::mud_defaults();
    let content_input = base_input(signal(
        "signal.content",
        FactionLeaderSignalKind::Application,
        FactionLeaderActionKind::Recruit,
    ));
    let records = faction_leader_content_records(&content_input.catalog, &rules)?;
    let validation = validate_rpg_authored_content_records(&records)?;
    let vault_dir = run.artifact_dir("calyx-vault-faction-leaders")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    let decisions = accepted_decisions(&rules)?;
    let refused = refused_theme_mismatch(&rules)?;
    let edges = edge_readbacks(&rules)?;

    let accepted_count = decisions
        .iter()
        .filter(|decision| decision.accepted)
        .count();
    let all_orders_lowered = decisions.iter().all(|decision| {
        decision.lowered_effects.first().is_some_and(|effect| {
            effect.leader_order.lowered_agent_id == 966
                && matches!(
                    effect.leader_order.source_kind,
                    litd_engine::rpg::FactionLeaderOrderSourceKind::LoweredAgent
                )
        })
    });
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    if accepted_count != 6 || refused.accepted || !all_orders_lowered || !all_edges_matched {
        return Err("issue966 faction-leader FSV acceptance failed".into());
    }
    let source_readbacks = SOURCE_PATHS
        .into_iter()
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;
    let source_readback_count = source_readbacks.len();
    let source_count_for = |needle: &str| -> usize {
        SOURCE_PATHS
            .iter()
            .filter(|path| path.contains(needle))
            .count()
    };

    let report = json!({
        "schema": "litd.issue966.faction_leaders_fsv.v2",
        "issue": 966,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this report through Synapse and inspect organization decisions, lowered orders, theme refusal, authored_validation, lowered_faction_leaders, edge codes, and hashes."
        },
        "research_sources": [
            "https://wiki.carrionfields.net/game/newbie-in-depth",
            "https://carrionfields.net/helpsearch.php?id=9",
            "https://wiki.carrionfields.net/cabal",
            "https://carrionfields.net/pbf-character-sheet.php?name=Bulonk",
            "https://carrionfields.net/pbf-character-sheet.php?name=Jyslin"
        ],
        "readme_references_reviewed": [
            "docs/readme/06-living-agents.png",
            "docs/readme/GP18-faction-war.png",
            "docs/readme/S08-ranks.png",
            "docs/readme/S17-agent-binding.png",
            "docs/readme/S20-living-world.png",
            "docs/readme/L18-raid-warfare.png"
        ],
        "sufficiency_argument": "The fixture uses two organizations and one role/religion-matched character because #966 is about leader decision classes and deterministic lowering, not content volume. The set proves all requested behaviors: recruit, induct, set objective, item-of-power warfare, reward loyalty, and rank advancement/power unlock. It also proves valid refusal for organization dogma mismatch and five fail-loud invalid classes.",
        "expected_actuals": {
            "source_readback_count": source_readback_count,
            "faction_leader_source_count": source_count_for("crates/litd-engine/src/rpg/faction_leaders"),
            "example_source_count": source_count_for("crates/litd-engine/examples"),
            "fixture_source_count": source_count_for("fixture"),
            "accepted_count": accepted_count,
            "action_count": decisions.len(),
            "effect_count": decisions.iter().map(|decision| decision.effect_count).sum::<usize>(),
            "recruit_invitation_present": decisions[0].lowered_effects[0].recruit_invitation_id.is_some(),
            "induct_rank_after": decisions[1].lowered_effects[0].rank_after,
            "objective_id_present": decisions[2].lowered_effects[0].objective_id.is_some(),
            "raid_order_id": decisions[3].lowered_effects[0].raid_order_id,
            "reward_id_present": decisions[4].lowered_effects[0].reward_id.is_some(),
            "advance_rank_after": decisions[5].lowered_effects[0].rank_after,
            "advance_unlocked_power_count": decisions[5].lowered_effects[0].unlocked_power_ids.len(),
            "all_orders_lowered_agent_966": all_orders_lowered,
            "refused_theme_grade": refused.grade,
            "refused_theme_effect_count": refused.effect_count,
            "edge_count": edges.len(),
            "authored_record_count": validation.record_count,
            "lowered_row_count": lowered.row_count,
            "acquired_map_record_count": validation.acquired_map_record_count,
            "all_editor_authored_sources": validation.all_editor_authored_sources,
            "no_builtin_map_runtime": validation.no_builtin_map_runtime,
            "calyx_live_query_allowed_in_hot_path": lowered.calyx_live_query_allowed_in_hot_path,
            "error_code": "LITD-RPG-FACTION-INVALID-CONFIG"
        },
        "authored_validation": validation,
        "lowered_faction_leaders": {
            "package_fingerprint": lowered.package_fingerprint,
            "entities": lowered.entities,
        },
        "decisions": decisions,
        "refused_theme_mismatch": refused,
        "edges": edges
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(
        &report["expected_actuals"]["faction_leader_source_count"],
        6,
    )?;
    require_i64(&report["expected_actuals"]["example_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["fixture_source_count"], 1)?;
    require_i64(&report["expected_actuals"]["authored_record_count"], 20)?;
    require_i64(&report["expected_actuals"]["lowered_row_count"], 20)?;
    require_i64(&report["expected_actuals"]["acquired_map_record_count"], 0)?;
    require_bool(
        &report["expected_actuals"]["all_editor_authored_sources"],
        true,
    )?;
    require_bool(&report["expected_actuals"]["no_builtin_map_runtime"], true)?;
    require_bool(
        &report["expected_actuals"]["calyx_live_query_allowed_in_hot_path"],
        false,
    )?;
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE966_FACTION_LEADERS_FSV report={} bytes={} sha256={} accepted={} effects={} advance_rank={} rows={} no_builtin_map={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["expected_actuals"]["accepted_count"],
        readback.json["expected_actuals"]["effect_count"],
        readback.json["expected_actuals"]["advance_rank_after"],
        readback.json["expected_actuals"]["lowered_row_count"],
        readback.json["expected_actuals"]["no_builtin_map_runtime"],
        readback.json["expected_actuals"]["edge_count"],
    );
    Ok(())
}

fn source_readback(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(json!({
        "path": path,
        "bytes": bytes.len(),
        "sha256": litd_hash::sha256_hex(&bytes),
    }))
}

fn accepted_decisions(
    rules: &FactionLeaderRules,
) -> Result<Vec<FactionLeaderDecisionReadback>, EngineError> {
    [
        signal(
            "signal.application",
            FactionLeaderSignalKind::Application,
            FactionLeaderActionKind::Recruit,
        ),
        signal(
            "signal.induct",
            FactionLeaderSignalKind::SponsorSupport,
            FactionLeaderActionKind::Induct,
        ),
        signal(
            "signal.objective",
            FactionLeaderSignalKind::WorldThreat,
            FactionLeaderActionKind::SetObjective,
        ),
        signal(
            "signal.raid",
            FactionLeaderSignalKind::ItemOfPowerThreat,
            FactionLeaderActionKind::WageWarfare,
        ),
        signal(
            "signal.reward",
            FactionLeaderSignalKind::LoyaltyReport,
            FactionLeaderActionKind::RewardLoyalty,
        ),
        signal(
            "signal.rank",
            FactionLeaderSignalKind::RankReview,
            FactionLeaderActionKind::AdvanceRank,
        ),
    ]
    .into_iter()
    .map(|signal| evaluate_faction_leader_signal(&base_input(signal), rules))
    .collect()
}

fn refused_theme_mismatch(
    rules: &FactionLeaderRules,
) -> Result<FactionLeaderDecisionReadback, EngineError> {
    let mut input = base_input(signal(
        "signal.theme_mismatch",
        FactionLeaderSignalKind::Application,
        FactionLeaderActionKind::Recruit,
    ));
    input
        .theme_input
        .declared_theme_tags
        .push("greed".to_string());
    evaluate_faction_leader_signal(&input, rules)
}

fn edge_readbacks(rules: &FactionLeaderRules) -> Result<Vec<Value>, EngineError> {
    let mut duplicate = base_input(signal(
        "signal.duplicate",
        FactionLeaderSignalKind::Application,
        FactionLeaderActionKind::Recruit,
    ));
    duplicate
        .catalog
        .organizations
        .push(duplicate.catalog.organizations[0].clone());

    let mut duplicate_rank = base_input(signal(
        "signal.duplicate_rank",
        FactionLeaderSignalKind::Application,
        FactionLeaderActionKind::Recruit,
    ));
    let rank_clone = duplicate_rank.catalog.organizations[0].ranks[0].clone();
    duplicate_rank.catalog.organizations[0]
        .ranks
        .push(rank_clone);

    let mut outside_ward = base_input(signal(
        "signal.outside",
        FactionLeaderSignalKind::Application,
        FactionLeaderActionKind::Recruit,
    ));
    outside_ward.signal.tags.push("forbidden".to_string());

    let mut missing_item = base_input(signal(
        "signal.missing_item",
        FactionLeaderSignalKind::ItemOfPowerThreat,
        FactionLeaderActionKind::WageWarfare,
    ));
    missing_item.signal.target_item_id = None;

    let mut disallowed = base_input(signal(
        "signal.disallowed",
        FactionLeaderSignalKind::WorldThreat,
        FactionLeaderActionKind::SetObjective,
    ));
    disallowed.catalog.organizations[0].allowed_actions = vec![FactionLeaderActionKind::Recruit];

    Ok(vec![
        edge("duplicate_organization", duplicate, rules)?,
        edge("duplicate_rank", duplicate_rank, rules)?,
        edge("outside_ward_tag", outside_ward, rules)?,
        edge("missing_warfare_item", missing_item, rules)?,
        edge("disallowed_action", disallowed, rules)?,
    ])
}

fn edge(
    case: &str,
    input: FactionLeaderEvaluationInput,
    rules: &FactionLeaderRules,
) -> Result<Value, EngineError> {
    let before = hash_json(&input)?;
    let error = match evaluate_faction_leader_signal(&input, rules) {
        Ok(_) => {
            return Err(EngineError::RpgFactionInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid faction-leader data to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    let after = hash_json(&input)?;
    Ok(json!({
        "case": case,
        "before_input_sha256": before,
        "after_input_sha256": after,
        "state_unchanged": before == after,
        "matched": before == after && error.code() == "LITD-RPG-FACTION-INVALID-CONFIG",
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn base_input(signal: FactionLeaderSignal) -> FactionLeaderEvaluationInput {
    FactionLeaderEvaluationInput {
        catalog: catalog(),
        theme_catalog: happy_catalog(),
        theme_rules: happy_rules(),
        theme_input: happy_input(),
        signal,
    }
}

fn catalog() -> FactionLeaderCatalog {
    FactionLeaderCatalog {
        schema: RPG_FACTION_LEADER_SCHEMA.to_string(),
        catalog_id: "catalog.issue966.faction_leaders".to_string(),
        organizations: vec![
            organization(
                "organization.dawnward",
                "Dawnward",
                966,
                Some("religion.dawn_court"),
                Some("item.dawn_standard"),
                &["organization.onyx"],
            ),
            organization(
                "organization.onyx",
                "Onyx Compact",
                1966,
                None,
                Some("item.onyx_crown"),
                &["organization.dawnward"],
            ),
        ],
    }
}

fn organization(
    id: &str,
    name: &str,
    leader_agent_id: u32,
    religion_id: Option<&str>,
    item_of_power_id: Option<&str>,
    enemies: &[&str],
) -> FactionOrganizationDefinition {
    FactionOrganizationDefinition {
        organization_id: id.to_string(),
        display_name: name.to_string(),
        organization_kind: FactionOrganizationKind::Cabal,
        leader_agent_id,
        leader_display_name: format!("{name} Voice"),
        religion_id: religion_id.map(str::to_string),
        home_region_id: format!("region.{id}.sanctum"),
        item_of_power_id: item_of_power_id.map(str::to_string),
        allowed_actions: vec![
            FactionLeaderActionKind::Recruit,
            FactionLeaderActionKind::Induct,
            FactionLeaderActionKind::SetObjective,
            FactionLeaderActionKind::WageWarfare,
            FactionLeaderActionKind::RewardLoyalty,
            FactionLeaderActionKind::AdvanceRank,
        ],
        ward_tags: ids(&[
            "compassion",
            "oathbound",
            "recruit",
            "objective",
            "warfare",
            "loyalty",
            "rank",
        ]),
        required_theme_tags: ids(&["compassion", "oathbound"]),
        forbidden_theme_tags: ids(&["greed"]),
        ranks: vec![
            rank(1, "Initiate", 700, 0, &["cabal.power.sanctuary"]),
            rank(
                2,
                "Standard Bearer",
                800,
                700,
                &["cabal.power.rally", "cabal.power.siege_blessing"],
            ),
        ],
        enemy_organization_ids: ids(enemies),
    }
}

fn rank(rank: u16, title: &str, rp: u16, loyalty: u16, powers: &[&str]) -> FactionRankDefinition {
    FactionRankDefinition {
        rank,
        title: title.to_string(),
        min_rp_quality_milli: rp,
        min_loyalty_milli: loyalty,
        power_ids: ids(powers),
    }
}

fn signal(
    id: &str,
    kind: FactionLeaderSignalKind,
    action: FactionLeaderActionKind,
) -> FactionLeaderSignal {
    let mut tags = ids(&["compassion", "oathbound"]);
    match action {
        FactionLeaderActionKind::Recruit | FactionLeaderActionKind::Induct => {
            tags.push("recruit".to_string());
        }
        FactionLeaderActionKind::SetObjective => tags.push("objective".to_string()),
        FactionLeaderActionKind::WageWarfare => tags.push("warfare".to_string()),
        FactionLeaderActionKind::RewardLoyalty => tags.push("loyalty".to_string()),
        FactionLeaderActionKind::AdvanceRank => tags.push("rank".to_string()),
    }
    FactionLeaderSignal {
        signal_id: id.to_string(),
        organization_id: "organization.dawnward".to_string(),
        tick: 77,
        kind,
        requested_action: action,
        strength_milli: 900,
        rp_quality_milli: 850,
        loyalty_milli: 760,
        source_cx_id: format!("cx.issue966.{id}"),
        target_character_id: "char.ashen.mender".to_string(),
        target_organization_id: (action == FactionLeaderActionKind::WageWarfare)
            .then(|| "organization.onyx".to_string()),
        target_item_id: (action == FactionLeaderActionKind::WageWarfare)
            .then(|| "item.onyx_crown".to_string()),
        current_rank: matches!(
            action,
            FactionLeaderActionKind::RewardLoyalty | FactionLeaderActionKind::AdvanceRank
        )
        .then_some(1),
        requested_rank: (action == FactionLeaderActionKind::AdvanceRank).then_some(2),
        tags,
        evidence_refs: vec![format!("evidence://issue966/{id}")],
    }
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn hash_json<T: serde::Serialize>(value: &T) -> Result<String, EngineError> {
    serde_json::to_vec(value)
        .map(|bytes| litd_hash::sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize issue966 faction-leader FSV value: {error}"),
        })
}

fn require_i64(value: &Value, expected: i64) -> Result<(), Box<dyn std::error::Error>> {
    if value.as_i64() == Some(expected) {
        return Ok(());
    }
    Err(format!("expected {expected}, got {value}").into())
}

fn require_bool(value: &Value, expected: bool) -> Result<(), Box<dyn std::error::Error>> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(format!("expected {expected}, got {value}").into())
}
