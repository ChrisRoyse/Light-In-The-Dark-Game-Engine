//! #853: persist the authored campaign to the Calyx Aster vault (single source of
//! truth — no side store) and read it back independently.
//!
//! The campaign row and an append-only write ledger both live in one Aster vault so
//! authoring is durable and auditable, mirroring the world-tools store pattern.

use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::history::stable_fingerprint;

use super::{Campaign, invalid_campaign};

pub const CAMPAIGN_STORE_LEDGER_SCHEMA: &str = "litd.campaign.store.ledger.v1";
const CAMPAIGN_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FD0";
const CAMPAIGN_VAULT_PURPOSE: &[u8] = b"litd-campaign-editor-issue853";
const CAMPAIGN_COLLECTION: &str = "litd_campaign_editor";
const CAMPAIGN_DOCUMENT_ROW: &[u8] = b"campaign/document/latest";
const CAMPAIGN_LEDGER_ROW: &[u8] = b"campaign/ledger/latest";
const CAMPAIGN_DOCUMENT_COLUMN: &[u8] = b"campaign_json";
const CAMPAIGN_LEDGER_COLUMN: &[u8] = b"ledger_json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub campaign_fingerprint: String,
    pub campaign: Campaign,
    pub chapter_count: usize,
    pub shared_datum_count: usize,
    pub hero_slot_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignStoreRecord {
    pub request_id: String,
    pub requested_by: String,
    pub action_summary: String,
    pub campaign_id: String,
    pub campaign_sha256: String,
    pub campaign_bytes: u64,
    pub campaign_fingerprint: String,
    pub chapter_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignStoreLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<CampaignStoreRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignStoreLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: CampaignStoreLedger,
}

pub fn write_campaign(
    path: impl AsRef<Path>,
    campaign: &Campaign,
) -> Result<CampaignReadback, EngineError> {
    campaign.validate()?;
    let path = path.as_ref();
    validate_path("campaign_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(campaign).map_err(|error| EngineError::SerializationFailed {
            format: "campaign-json",
            reason: format!("serialize campaign: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&document_spec(path), &bytes)
        .map_err(|error| store_error("document.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "document.write",
            "Aster campaign bytes did not match serialized bytes",
        ));
    }
    let readback = read_campaign(path)?;
    if readback.campaign != *campaign {
        return Err(store_error(
            "document.readback",
            format!("{} readback did not match written campaign", path.display()),
        ));
    }
    Ok(readback)
}

pub fn read_campaign(path: impl AsRef<Path>) -> Result<CampaignReadback, EngineError> {
    let path = path.as_ref();
    validate_path("campaign_path", path)?;
    let readback = read_calyx_json_bytes(&document_spec(path))
        .map_err(|error| store_error("document.read", error))?
        .ok_or_else(|| {
            store_error(
                "document.read",
                format!("missing Calyx Aster campaign row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let campaign: Campaign =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "campaign-json",
            reason: format!("parse campaign {}: {error}", path.display()),
        })?;
    campaign.validate()?;
    Ok(CampaignReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        campaign_fingerprint: stable_fingerprint(&campaign)?,
        chapter_count: campaign.chapters.len(),
        shared_datum_count: campaign.shared_object_data.len(),
        hero_slot_count: campaign.hero_carryover_slots.len(),
        campaign,
    })
}

pub fn write_campaign_ledger(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    record: CampaignStoreRecord,
) -> Result<CampaignStoreLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let ledger_id = ledger_id.into();
    let mut records =
        match read_calyx_json_bytes(&ledger_spec(path))
            .map_err(|error| store_error("ledger.read", error))?
        {
            Some(readback) => {
                let existing: CampaignStoreLedger = serde_json::from_slice(&readback.bytes)
                    .map_err(|error| EngineError::SerializationFailed {
                        format: "campaign-ledger-json",
                        reason: format!("parse campaign ledger {}: {error}", path.display()),
                    })?;
                validate_campaign_ledger(&existing)?;
                if existing.ledger_id != ledger_id {
                    return Err(store_error(
                        "ledger.ledger_id",
                        format!(
                            "ledger_id mismatch: existing {}, requested {ledger_id}",
                            existing.ledger_id
                        ),
                    ));
                }
                existing.records
            }
            None => Vec::new(),
        };
    records.push(record);
    let ledger = build_campaign_ledger(ledger_id, records)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "campaign-ledger-json",
            reason: format!("serialize campaign ledger: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&ledger_spec(path), &bytes)
        .map_err(|error| store_error("ledger.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "ledger.write",
            "Aster campaign ledger bytes did not match serialized bytes",
        ));
    }
    let readback = read_campaign_ledger(path)?;
    if readback.ledger != ledger {
        return Err(store_error(
            "ledger.readback",
            format!("{} readback did not match written ledger", path.display()),
        ));
    }
    Ok(readback)
}

pub fn read_campaign_ledger(
    path: impl AsRef<Path>,
) -> Result<CampaignStoreLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
        .ok_or_else(|| {
            store_error(
                "ledger.read",
                format!(
                    "missing Calyx Aster campaign ledger row at {}",
                    path.display()
                ),
            )
        })?;
    let bytes = readback.bytes;
    let ledger: CampaignStoreLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "campaign-ledger-json",
            reason: format!("parse campaign ledger {}: {error}", path.display()),
        })?;
    validate_campaign_ledger(&ledger)?;
    Ok(CampaignStoreLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
    })
}

pub fn validate_campaign_ledger(ledger: &CampaignStoreLedger) -> Result<(), EngineError> {
    if ledger.schema != CAMPAIGN_STORE_LEDGER_SCHEMA
        || ledger.issue != 853
        || ledger.record_count == 0
        || ledger.record_count != ledger.records.len()
    {
        return Err(store_error(
            "ledger",
            "schema, issue, and record count are invalid",
        ));
    }
    for record in &ledger.records {
        if record.request_id.trim().is_empty()
            || record.requested_by.trim().is_empty()
            || record.campaign_id.trim().is_empty()
            || record.campaign_sha256.len() != 64
            || record.campaign_bytes == 0
            || record.chapter_count == 0
        {
            return Err(store_error(
                "record",
                "campaign ledger record fields must be concrete",
            ));
        }
    }
    Ok(())
}

fn build_campaign_ledger(
    ledger_id: impl Into<String>,
    records: Vec<CampaignStoreRecord>,
) -> Result<CampaignStoreLedger, EngineError> {
    let ledger = CampaignStoreLedger {
        schema: CAMPAIGN_STORE_LEDGER_SCHEMA.to_string(),
        issue: 853,
        ledger_id: ledger_id.into(),
        source_of_truth: "Calyx Aster campaign document row + write ledger row".to_string(),
        record_count: records.len(),
        records,
    };
    validate_campaign_ledger(&ledger)?;
    Ok(ledger)
}

fn document_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: CAMPAIGN_VAULT_ID,
        purpose: CAMPAIGN_VAULT_PURPOSE,
        collection: CAMPAIGN_COLLECTION,
        row_key: CAMPAIGN_DOCUMENT_ROW,
        column: CAMPAIGN_DOCUMENT_COLUMN,
    }
}

fn ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: CAMPAIGN_VAULT_ID,
        purpose: CAMPAIGN_VAULT_PURPOSE,
        collection: CAMPAIGN_COLLECTION,
        row_key: CAMPAIGN_LEDGER_ROW,
        column: CAMPAIGN_LEDGER_COLUMN,
    }
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_campaign(
            format!("store.{field}"),
            "path must be non-empty",
        ));
    }
    Ok(())
}

fn store_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    invalid_campaign(format!("store.{}", field.into()), reason)
}
