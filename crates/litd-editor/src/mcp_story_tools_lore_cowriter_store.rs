use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use super::{
    LoreCowriterDraftResult, LoreCowriterWardPolicy, invalid_mcp_story, support::validate_path,
};

pub const LORE_COWRITER_LEDGER_SCHEMA: &str = "litd.mcp.story_tools.lore_cowriter.v1";
const LORE_COWRITER_VAULT_ID: &str = "01HGZQ4C1V8X6M5A9T2K7D3P1C";
const LORE_COWRITER_VAULT_PURPOSE: &[u8] = b"litd-mcp-story-tools-issue1053";
const LORE_COWRITER_COLLECTION: &str = "litd_lore_cowriter";
const LORE_COWRITER_ROW: &[u8] = b"lore/cowriter/latest";
const LORE_COWRITER_COLUMN: &[u8] = b"cowriter_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreCowriterLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub source_of_truth: String,
    pub state_sha256_before: String,
    pub state_sha256_after: String,
    pub ward_policy: LoreCowriterWardPolicy,
    pub draft_count: usize,
    pub admitted_count: usize,
    pub generated_entry_count: usize,
    pub source_anchor_count: usize,
    pub matched_term_count: usize,
    pub lowered_fingerprint_count: usize,
    pub results: Vec<LoreCowriterDraftResult>,
    pub report_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreCowriterLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub ledger: LoreCowriterLedger,
}

pub(crate) struct CowriterLedgerArgs {
    pub(crate) ledger_id: String,
    pub(crate) request_id: String,
    pub(crate) requested_by: String,
    pub(crate) source_of_truth: String,
    pub(crate) state_sha256_before: String,
    pub(crate) state_sha256_after: String,
    pub(crate) ward_policy: LoreCowriterWardPolicy,
    pub(crate) results: Vec<LoreCowriterDraftResult>,
}

pub(super) fn build_ledger(args: CowriterLedgerArgs) -> Result<LoreCowriterLedger, EngineError> {
    let CowriterLedgerArgs {
        ledger_id,
        request_id,
        requested_by,
        source_of_truth,
        state_sha256_before,
        state_sha256_after,
        ward_policy,
        results,
    } = args;
    let source_anchor_count = results
        .iter()
        .flat_map(|result| result.grounding.matched_anchor_ids.iter())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let matched_term_count = results
        .iter()
        .flat_map(|result| result.grounding.matched_terms.iter())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let mut ledger = LoreCowriterLedger {
        schema: LORE_COWRITER_LEDGER_SCHEMA.to_string(),
        issue: 1053,
        ledger_id,
        request_id,
        requested_by,
        source_of_truth,
        state_sha256_before,
        state_sha256_after,
        ward_policy,
        draft_count: results.len(),
        admitted_count: results
            .iter()
            .filter(|result| result.status == "admitted")
            .count(),
        generated_entry_count: results.len(),
        source_anchor_count,
        matched_term_count,
        lowered_fingerprint_count: results
            .iter()
            .filter(|result| !result.lowered_fingerprint.is_empty())
            .count(),
        results,
        report_fingerprint: String::new(),
    };
    ledger.report_fingerprint = ledger_fingerprint(&ledger)?;
    validate_lore_cowriter_ledger(&ledger)?;
    Ok(ledger)
}

pub(super) fn write_lore_cowriter_ledger(
    path: impl AsRef<Path>,
    ledger: LoreCowriterLedger,
) -> Result<LoreCowriterLedgerReadback, EngineError> {
    validate_lore_cowriter_ledger(&ledger)?;
    let path = path.as_ref();
    validate_path("cowriter_ledger_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "lore-cowriter-ledger-json",
            reason: error.to_string(),
        })?;
    write_calyx_json_bytes(&ledger_spec(path), &bytes)
        .map_err(|error| invalid_mcp_story("lore_cowriter.ledger.write", error))?;
    let readback = read_lore_cowriter_ledger(path)?;
    if readback.ledger != ledger {
        return Err(invalid_mcp_story(
            "lore_cowriter.ledger.readback",
            "co-writer ledger readback mismatch",
        ));
    }
    Ok(readback)
}

pub fn read_lore_cowriter_ledger(
    path: impl AsRef<Path>,
) -> Result<LoreCowriterLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("cowriter_ledger_path", path)?;
    let readback = read_calyx_json_bytes(&ledger_spec(path))
        .map_err(|error| invalid_mcp_story("lore_cowriter.ledger.read", error))?
        .ok_or_else(|| {
            invalid_mcp_story("lore_cowriter.ledger.read", "missing co-writer ledger")
        })?;
    let bytes = readback.bytes;
    let ledger: LoreCowriterLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "lore-cowriter-ledger-json",
            reason: format!("parse {}: {error}", path.display()),
        })?;
    validate_lore_cowriter_ledger(&ledger)?;
    Ok(LoreCowriterLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        seq: readback.seq,
        graph_rows: readback.graph_rows,
        ledger,
    })
}

pub fn validate_lore_cowriter_ledger(ledger: &LoreCowriterLedger) -> Result<(), EngineError> {
    if ledger.schema != LORE_COWRITER_LEDGER_SCHEMA
        || ledger.issue != 1053
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.draft_count != ledger.results.len()
        || ledger.admitted_count != ledger.results.len()
        || ledger.generated_entry_count != ledger.results.len()
        || ledger.lowered_fingerprint_count != ledger.results.len()
        || ledger.source_anchor_count == 0
        || ledger.matched_term_count == 0
        || ledger.report_fingerprint != ledger_fingerprint(ledger)?
    {
        return Err(invalid_mcp_story(
            "lore_cowriter.ledger",
            "ledger schema, counts, or fingerprint are invalid",
        ));
    }
    Ok(())
}

fn ledger_fingerprint(ledger: &LoreCowriterLedger) -> Result<String, EngineError> {
    let mut clone = ledger.clone();
    clone.report_fingerprint.clear();
    serde_json::to_vec(&clone)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "lore-cowriter-ledger-json",
            reason: error.to_string(),
        })
}

fn ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: LORE_COWRITER_VAULT_ID,
        purpose: LORE_COWRITER_VAULT_PURPOSE,
        collection: LORE_COWRITER_COLLECTION,
        row_key: LORE_COWRITER_ROW,
        column: LORE_COWRITER_COLUMN,
    }
}
