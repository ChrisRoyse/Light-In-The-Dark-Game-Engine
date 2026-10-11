//! Readback + failure-report types for the itemsearch DB ingestion pipeline
//! (#1688).
//!
//! Every record in a captured itemsearch dump is accounted for: it either
//! becomes a validated [`ItemSpec`] (persisted + read back through Calyx) or a
//! LOUD, itemized [`ItemIngestFailure`] naming the offending record and the
//! reason. There is no silent-drop path -- `total_records == parsed + failed`
//! is the anti-silent-drop invariant the FSV asserts.

use serde::{Deserialize, Serialize};

use super::item_specs_types::ItemSpec;

/// One record that could NOT be ingested, itemized with its source position,
/// the item's short-description (so a human can find it in the dump), and the
/// structured reason. No record is ever dropped silently: it is either parsed
/// or it lands here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemIngestFailure {
    /// 1-based index of the record within the dump (source-of-truth position).
    pub record_index: usize,
    /// The item's raw `Name:` short-description, or `<unnamed record N>` when the
    /// name itself could not be read.
    pub item_name: String,
    /// The structured failure reason (unknown material/flag, missing field,
    /// racial slot without race gating, unknown weapon noun, ...).
    pub reason: String,
}

/// The outcome of parsing a captured itemsearch dump into item specs. Carries
/// the successfully-parsed specs, the itemized failures, and the total record
/// count so the pipeline can assert `total == specs + failures`.
#[derive(Debug, Clone)]
pub struct ItemIngestParse {
    /// Every `Name:`-delimited record found in the dump.
    pub total_records: usize,
    /// The specs that parsed AND validated cleanly.
    pub specs: Vec<ItemSpec>,
    /// The records that failed, each itemized with a reason (loud, never silent).
    pub failures: Vec<ItemIngestFailure>,
    /// Number of item ids that collided on their name-slug and were made unique
    /// (deterministic `_2`, `_3`, ... suffix). Reported, never hidden.
    pub id_collisions_resolved: usize,
}

impl ItemIngestParse {
    /// The anti-silent-drop invariant: every record is either a spec or a
    /// failure. Returns false if any record vanished.
    pub fn accounts_for_every_record(&self) -> bool {
        self.total_records == self.specs.len() + self.failures.len()
    }
}

/// The full ingestion readback: parse accounting PLUS the Calyx persistence /
/// readback counts. `persisted_rows == readback_rows == parsed` proves the
/// tranche round-tripped through the store with zero loss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemIngestReadback {
    pub schema: String,
    /// Every record in the dump.
    pub total_records: usize,
    /// Records that became validated specs.
    pub parsed: usize,
    /// Records that failed loud (itemized in `failures`).
    pub failed: usize,
    /// Rows written into the Calyx content store.
    pub persisted_rows: usize,
    /// Rows read back out of the store (independent projection).
    pub readback_rows: usize,
    /// Name-slug collisions that were deterministically de-duplicated.
    pub id_collisions_resolved: usize,
    /// The deterministic package fingerprint over the persisted tranche.
    pub package_fingerprint: String,
    /// The itemized failure report (loud; one entry per unparseable record).
    pub failures: Vec<ItemIngestFailure>,
}

pub const RPG_ITEM_INGEST_READBACK_SCHEMA: &str = "litd.rpg.item_ingest_readback.v1";
