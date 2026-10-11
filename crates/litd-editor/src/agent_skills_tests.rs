use std::{fs, path::PathBuf};

use super::*;

#[test]
fn default_registry_searches_known_skills_by_intent() {
    let registry = default_agent_skill_registry().unwrap();

    let report = search_agent_skills(
        &registry,
        &query("issue1206-terrain", "paint terrain stamp height", 3),
    )
    .unwrap();

    assert_eq!(report.status, AgentSkillSearchStatus::Matched);
    assert_eq!(report.code, "LITD-SKILL-MATCHED");
    assert_eq!(report.results[0].skill_id, "skill.terrain_world");
    assert!(!report.results[0].tool_names.is_empty());
    println!(
        "issue1206 happy skill={} score={} tools={}",
        report.results[0].skill_id,
        report.results[0].score,
        report.results[0].tool_names.len()
    );
}

#[test]
fn no_match_and_ambiguous_intent_are_structured() {
    let registry = default_agent_skill_registry().unwrap();

    let no_match = search_agent_skills(
        &registry,
        &query("issue1206-no-match", "offshore tax filing spreadsheet", 5),
    )
    .unwrap();
    assert_eq!(no_match.status, AgentSkillSearchStatus::NoMatch);
    assert_eq!(no_match.code, "LITD-SKILL-NO-MATCH");
    assert!(no_match.results.is_empty());

    let ambiguous = search_agent_skills(
        &registry,
        &query("issue1206-ambiguous", "agent read state story tools", 5),
    )
    .unwrap();
    assert_eq!(ambiguous.status, AgentSkillSearchStatus::Matched);
    assert!(ambiguous.results.len() >= 2);
    assert!(ambiguous.results[0].score >= ambiguous.results[1].score);
    println!(
        "issue1206 edges no_match={} ambiguous_top={} second={}",
        no_match.code, ambiguous.results[0].skill_id, ambiguous.results[1].skill_id
    );
}

#[test]
fn duplicate_skill_is_deduped_by_stable_id() {
    let first = skill("skill.agent_binding", "Bind Agents", "bind agent brains");
    let mut duplicate = skill("skill.agent_binding", "Bind Agents", "agent prompt model");
    duplicate
        .tool_names
        .push("litd.editor.agents.set_prompt_model".to_string());
    duplicate.intents.push("set prompt model".to_string());

    let registry = build_agent_skill_registry("issue1206-dedupe", vec![first, duplicate]).unwrap();

    assert_eq!(registry.skill_count, 1);
    assert_eq!(registry.duplicate_count, 1);
    assert!(
        registry.skills[0]
            .tool_names
            .contains(&"litd.editor.agents.set_prompt_model".to_string())
    );
}

#[test]
fn discovery_ledger_persists_to_aster_and_reads_back() {
    let registry = default_agent_skill_registry().unwrap();
    let happy = search_agent_skills(
        &registry,
        &query("issue1206-ledger-happy", "bind agent prompt model", 3),
    )
    .unwrap();
    let no_match = search_agent_skills(
        &registry,
        &query("issue1206-ledger-no-match", "offshore tax spreadsheet", 3),
    )
    .unwrap();
    let ledger = build_agent_skill_discovery_ledger(
        "issue1206-agent-skills-test",
        registry,
        vec![happy, no_match],
    )
    .unwrap();
    let root = test_root("ledger");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("agent-skills-ledger.json");
    let aster_dir = root.join("aster");

    let writeback = write_agent_skill_discovery_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let readback = read_agent_skill_discovery_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_agent_skill_discovery_ledger(&readback.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(readback.bytes, writeback.bytes);
    assert_eq!(parsed.search_count, 2);
    assert!(parsed.registry.skill_count >= 10);
    assert_eq!(parsed.searches[0].status, AgentSkillSearchStatus::Matched);
    assert_eq!(parsed.searches[1].status, AgentSkillSearchStatus::NoMatch);
    println!(
        "issue1206 aster skills={} searches={} graph_rows={} sha={}",
        parsed.registry.skill_count, parsed.search_count, readback.graph_rows, readback.sha256
    );
}

fn query(query_id: &str, intent: &str, limit: usize) -> AgentSkillSearchQuery {
    AgentSkillSearchQuery {
        query_id: query_id.to_string(),
        intent: intent.to_string(),
        limit,
        requested_by: "issue1206-test".to_string(),
    }
}

fn skill(skill_id: &str, title: &str, description: &str) -> AgentSkill {
    AgentSkill {
        skill_id: skill_id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        source_domain_id: "in_game_agents".to_string(),
        source_issue: 924,
        intents: vec!["bind agent".to_string()],
        tool_names: vec!["litd.editor.agents.bind_agent".to_string()],
        readback_tool_names: vec!["litd.editor.agents.read_agents".to_string()],
        progressive_disclosure_ref: "docs/readme/03-mcp-agent.png".to_string(),
    }
}

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1206", label)
}
