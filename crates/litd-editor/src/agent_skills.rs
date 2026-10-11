use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{EngineError, render::shader::shader_fingerprint};
use serde::{Deserialize, Serialize};

use crate::mcp_tools::{McpToolKind, default_mcp_domain_toolset_manifest};

#[path = "agent_skills_store.rs"]
mod agent_skills_store;

pub use agent_skills_store::{
    AgentSkillRegistryWriteback, parse_agent_skill_discovery_ledger,
    read_agent_skill_discovery_ledger, write_agent_skill_discovery_ledger,
};

pub const AGENT_SKILL_DISCOVERY_SCHEMA: &str = "litd.agent_skills.discovery_ledger.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSkill {
    pub skill_id: String,
    pub title: String,
    pub description: String,
    pub source_domain_id: String,
    pub source_issue: u32,
    pub intents: Vec<String>,
    pub tool_names: Vec<String>,
    pub readback_tool_names: Vec<String>,
    pub progressive_disclosure_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSkillRegistry {
    pub schema: String,
    pub registry_id: String,
    pub skill_count: usize,
    pub duplicate_count: usize,
    pub skills: Vec<AgentSkill>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSkillSearchQuery {
    pub query_id: String,
    pub intent: String,
    pub limit: usize,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSkillSearchResult {
    pub skill_id: String,
    pub title: String,
    pub score: u32,
    pub matched_terms: Vec<String>,
    pub tool_names: Vec<String>,
    pub readback_tool_names: Vec<String>,
    pub progressive_disclosure_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentSkillSearchStatus {
    Matched,
    NoMatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSkillSearchReport {
    pub schema: String,
    pub query_id: String,
    pub registry_id: String,
    pub status: AgentSkillSearchStatus,
    pub code: String,
    pub intent: String,
    pub query_fingerprint: String,
    pub requested_by: String,
    pub limit: usize,
    pub result_count: usize,
    pub results: Vec<AgentSkillSearchResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSkillDiscoveryLedger {
    pub schema: String,
    pub ledger_id: String,
    pub registry: AgentSkillRegistry,
    pub search_count: usize,
    pub searches: Vec<AgentSkillSearchReport>,
}

pub fn default_agent_skill_registry() -> Result<AgentSkillRegistry, EngineError> {
    let toolset = default_mcp_domain_toolset_manifest()?;
    let skills = toolset
        .domains
        .iter()
        .map(|domain| AgentSkill {
            skill_id: format!("skill.{}", domain.domain_id),
            title: domain.title.clone(),
            description: format!(
                "{} via {} with {} tools",
                domain.title,
                domain.gui_surface,
                domain.tools.len()
            ),
            source_domain_id: domain.domain_id.clone(),
            source_issue: domain.child_issue,
            intents: domain
                .capability_namespaces
                .iter()
                .chain(domain.mcp_prefixes.iter())
                .chain(domain.tools.iter().map(|tool| &tool.title))
                .cloned()
                .collect(),
            tool_names: domain
                .tools
                .iter()
                .map(|tool| tool.tool_name.clone())
                .collect(),
            readback_tool_names: domain
                .tools
                .iter()
                .filter(|tool| tool.kind == McpToolKind::Readback)
                .map(|tool| tool.tool_name.clone())
                .collect(),
            progressive_disclosure_ref: domain
                .readme_refs
                .first()
                .cloned()
                .unwrap_or_else(|| domain.gui_surface.clone()),
        })
        .collect();
    build_agent_skill_registry("litd.agent_skills.default.v1", skills)
}

pub fn build_agent_skill_registry(
    registry_id: impl Into<String>,
    skills: Vec<AgentSkill>,
) -> Result<AgentSkillRegistry, EngineError> {
    let mut by_id = BTreeMap::<String, AgentSkill>::new();
    let mut duplicate_count = 0;
    for skill in skills {
        validate_skill(&skill)?;
        if let Some(existing) = by_id.get_mut(&skill.skill_id) {
            duplicate_count += 1;
            merge_skill(existing, skill);
        } else {
            by_id.insert(skill.skill_id.clone(), skill);
        }
    }
    let mut skills = by_id.into_values().collect::<Vec<_>>();
    skills.sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
    let registry = AgentSkillRegistry {
        schema: AGENT_SKILL_DISCOVERY_SCHEMA.to_string(),
        registry_id: registry_id.into(),
        skill_count: skills.len(),
        duplicate_count,
        skills,
    };
    validate_agent_skill_registry(&registry)?;
    Ok(registry)
}

pub fn search_agent_skills(
    registry: &AgentSkillRegistry,
    query: &AgentSkillSearchQuery,
) -> Result<AgentSkillSearchReport, EngineError> {
    validate_agent_skill_registry(registry)?;
    validate_search_query(query)?;
    let terms = token_counts(&query.intent);
    let mut results = registry
        .skills
        .iter()
        .filter_map(|skill| score_skill(skill, &terms))
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.skill_id.cmp(&right.skill_id))
    });
    results.truncate(query.limit);
    let status = if results.is_empty() {
        AgentSkillSearchStatus::NoMatch
    } else {
        AgentSkillSearchStatus::Matched
    };
    let report = AgentSkillSearchReport {
        schema: AGENT_SKILL_DISCOVERY_SCHEMA.to_string(),
        query_id: query.query_id.clone(),
        registry_id: registry.registry_id.clone(),
        status,
        code: if results.is_empty() {
            "LITD-SKILL-NO-MATCH".to_string()
        } else {
            "LITD-SKILL-MATCHED".to_string()
        },
        intent: query.intent.clone(),
        query_fingerprint: shader_fingerprint(&format!(
            "{}|{}|{}|{}",
            query.query_id, query.intent, query.limit, query.requested_by
        )),
        requested_by: query.requested_by.clone(),
        limit: query.limit,
        result_count: results.len(),
        results,
    };
    validate_search_report(&report)?;
    Ok(report)
}

pub fn build_agent_skill_discovery_ledger(
    ledger_id: impl Into<String>,
    registry: AgentSkillRegistry,
    searches: Vec<AgentSkillSearchReport>,
) -> Result<AgentSkillDiscoveryLedger, EngineError> {
    validate_agent_skill_registry(&registry)?;
    for search in &searches {
        validate_search_report(search)?;
        if search.registry_id != registry.registry_id {
            return invalid_skill("ledger.search_registry", "search registry id mismatch");
        }
    }
    let ledger = AgentSkillDiscoveryLedger {
        schema: AGENT_SKILL_DISCOVERY_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        registry,
        search_count: searches.len(),
        searches,
    };
    validate_agent_skill_discovery_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_agent_skill_discovery_ledger(
    ledger: &AgentSkillDiscoveryLedger,
) -> Result<(), EngineError> {
    if ledger.schema != AGENT_SKILL_DISCOVERY_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.search_count != ledger.searches.len()
    {
        return invalid_skill("ledger", "schema, id, and search count must be valid");
    }
    validate_agent_skill_registry(&ledger.registry)?;
    let mut seen = BTreeSet::new();
    for search in &ledger.searches {
        validate_search_report(search)?;
        if !seen.insert(search.query_id.clone()) {
            return invalid_skill("ledger.duplicate_search", "duplicate search query id");
        }
    }
    Ok(())
}

pub fn validate_agent_skill_registry(registry: &AgentSkillRegistry) -> Result<(), EngineError> {
    if registry.schema != AGENT_SKILL_DISCOVERY_SCHEMA
        || registry.registry_id.trim().is_empty()
        || registry.skill_count == 0
        || registry.skill_count != registry.skills.len()
    {
        return invalid_skill("registry", "schema, id, and skill count must be valid");
    }
    let mut seen = BTreeSet::new();
    for skill in &registry.skills {
        validate_skill(skill)?;
        if !seen.insert(skill.skill_id.clone()) {
            return invalid_skill(
                "registry.duplicate",
                "registry contains duplicate skill ids",
            );
        }
    }
    Ok(())
}

fn validate_skill(skill: &AgentSkill) -> Result<(), EngineError> {
    if skill.skill_id.trim().is_empty()
        || skill.title.trim().is_empty()
        || skill.description.trim().is_empty()
        || skill.source_domain_id.trim().is_empty()
        || skill.intents.is_empty()
        || skill.tool_names.is_empty()
        || skill.readback_tool_names.is_empty()
        || skill.progressive_disclosure_ref.trim().is_empty()
    {
        return invalid_skill(
            "skill",
            "skill metadata, intents, tools, and readback are required",
        );
    }
    if !skill.skill_id.starts_with("skill.") {
        return invalid_skill("skill.id", "skill id must use skill.* namespace");
    }
    validate_list("skill.intents", &skill.intents)?;
    validate_list("skill.tool_names", &skill.tool_names)?;
    validate_list("skill.readback_tool_names", &skill.readback_tool_names)
}

fn validate_search_query(query: &AgentSkillSearchQuery) -> Result<(), EngineError> {
    if query.query_id.trim().is_empty()
        || query.intent.trim().is_empty()
        || query.requested_by.trim().is_empty()
        || query.limit == 0
    {
        return invalid_skill(
            "search.query",
            "query id, intent, requester, and limit are required",
        );
    }
    Ok(())
}

fn validate_search_report(report: &AgentSkillSearchReport) -> Result<(), EngineError> {
    if report.schema != AGENT_SKILL_DISCOVERY_SCHEMA
        || report.query_id.trim().is_empty()
        || report.registry_id.trim().is_empty()
        || report.code.trim().is_empty()
        || report.intent.trim().is_empty()
        || report.query_fingerprint.trim().is_empty()
        || report.result_count != report.results.len()
    {
        return invalid_skill("search.report", "search report fields must be valid");
    }
    match report.status {
        AgentSkillSearchStatus::Matched => {
            if report.code != "LITD-SKILL-MATCHED" || report.results.is_empty() {
                return invalid_skill("search.matched", "matched searches must return results");
            }
        }
        AgentSkillSearchStatus::NoMatch => {
            if report.code != "LITD-SKILL-NO-MATCH" || !report.results.is_empty() {
                return invalid_skill(
                    "search.no_match",
                    "no-match searches must return no results",
                );
            }
        }
    }
    Ok(())
}

fn score_skill(
    skill: &AgentSkill,
    query_terms: &BTreeMap<String, u32>,
) -> Option<AgentSkillSearchResult> {
    let title_terms = token_counts(&skill.title);
    let description_terms = token_counts(&skill.description);
    let intent_terms = token_counts(&skill.intents.join(" "));
    let tool_terms = token_counts(&skill.tool_names.join(" "));
    let mut matched = BTreeSet::new();
    let mut score = 0;
    for term in query_terms.keys() {
        if title_terms.contains_key(term) {
            score += 40;
            matched.insert(term.clone());
        }
        if intent_terms.contains_key(term) {
            score += 35;
            matched.insert(term.clone());
        }
        if description_terms.contains_key(term) {
            score += 20;
            matched.insert(term.clone());
        }
        if tool_terms.contains_key(term) {
            score += 10;
            matched.insert(term.clone());
        }
    }
    (score > 0).then(|| AgentSkillSearchResult {
        skill_id: skill.skill_id.clone(),
        title: skill.title.clone(),
        score,
        matched_terms: matched.into_iter().collect(),
        tool_names: skill.tool_names.clone(),
        readback_tool_names: skill.readback_tool_names.clone(),
        progressive_disclosure_ref: skill.progressive_disclosure_ref.clone(),
    })
}

fn merge_skill(existing: &mut AgentSkill, duplicate: AgentSkill) {
    existing.intents = merge_list(existing.intents.clone(), duplicate.intents);
    existing.tool_names = merge_list(existing.tool_names.clone(), duplicate.tool_names);
    existing.readback_tool_names = merge_list(
        existing.readback_tool_names.clone(),
        duplicate.readback_tool_names,
    );
}

fn merge_list(left: Vec<String>, right: Vec<String>) -> Vec<String> {
    left.into_iter()
        .chain(right)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn validate_list(field: &str, values: &[String]) -> Result<(), EngineError> {
    for value in values {
        if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
            return invalid_skill(field, "list values must be non-empty and single-line");
        }
    }
    Ok(())
}

fn token_counts(text: &str) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for token in text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
    {
        *counts.entry(token).or_insert(0) += 1;
    }
    counts
}

fn invalid_skill<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.agent_skills.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "agent_skills_tests.rs"]
mod tests;
