#[path = "fixtures/issue964_themes_fixture.rs"]
#[allow(dead_code)]
mod issue964_themes_fixture;

use std::fs;

use issue964_themes_fixture::{happy_catalog, happy_input, happy_rules, mismatch_input};
use litd_engine::{
    EngineError,
    calyx_content::EngineCalyxContentStore,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    math::FixedVec3,
    rpg::{
        ImmortalActionKind, ImmortalCatalog, ImmortalDecisionReadback, ImmortalDefinition,
        ImmortalEvaluationInput, ImmortalRules, ImmortalSignal, ImmortalSignalKind,
        RPG_IMMORTAL_SCHEMA, evaluate_immortal_signal, immortal_content_records,
        persist_and_lower_rpg_authored_content, validate_rpg_authored_content_records,
    },
};
use serde_json::{Value, json};

const IMMORTALS_PATH: &str = "crates/litd-engine/src/rpg/immortals.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/immortals_content.rs";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/immortals_types.rs";
const VALIDATE_PATH: &str = "crates/litd-engine/src/rpg/immortals_validate.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/immortals_tests.rs";
const THEMES_FIXTURE_PATH: &str = "crates/litd-engine/examples/fixtures/issue964_themes_fixture.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue965_immortal_agents_fsv.rs";
const SOURCE_PATHS: [&str; 7] = [
    IMMORTALS_PATH,
    CONTENT_PATH,
    TYPES_PATH,
    VALIDATE_PATH,
    TEST_PATH,
    THEMES_FIXTURE_PATH,
    EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue965_immortal_agents")?;
    let rules = ImmortalRules::mud_defaults();
    let content_input = base_input(signal(
        "signal.content",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
    ));
    let records = immortal_content_records(&content_input.catalog, &rules)?;
    let validation = validate_rpg_authored_content_records(&records)?;
    let vault_dir = run.artifact_dir("calyx-vault-immortals")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    let decisions = accepted_decisions(&rules)?;
    let refused = refused_mismatch(&rules)?;
    let edges = edge_readbacks(&rules)?;

    let accepted_count = decisions
        .iter()
        .filter(|decision| decision.accepted)
        .count();
    let possess_order = decisions
        .iter()
        .find(|decision| decision.action_kind == ImmortalActionKind::PossessMob)
        .and_then(|decision| decision.lowered_effects.first())
        .and_then(|effect| effect.sim_order)
        .ok_or("missing possession sim order")?;
    let all_ledger_hashes = decisions
        .iter()
        .all(|decision| decision.decision_ledger_sha256.len() == 64);
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    if accepted_count != 5
        || refused.accepted
        || possess_order.source.kind != litd_engine::sim_order::SimOrderSourceKind::LoweredAgent
        || !all_ledger_hashes
        || !all_edges_matched
    {
        return Err("issue965 Immortal agent FSV acceptance failed".into());
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
        "schema": "litd.issue965.immortal_agents_fsv.v2",
        "issue": 965,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this report through Synapse and inspect accepted decisions, refused mismatch, authored_validation, lowered_immortals, lowered possession order, edge codes, and hashes."
        },
        "research_sources": [
            "https://wiki.carrionfields.net/game/newbie-in-depth",
            "https://carrionfields.net/",
            "https://carrionfields.net/pbf-character-sheet.php?name=Gurwunt",
            "https://www.mudconnect.com/cgi-bin/search.cgi?mode=mud_listing&mud=Carrion+Fields",
            "https://carrionfields.net/pbf-character-sheet.php?name=Terenthial"
        ],
        "readme_references_reviewed": [
            "docs/readme/06-living-agents.png",
            "docs/readme/S17-agent-binding.png",
            "docs/readme/03-mcp-agent.png"
        ],
        "sufficiency_argument": "The fixture uses one deity/Immortal and one role/religion-matched character because #965 is about action classes and deterministic boundaries, not content volume. It proves all five requested Immortal/GM capabilities: event, empowerment, quest, RP enforcement, and possession/animation. It also proves explicit refusal for religion/theme mismatch and five fail-loud invalid classes.",
        "expected_actuals": {
            "source_readback_count": source_readback_count,
            "immortal_source_count": source_count_for("crates/litd-engine/src/rpg/immortals"),
            "example_source_count": source_count_for("crates/litd-engine/examples"),
            "fixture_source_count": source_count_for("fixture"),
            "accepted_count": accepted_count,
            "action_count": decisions.len(),
            "effect_count": decisions.iter().map(|decision| decision.effect_count).sum::<usize>(),
            "grant_empowerment_tier": decisions[0].lowered_effects[0].empowerment_tier,
            "quest_id_present": decisions[1].lowered_effects[0].quest_id.is_some(),
            "event_id_present": decisions[2].lowered_effects[0].event_id.is_some(),
            "enforcement_severity": decisions[3].lowered_effects[0].enforcement_severity,
            "possession_order_source": format!("{:?}", possess_order.source.kind),
            "possession_lowered_agent_id": possess_order.source.id,
            "refused_mismatch_grade": refused.grade,
            "refused_mismatch_effect_count": refused.effect_count,
            "edge_count": edges.len(),
            "authored_record_count": validation.record_count,
            "lowered_row_count": lowered.row_count,
            "acquired_map_record_count": validation.acquired_map_record_count,
            "all_editor_authored_sources": validation.all_editor_authored_sources,
            "no_builtin_map_runtime": validation.no_builtin_map_runtime,
            "calyx_live_query_allowed_in_hot_path": lowered.calyx_live_query_allowed_in_hot_path,
            "error_code": "LITD-RPG-IMMORTAL-INVALID-CONFIG"
        },
        "authored_validation": validation,
        "lowered_immortals": {
            "package_fingerprint": lowered.package_fingerprint,
            "entities": lowered.entities,
        },
        "decisions": decisions,
        "refused_mismatch": refused,
        "edges": edges
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["immortal_source_count"], 5)?;
    require_i64(&report["expected_actuals"]["example_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["fixture_source_count"], 1)?;
    require_i64(&report["expected_actuals"]["authored_record_count"], 8)?;
    require_i64(&report["expected_actuals"]["lowered_row_count"], 8)?;
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
        "ISSUE965_IMMORTAL_AGENTS_FSV report={} bytes={} sha256={} accepted={} effects={} possess_agent={} rows={} no_builtin_map={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["expected_actuals"]["accepted_count"],
        readback.json["expected_actuals"]["effect_count"],
        readback.json["expected_actuals"]["possession_lowered_agent_id"],
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

fn accepted_decisions(rules: &ImmortalRules) -> Result<Vec<ImmortalDecisionReadback>, EngineError> {
    [
        signal(
            "signal.prayer",
            ImmortalSignalKind::Prayer,
            ImmortalActionKind::GrantEmpowerment,
        ),
        signal(
            "signal.quest",
            ImmortalSignalKind::QuestOpportunity,
            ImmortalActionKind::IssueQuest,
        ),
        signal(
            "signal.event",
            ImmortalSignalKind::EventOpportunity,
            ImmortalActionKind::RunEvent,
        ),
        signal(
            "signal.rule_break",
            ImmortalSignalKind::RuleBreak,
            ImmortalActionKind::EnforceRoleplay,
        ),
        signal(
            "signal.possess",
            ImmortalSignalKind::PossessionRequest,
            ImmortalActionKind::PossessMob,
        ),
    ]
    .into_iter()
    .map(|signal| evaluate_immortal_signal(&base_input(signal), rules))
    .collect()
}

fn refused_mismatch(rules: &ImmortalRules) -> Result<ImmortalDecisionReadback, EngineError> {
    let mut input = base_input(signal(
        "signal.mismatch",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
    ));
    input.theme_input = mismatch_input();
    evaluate_immortal_signal(&input, rules)
}

fn edge_readbacks(rules: &ImmortalRules) -> Result<Vec<Value>, EngineError> {
    let mut duplicate = base_input(signal(
        "signal.duplicate",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
    ));
    duplicate
        .catalog
        .immortals
        .push(duplicate.catalog.immortals[0].clone());

    let mut unknown = base_input(signal(
        "signal.unknown",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
    ));
    unknown.signal.immortal_id = "immortal.missing".to_string();

    let mut outside_ward = base_input(signal(
        "signal.outside",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
    ));
    outside_ward.signal.tags.push("forbidden".to_string());

    let mut missing_body = base_input(signal(
        "signal.no_body",
        ImmortalSignalKind::PossessionRequest,
        ImmortalActionKind::PossessMob,
    ));
    missing_body.signal.target_body_id = None;

    let mut disallowed = base_input(signal(
        "signal.disallowed",
        ImmortalSignalKind::EventOpportunity,
        ImmortalActionKind::RunEvent,
    ));
    disallowed.catalog.immortals[0].allowed_actions = vec![ImmortalActionKind::GrantEmpowerment];

    Ok(vec![
        edge("duplicate_immortal", duplicate, rules)?,
        edge("unknown_immortal", unknown, rules)?,
        edge("outside_ward_tag", outside_ward, rules)?,
        edge("missing_possession_body", missing_body, rules)?,
        edge("disallowed_action", disallowed, rules)?,
    ])
}

fn edge(
    case: &str,
    input: ImmortalEvaluationInput,
    rules: &ImmortalRules,
) -> Result<Value, EngineError> {
    let before = hash_json(&input)?;
    let error = match evaluate_immortal_signal(&input, rules) {
        Ok(_) => {
            return Err(EngineError::RpgImmortalInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid Immortal data to fail".to_string(),
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
        "matched": before == after && error.code() == "LITD-RPG-IMMORTAL-INVALID-CONFIG",
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn base_input(signal: ImmortalSignal) -> ImmortalEvaluationInput {
    ImmortalEvaluationInput {
        catalog: ImmortalCatalog {
            schema: RPG_IMMORTAL_SCHEMA.to_string(),
            catalog_id: "catalog.issue965.immortals".to_string(),
            immortals: vec![ImmortalDefinition {
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
                ward_tags: ids(&[
                    "compassion",
                    "oathbound",
                    "rp_break",
                    "quest",
                    "event",
                    "possession",
                ]),
                lowered_agent_id: 965,
                max_empowerment_tier: 51,
            }],
        },
        theme_catalog: happy_catalog(),
        theme_rules: happy_rules(),
        theme_input: happy_input(),
        signal,
    }
}

fn signal(id: &str, kind: ImmortalSignalKind, action: ImmortalActionKind) -> ImmortalSignal {
    let mut tags = ids(&["compassion", "oathbound"]);
    match action {
        ImmortalActionKind::IssueQuest => tags.push("quest".to_string()),
        ImmortalActionKind::RunEvent => tags.push("event".to_string()),
        ImmortalActionKind::EnforceRoleplay => tags.push("rp_break".to_string()),
        ImmortalActionKind::PossessMob => tags.push("possession".to_string()),
        ImmortalActionKind::GrantEmpowerment => {}
    }
    ImmortalSignal {
        signal_id: id.to_string(),
        immortal_id: "immortal.luminara".to_string(),
        tick: 77,
        kind,
        requested_action: action,
        strength_milli: 900,
        rp_quality_milli: 850,
        source_cx_id: format!("cx.issue965.{id}"),
        target_character_id: "char.ashen.mender".to_string(),
        target_body_id: Some(42),
        possession_velocity: Some(FixedVec3::from_i32s(1, 0, 0)),
        tags,
        evidence_refs: vec![format!("evidence://issue965/{id}")],
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
            reason: format!("serialize issue965 Immortal FSV value: {error}"),
        })
}

fn require_i64(value: &Value, expected: i64) -> Result<(), Box<dyn std::error::Error>> {
    match value.as_i64() {
        Some(actual) if actual == expected => Ok(()),
        actual => Err(format!("expected {expected}, got {actual:?}").into()),
    }
}

fn require_bool(value: &Value, expected: bool) -> Result<(), Box<dyn std::error::Error>> {
    match value.as_bool() {
        Some(actual) if actual == expected => Ok(()),
        actual => Err(format!("expected {expected}, got {actual:?}").into()),
    }
}
