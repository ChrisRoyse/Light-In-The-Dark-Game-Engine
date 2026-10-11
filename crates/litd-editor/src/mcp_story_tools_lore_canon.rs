use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use litd_engine::{EngineError, calyx_semantic::EngineCalyxSemanticHit};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    LoreCanonClaim, LoreCanonQuery, LoreCanonQueryKind, McpStoryToolCallOutput, QueryLoreCanonArgs,
    invalid_mcp_story,
    mcp_story_tools_store::{McpStoryState, read_mcp_story_state},
};

#[path = "mcp_story_tools_lore_canon_checks.rs"]
mod checks;
#[path = "mcp_story_tools_lore_canon_records.rs"]
mod records;
#[path = "mcp_story_tools_lore_canon_store.rs"]
mod store;

pub use store::{
    LORE_CANON_QUERY_LEDGER_SCHEMA, LoreCanonQueryLedger, LoreCanonQueryLedgerReadback,
    read_lore_canon_query_ledger, validate_lore_canon_query_ledger,
};

const MAX_QUERY_COUNT: usize = 64;
const MAX_QUERY_LIMIT: usize = 32;
const MAX_CLAIM_COUNT: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreCanonQueryResult {
    pub query_id: String,
    pub kind: LoreCanonQueryKind,
    pub text: String,
    pub status: String,
    pub hit_count: usize,
    pub hits: Vec<LoreCanonHit>,
    pub claim_findings: Vec<LoreCanonClaimFinding>,
    pub grounded_answer: Option<LoreGroundedAnswer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreCanonHit {
    pub record_type: String,
    pub content_id: String,
    pub source_id: String,
    pub title: String,
    pub excerpt: String,
    pub score: u32,
    pub matched_terms: Vec<String>,
    pub source_refs: Vec<String>,
    pub linked_lore_entry_ids: Vec<String>,
    pub provenance_refs: Vec<litd_engine::calyx_semantic::EngineCalyxProvenanceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreCanonClaimFinding {
    pub claim_id: String,
    pub status: String,
    pub subject_entry_id: String,
    pub evidence_refs: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreGroundedAnswer {
    pub answer_text: String,
    pub source_ids: Vec<String>,
    pub evidence_refs: Vec<String>,
}

pub(super) fn query_lore_canon_tool(
    route: crate::mcp_tools::McpDomainTool,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    super::validate_mutating_route(&route)?;
    let args: QueryLoreCanonArgs = super::parse_args(super::QUERY_LORE_CANON_TOOL, arguments)?;
    // #1518: confine the state read + semantic-vault read + query-ledger write sinks to the
    // project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("semantic_vault_path", args.semantic_vault_path.as_path()),
        ("query_ledger_path", args.query_ledger_path.as_path()),
    ])?;
    validate_single_line("lore_canon.request_id", &args.request_id)?;
    validate_single_line("lore_canon.requested_by", &args.requested_by)?;
    validate_args(&args)?;
    let state_readback = read_mcp_story_state(&args.state_path)?;
    if state_readback.state.state_id != args.state_id {
        return Err(invalid_mcp_story(
            "lore_canon.state_id",
            "state_id mismatch",
        ));
    }
    if state_readback.state.lore_entries.is_empty() {
        return Err(invalid_mcp_story(
            "lore_canon.entries",
            "at least one lore entry is required",
        ));
    }
    let source_of_truth = format!(
        "Calyx Aster story state {} sha256 {}",
        state_readback.path, state_readback.sha256
    );
    let (semantic, catalog) = records::build_semantic_index(&args, &state_readback.state)?;
    let results = args
        .queries
        .iter()
        .map(|query| run_query(query, &state_readback.state, &semantic, &catalog))
        .collect::<Result<Vec<_>, _>>()?;
    let ledger = store::build_ledger(
        &args,
        source_of_truth,
        state_readback.sha256.clone(),
        semantic.index_fingerprint.clone(),
        catalog.len(),
        results,
    )?;
    let query_readback = store::write_lore_canon_query_ledger(&args.query_ledger_path, ledger)?;
    Ok(super::output(
        route,
        "MCP story/lore state JSON + Calyx Aster lore canon query ledger + engine semantic index",
        json!({ "lore_canon_query_readback": query_readback }),
        Some(state_readback),
        None,
    ))
}

fn run_query(
    query: &LoreCanonQuery,
    state: &McpStoryState,
    index: &litd_engine::calyx_semantic::EngineCalyxSemanticIndex,
    catalog: &BTreeMap<String, records::CanonRecord>,
) -> Result<LoreCanonQueryResult, EngineError> {
    let semantic = litd_engine::calyx_semantic::query_engine_calyx_semantic_index(
        index,
        &query.text,
        query.limit,
    )
    .map_err(|error| invalid_mcp_story("lore_canon.semantic.query", error.to_string()))?;
    let hits = semantic
        .hits
        .iter()
        .map(|hit| canon_hit(hit, catalog))
        .collect::<Result<Vec<_>, _>>()?;
    let claim_findings = query
        .proposed_claims
        .iter()
        .map(|claim| checks::check_claim(claim, state))
        .collect::<Vec<_>>();
    let contradiction_count = claim_findings
        .iter()
        .filter(|finding| finding.status == "contradiction")
        .count();
    let ood_count = claim_findings
        .iter()
        .filter(|finding| finding.status == "out_of_distribution")
        .count();
    let grounded_answer = checks::grounded_answer(query, &hits, contradiction_count, ood_count);
    let status = checks::query_status(query.kind, hits.len(), contradiction_count, ood_count);
    Ok(LoreCanonQueryResult {
        query_id: query.query_id.clone(),
        kind: query.kind,
        text: query.text.trim().to_string(),
        status,
        hit_count: hits.len(),
        hits,
        claim_findings,
        grounded_answer,
    })
}

fn canon_hit(
    hit: &EngineCalyxSemanticHit,
    catalog: &BTreeMap<String, records::CanonRecord>,
) -> Result<LoreCanonHit, EngineError> {
    let canon = catalog.get(&hit.id).ok_or_else(|| {
        invalid_mcp_story(
            "lore_canon.semantic.hit",
            format!("semantic hit {} missing canon record", hit.id),
        )
    })?;
    Ok(LoreCanonHit {
        record_type: canon.record_type.clone(),
        content_id: canon.content_id.clone(),
        source_id: canon.source_id.clone(),
        title: canon.title.clone(),
        excerpt: canon.excerpt.clone(),
        score: hit.score,
        matched_terms: hit.matched_terms.clone(),
        source_refs: canon.source_refs.clone(),
        linked_lore_entry_ids: canon.linked_lore_entry_ids.clone(),
        provenance_refs: hit.provenance_refs.clone(),
    })
}

fn validate_args(args: &QueryLoreCanonArgs) -> Result<(), EngineError> {
    validate_path("semantic_vault_path", &args.semantic_vault_path)?;
    validate_path("query_ledger_path", &args.query_ledger_path)?;
    validate_symbol("semantic_index_id", &args.semantic_index_id)?;
    if !crate::row_safe_ledger_id::is_row_safe_ledger_id(&args.query_ledger_id) {
        return Err(invalid_mcp_story(
            "lore_canon.query_ledger_id",
            "query ledger id must be row-safe",
        ));
    }
    if args.queries.is_empty() || args.queries.len() > MAX_QUERY_COUNT {
        return Err(invalid_mcp_story(
            "lore_canon.queries",
            format!("query count must be in 1..={MAX_QUERY_COUNT}"),
        ));
    }
    let mut query_ids = BTreeSet::new();
    for query in &args.queries {
        validate_symbol("lore_canon.query_id", &query.query_id)?;
        validate_single_line("lore_canon.query.text", &query.text)?;
        if query.limit == 0 || query.limit > MAX_QUERY_LIMIT {
            return Err(invalid_mcp_story(
                "lore_canon.query.limit",
                format!("query limit must be in 1..={MAX_QUERY_LIMIT}"),
            ));
        }
        if !query_ids.insert(query.query_id.as_str()) {
            return Err(invalid_mcp_story(
                "lore_canon.query_id",
                format!("duplicate query {}", query.query_id),
            ));
        }
        validate_claims(&query.proposed_claims)?;
    }
    Ok(())
}

fn validate_claims(claims: &[LoreCanonClaim]) -> Result<(), EngineError> {
    if claims.len() > MAX_CLAIM_COUNT {
        return Err(invalid_mcp_story(
            "lore_canon.claims",
            format!("claim count must be <= {MAX_CLAIM_COUNT}"),
        ));
    }
    let mut claim_ids = BTreeSet::new();
    for claim in claims {
        validate_symbol("lore_canon.claim_id", &claim.claim_id)?;
        validate_symbol("lore_canon.claim.subject_entry_id", &claim.subject_entry_id)?;
        validate_single_line("lore_canon.claim.source_ref", &claim.source_ref)?;
        if !claim_ids.insert(claim.claim_id.as_str()) {
            return Err(invalid_mcp_story(
                "lore_canon.claim_id",
                format!("duplicate claim {}", claim.claim_id),
            ));
        }
        let has_link = claim.relation.is_some() && claim.target_entry_id.is_some();
        let has_metadata = claim.metadata_key.is_some() && claim.metadata_value.is_some();
        if !has_link && !has_metadata {
            return Err(invalid_mcp_story(
                "lore_canon.claim",
                "claim must include relation+target or metadata_key+metadata_value",
            ));
        }
        for (field, value) in [
            ("lore_canon.claim.relation", claim.relation.as_ref()),
            (
                "lore_canon.claim.target_entry_id",
                claim.target_entry_id.as_ref(),
            ),
            ("lore_canon.claim.metadata_key", claim.metadata_key.as_ref()),
        ] {
            if let Some(value) = value {
                validate_symbol(field, value)?;
            }
        }
        if let Some(value) = &claim.metadata_value {
            validate_single_line("lore_canon.claim.metadata_value", value)?;
        }
    }
    Ok(())
}

fn refs(prefix: &str, id: &str, source_refs: &[String]) -> Vec<String> {
    let mut out = vec![format!("{prefix}/{id}")];
    out.extend(source_refs.iter().cloned());
    out
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_mcp_story(field, "path must be non-empty"));
    }
    if path.is_file() {
        return Err(invalid_mcp_story(
            field,
            format!(
                "{} is a file; expected an Aster vault directory",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 4096 || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_mcp_story(
            field,
            "value must be non-empty bounded single-line text",
        ));
    }
    Ok(())
}

fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_mcp_story(field, "symbol cannot be empty"));
    };
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(invalid_mcp_story(
            field,
            "symbol must be an ASCII identifier",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "mcp_story_tools_lore_canon_tests.rs"]
mod tests;
