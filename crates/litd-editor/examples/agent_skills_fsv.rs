use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::agent_skills::{
    AgentSkillDiscoveryLedger, AgentSkillSearchQuery, AgentSkillSearchStatus,
    build_agent_skill_discovery_ledger, build_agent_skill_registry, default_agent_skill_registry,
    parse_agent_skill_discovery_ledger, read_agent_skill_discovery_ledger, search_agent_skills,
    write_agent_skill_discovery_ledger,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, output_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_AGENT_SKILLS_FSV",
        "issue1206_agent_skills",
        "manual-fsv.json",
    )?;
    let root = output_path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1206 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let state_path = root.join("agent-skills-ledger.json");
    let aster_dir = root.join("aster");
    let before = path_state(&output_path, &state_path, &aster_dir);

    let default_registry = default_agent_skill_registry()?;
    let mut skills = default_registry.skills.clone();
    skills.push(default_registry.skills[0].clone());
    let registry = build_agent_skill_registry("issue1206-agent-skills-fsv", skills)?;
    let happy = search_agent_skills(
        &registry,
        &query("issue1206-happy", "paint terrain stamp height", 3),
    )?;
    let no_match = search_agent_skills(
        &registry,
        &query("issue1206-no-match", "offshore tax filing spreadsheet", 5),
    )?;
    let ambiguous = search_agent_skills(
        &registry,
        &query("issue1206-ambiguous", "agent read state story tools", 5),
    )?;
    let ledger = build_agent_skill_discovery_ledger(
        "issue1206-agent-skills-fsv",
        registry,
        vec![happy.clone(), no_match.clone(), ambiguous.clone()],
    )?;
    verify_expected(&ledger)?;

    let writeback = write_agent_skill_discovery_ledger(&state_path, &aster_dir, &ledger)?;
    let readback = read_agent_skill_discovery_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing issue1206 agent-skill discovery ledger")?;
    let disk_bytes = fs::read(&state_path)?;
    if disk_bytes != readback.bytes {
        return Err("state file bytes differ from Aster agent-skill ledger bytes".into());
    }
    let parsed = parse_agent_skill_discovery_ledger(&readback.bytes)?;
    verify_expected(&parsed)?;

    let document = json!({
        "schema": "litd.issue1206.agent_skills_fsv.v1",
        "issue": 1206,
        "source_of_truth": "Aster agent-skill discovery ledger row and JSON mirror readback",
        "before": before,
        "happy_path": {
            "before": {"query": "paint terrain stamp height", "limit": 3},
            "after": {
                "state_path": state_path,
                "state_sha256": sha256_hex(&disk_bytes),
                "aster_sha256": readback.sha256,
                "writeback_sha256": writeback.sha256,
                "state_matches_aster": disk_bytes == readback.bytes,
                "skill_count": parsed.registry.skill_count,
                "duplicate_count": parsed.registry.duplicate_count,
                "search_count": parsed.search_count,
                "graph_rows": readback.graph_rows,
                "seq": readback.seq,
                "terrain_result": summarize_search(&happy),
                "passed": true
            }
        },
        "edges": [
            {
                "edge": "no_match_intent",
                "before": {"query": no_match.intent},
                "after": summarize_search(&no_match)
            },
            {
                "edge": "ambiguous_intent_ranked",
                "before": {"query": ambiguous.intent},
                "after": summarize_search(&ambiguous)
            },
            {
                "edge": "duplicate_skill_deduped",
                "before": {"default_skill_count": default_registry.skill_count, "added_duplicate": default_registry.skills[0].skill_id},
                "after": {"skill_count": parsed.registry.skill_count, "duplicate_count": parsed.registry.duplicate_count}
            }
        ]
    });
    write_document_with_final_state(&output_path, document)?;
    let report_readback = fsv_run.record_json_file(&output_path)?;
    println!(
        "wrote issue1206 agent skills FSV to {} sha256={}",
        output_path.display(),
        report_readback.sha256
    );
    println!(
        "ledger skills={} duplicate_count={} searches={} graph_rows={} state_matches_aster={} sha={}",
        parsed.registry.skill_count,
        parsed.registry.duplicate_count,
        parsed.search_count,
        readback.graph_rows,
        disk_bytes == readback.bytes,
        readback.sha256
    );
    println!(
        "searches happy={} top={} no_match={} ambiguous_top={} second={}",
        happy.code,
        happy.results[0].skill_id,
        no_match.code,
        ambiguous.results[0].skill_id,
        ambiguous.results[1].skill_id
    );
    Ok(())
}

fn verify_expected(ledger: &AgentSkillDiscoveryLedger) -> Result<(), Box<dyn std::error::Error>> {
    if ledger.registry.skill_count < 10 || ledger.registry.duplicate_count != 1 {
        return Err("registry did not preserve expected skill count and dedupe evidence".into());
    }
    let happy = search(ledger, "issue1206-happy")?;
    if happy.status != AgentSkillSearchStatus::Matched
        || happy.results[0].skill_id != "skill.terrain_world"
    {
        return Err("terrain intent did not return terrain_world first".into());
    }
    let no_match = search(ledger, "issue1206-no-match")?;
    if no_match.status != AgentSkillSearchStatus::NoMatch || !no_match.results.is_empty() {
        return Err("no-match intent did not fail closed with zero results".into());
    }
    let ambiguous = search(ledger, "issue1206-ambiguous")?;
    if ambiguous.results.len() < 2 || ambiguous.results[0].score < ambiguous.results[1].score {
        return Err("ambiguous intent did not return a ranked result list".into());
    }
    Ok(())
}

fn search<'a>(
    ledger: &'a AgentSkillDiscoveryLedger,
    query_id: &str,
) -> Result<&'a litd_editor::agent_skills::AgentSkillSearchReport, Box<dyn std::error::Error>> {
    ledger
        .searches
        .iter()
        .find(|search| search.query_id == query_id)
        .ok_or_else(|| format!("missing skill search {query_id}").into())
}

fn summarize_search(
    report: &litd_editor::agent_skills::AgentSkillSearchReport,
) -> serde_json::Value {
    json!({
        "query_id": report.query_id,
        "status": format!("{:?}", report.status),
        "code": report.code,
        "result_count": report.result_count,
        "results": report.results.iter().map(|result| json!({
            "skill_id": result.skill_id,
            "score": result.score,
            "matched_terms": result.matched_terms,
            "tool_count": result.tool_names.len(),
            "readback_tool_count": result.readback_tool_names.len(),
            "progressive_disclosure_ref": result.progressive_disclosure_ref
        })).collect::<Vec<_>>()
    })
}

fn query(query_id: &str, intent: &str, limit: usize) -> AgentSkillSearchQuery {
    AgentSkillSearchQuery {
        query_id: query_id.to_string(),
        intent: intent.to_string(),
        limit,
        requested_by: "issue1206-fsv".to_string(),
    }
}

fn write_document_with_final_state(
    path: &PathBuf,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(&document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            if fs::metadata(path)?.len() != expected_len {
                return Err("issue1206 FSV artifact byte readback mismatch".into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err("issue1206 FSV artifact byte-count stabilization failed".into())
}

fn path_state(output_path: &PathBuf, state_path: &PathBuf, aster_dir: &Path) -> serde_json::Value {
    json!({
        "artifact_exists": output_path.exists(),
        "artifact_bytes": fs::metadata(output_path).map(|metadata| metadata.len()).unwrap_or(0),
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "aster_dir_exists": aster_dir.exists()
    })
}
