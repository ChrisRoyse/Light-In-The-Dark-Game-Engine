use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use super::{
    LoreTemplateResult, LoreTemplateWardPolicy, invalid_mcp_story, support::validate_path,
};

pub const LORE_TEMPLATE_LEDGER_SCHEMA: &str = "litd.mcp.story_tools.lore_template.v1";
const LORE_TEMPLATE_VAULT_ID: &str = "01HGZQ4C1V8X6M5A9T2K7D3P1D";
const LORE_TEMPLATE_VAULT_PURPOSE: &[u8] = b"litd-mcp-story-tools-issue1054";
const LORE_TEMPLATE_COLLECTION: &str = "litd_lore_template";
const LORE_TEMPLATE_ROW: &[u8] = b"lore/template/latest";
const LORE_TEMPLATE_COLUMN: &[u8] = b"template_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreTemplateLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub source_of_truth: String,
    pub state_sha256_before: String,
    pub state_sha256_after: String,
    pub ward_policy: LoreTemplateWardPolicy,
    pub template_count: usize,
    pub generated_entry_count: usize,
    pub generated_link_count: usize,
    pub lowered_fingerprint_count: usize,
    pub template_kinds: Vec<String>,
    pub results: Vec<LoreTemplateResult>,
    pub report_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoreTemplateLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub ledger: LoreTemplateLedger,
}

pub(crate) struct TemplatesLedgerArgs {
    pub(crate) ledger_id: String,
    pub(crate) request_id: String,
    pub(crate) requested_by: String,
    pub(crate) source_of_truth: String,
    pub(crate) state_sha256_before: String,
    pub(crate) state_sha256_after: String,
    pub(crate) ward_policy: LoreTemplateWardPolicy,
    pub(crate) generated_link_count: usize,
    pub(crate) results: Vec<LoreTemplateResult>,
}

pub(super) fn build_ledger(args: TemplatesLedgerArgs) -> Result<LoreTemplateLedger, EngineError> {
    let TemplatesLedgerArgs {
        ledger_id,
        request_id,
        requested_by,
        source_of_truth,
        state_sha256_before,
        state_sha256_after,
        ward_policy,
        generated_link_count,
        results,
    } = args;
    let template_kinds = results
        .iter()
        .map(|result| result.kind.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let generated_entry_count = results
        .iter()
        .map(|result| result.generated_entry_ids.len())
        .sum();
    let lowered_fingerprint_count = results
        .iter()
        .map(|result| result.lowered_fingerprints.len())
        .sum();
    let mut ledger = LoreTemplateLedger {
        schema: LORE_TEMPLATE_LEDGER_SCHEMA.to_string(),
        issue: 1054,
        ledger_id,
        request_id,
        requested_by,
        source_of_truth,
        state_sha256_before,
        state_sha256_after,
        ward_policy,
        template_count: results.len(),
        generated_entry_count,
        generated_link_count,
        lowered_fingerprint_count,
        template_kinds,
        results,
        report_fingerprint: String::new(),
    };
    ledger.report_fingerprint = ledger_fingerprint(&ledger)?;
    validate_lore_template_ledger(&ledger)?;
    Ok(ledger)
}

pub(super) fn write_lore_template_ledger(
    path: impl AsRef<Path>,
    ledger: LoreTemplateLedger,
) -> Result<LoreTemplateLedgerReadback, EngineError> {
    validate_lore_template_ledger(&ledger)?;
    let path = path.as_ref();
    validate_path("template_ledger_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "lore-template-ledger-json",
            reason: error.to_string(),
        })?;
    write_calyx_json_bytes(&ledger_spec(path), &bytes)
        .map_err(|error| invalid_mcp_story("lore_template.ledger.write", error))?;
    let readback = read_lore_template_ledger(path)?;
    if readback.ledger != ledger {
        return Err(invalid_mcp_story(
            "lore_template.ledger.readback",
            "template ledger readback mismatch",
        ));
    }
    Ok(readback)
}

pub fn read_lore_template_ledger(
    path: impl AsRef<Path>,
) -> Result<LoreTemplateLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("template_ledger_path", path)?;
    let readback = read_calyx_json_bytes(&ledger_spec(path))
        .map_err(|error| invalid_mcp_story("lore_template.ledger.read", error))?
        .ok_or_else(|| invalid_mcp_story("lore_template.ledger.read", "missing template ledger"))?;
    let bytes = readback.bytes;
    let ledger: LoreTemplateLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "lore-template-ledger-json",
            reason: format!("parse {}: {error}", path.display()),
        })?;
    validate_lore_template_ledger(&ledger)?;
    Ok(LoreTemplateLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        seq: readback.seq,
        graph_rows: readback.graph_rows,
        ledger,
    })
}

pub fn validate_lore_template_ledger(ledger: &LoreTemplateLedger) -> Result<(), EngineError> {
    if ledger.schema != LORE_TEMPLATE_LEDGER_SCHEMA
        || ledger.issue != 1054
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.state_sha256_before.len() != 64
        || ledger.state_sha256_after.len() != 64
        || ledger.template_count != ledger.results.len()
        || ledger.generated_entry_count == 0
        || ledger.lowered_fingerprint_count != ledger.generated_entry_count
        || ledger.report_fingerprint != ledger_fingerprint(ledger)?
    {
        return Err(invalid_mcp_story(
            "lore_template.ledger",
            "ledger schema, counts, hashes, or fingerprint are invalid",
        ));
    }
    for result in &ledger.results {
        if result.status != "admitted"
            || !result.ward_decision.passed
            || result.generated_entry_ids.is_empty()
            || result.generated_entry_ids.len() != result.generated_titles.len()
            || result.generated_entry_ids.len() != result.lowered_fingerprints.len()
            || result
                .lowered_fingerprints
                .iter()
                .any(|fingerprint| fingerprint.len() != 64)
        {
            return Err(invalid_mcp_story(
                "lore_template.ledger.results",
                "template result counts or Ward decision are invalid",
            ));
        }
    }
    Ok(())
}

fn ledger_fingerprint(ledger: &LoreTemplateLedger) -> Result<String, EngineError> {
    let mut clone = ledger.clone();
    clone.report_fingerprint.clear();
    serde_json::to_vec(&clone)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "lore-template-ledger-json",
            reason: error.to_string(),
        })
}

fn ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: LORE_TEMPLATE_VAULT_ID,
        purpose: LORE_TEMPLATE_VAULT_PURPOSE,
        collection: LORE_TEMPLATE_COLLECTION,
        row_key: LORE_TEMPLATE_ROW,
        column: LORE_TEMPLATE_COLUMN,
    }
}
