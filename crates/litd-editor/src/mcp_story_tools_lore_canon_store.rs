use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
    calyx_semantic::ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT,
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use super::{LoreCanonQueryResult, QueryLoreCanonArgs, invalid_mcp_story, validate_path};

pub const LORE_CANON_QUERY_LEDGER_SCHEMA: &str = "litd.mcp.story_tools.lore_canon_query.v1";
const LORE_CANON_VAULT_ID: &str = "01HGZQ4C1V8X6M5A9T2K7D3P0B";
const LORE_CANON_VAULT_PURPOSE: &[u8] = b"litd-mcp-story-tools-issue1052";
const LORE_CANON_COLLECTION: &str = "litd_lore_canon_query";
const LORE_CANON_ROW: &[u8] = b"lore/canon/query/latest";
const LORE_CANON_COLUMN: &[u8] = b"query_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreCanonQueryLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub source_of_truth: String,
    pub state_sha256: String,
    pub semantic_vault_path: String,
    pub semantic_index_id: String,
    pub semantic_index_fingerprint: String,
    pub semantic_perspective_count: usize,
    pub content_record_count: usize,
    pub query_count: usize,
    pub total_hit_count: usize,
    pub contradiction_count: usize,
    pub out_of_distribution_count: usize,
    pub grounded_answer_count: usize,
    pub results: Vec<LoreCanonQueryResult>,
    pub report_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreCanonQueryLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub ledger: LoreCanonQueryLedger,
}

pub(super) fn build_ledger(
    args: &QueryLoreCanonArgs,
    source_of_truth: String,
    state_sha256: String,
    semantic_index_fingerprint: String,
    content_record_count: usize,
    results: Vec<LoreCanonQueryResult>,
) -> Result<LoreCanonQueryLedger, EngineError> {
    let mut ledger = LoreCanonQueryLedger {
        schema: LORE_CANON_QUERY_LEDGER_SCHEMA.to_string(),
        issue: 1052,
        ledger_id: args.query_ledger_id.clone(),
        request_id: args.request_id.clone(),
        requested_by: args.requested_by.clone(),
        source_of_truth,
        state_sha256,
        semantic_vault_path: args.semantic_vault_path.display().to_string(),
        semantic_index_id: args.semantic_index_id.clone(),
        semantic_index_fingerprint,
        semantic_perspective_count: ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT,
        content_record_count,
        query_count: results.len(),
        total_hit_count: results.iter().map(|result| result.hit_count).sum(),
        contradiction_count: findings(&results, "contradiction"),
        out_of_distribution_count: results
            .iter()
            .filter(|result| result.status == "out_of_distribution")
            .count()
            + findings(&results, "out_of_distribution"),
        grounded_answer_count: results
            .iter()
            .filter(|result| result.grounded_answer.is_some())
            .count(),
        results,
        report_fingerprint: String::new(),
    };
    ledger.report_fingerprint = ledger_fingerprint(&ledger)?;
    validate_lore_canon_query_ledger(&ledger)?;
    Ok(ledger)
}

pub(super) fn write_lore_canon_query_ledger(
    path: impl AsRef<Path>,
    ledger: LoreCanonQueryLedger,
) -> Result<LoreCanonQueryLedgerReadback, EngineError> {
    validate_lore_canon_query_ledger(&ledger)?;
    let path = path.as_ref();
    validate_path("query_ledger_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "lore-canon-query-ledger-json",
            reason: error.to_string(),
        })?;
    write_calyx_json_bytes(&ledger_spec(path), &bytes)
        .map_err(|error| invalid_mcp_story("lore_canon.ledger.write", error))?;
    let readback = read_lore_canon_query_ledger(path)?;
    if readback.ledger != ledger {
        return Err(invalid_mcp_story(
            "lore_canon.ledger.readback",
            "query ledger readback mismatch",
        ));
    }
    Ok(readback)
}

pub fn read_lore_canon_query_ledger(
    path: impl AsRef<Path>,
) -> Result<LoreCanonQueryLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("query_ledger_path", path)?;
    let readback = read_calyx_json_bytes(&ledger_spec(path))
        .map_err(|error| invalid_mcp_story("lore_canon.ledger.read", error))?
        .ok_or_else(|| invalid_mcp_story("lore_canon.ledger.read", "missing query ledger"))?;
    let bytes = readback.bytes;
    let ledger: LoreCanonQueryLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "lore-canon-query-ledger-json",
            reason: format!("parse {}: {error}", path.display()),
        })?;
    validate_lore_canon_query_ledger(&ledger)?;
    Ok(LoreCanonQueryLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        seq: readback.seq,
        graph_rows: readback.graph_rows,
        ledger,
    })
}

pub fn validate_lore_canon_query_ledger(ledger: &LoreCanonQueryLedger) -> Result<(), EngineError> {
    if ledger.schema != LORE_CANON_QUERY_LEDGER_SCHEMA
        || ledger.issue != 1052
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.query_count != ledger.results.len()
        || ledger.semantic_perspective_count != ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
        || ledger.content_record_count == 0
        || ledger.report_fingerprint != ledger_fingerprint(ledger)?
    {
        return Err(invalid_mcp_story(
            "lore_canon.ledger",
            "ledger schema, counts, or fingerprint are invalid",
        ));
    }
    Ok(())
}

fn ledger_fingerprint(ledger: &LoreCanonQueryLedger) -> Result<String, EngineError> {
    let mut clone = ledger.clone();
    clone.report_fingerprint.clear();
    serde_json::to_vec(&clone)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "lore-canon-query-ledger-json",
            reason: error.to_string(),
        })
}

fn findings(results: &[LoreCanonQueryResult], status: &str) -> usize {
    results
        .iter()
        .flat_map(|result| result.claim_findings.iter())
        .filter(|finding| finding.status == status)
        .count()
}

fn ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: LORE_CANON_VAULT_ID,
        purpose: LORE_CANON_VAULT_PURPOSE,
        collection: LORE_CANON_COLLECTION,
        row_key: LORE_CANON_ROW,
        column: LORE_CANON_COLUMN,
    }
}
