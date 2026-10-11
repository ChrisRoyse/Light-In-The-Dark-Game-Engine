//! The itemsearch DB ingestion pipeline entry points (#1688).
//!
//! `parse_itemsearch_dump` turns a captured itemsearch dump into validated
//! [`ItemSpec`]s plus an itemized failure report. `ingest_itemsearch_dump`
//! carries the parsed tranche through the SAME shared authored-content valve the
//! hand-authored pilots use (`persist_and_lower_rpg_authored_content`) and reads
//! every persisted row back out of the Calyx store, returning count parity as
//! the FSV evidence: `total_records == parsed + failed`, and
//! `parsed == persisted_rows == readback_rows`.
//!
//! Engineered for the full 3,197-item corpus; PROVEN on the real captured
//! tranche (`item_ingest_tests.rs`). No silent drops: an unparseable record is
//! always an itemized [`ItemIngestFailure`], never a vanished row.

use crate::EngineError;
use crate::calyx_content::EngineCalyxContentStore;

use super::authored_content::persist_and_lower_rpg_authored_content;
use super::item_ingest_parse::parse_dump;
use super::item_ingest_types::{
    ItemIngestParse, ItemIngestReadback, RPG_ITEM_INGEST_READBACK_SCHEMA,
};
use super::item_specs_content::item_spec_content_records;

/// Parse a captured itemsearch dump into specs + an itemized failure report.
/// Pure (no persistence); every record is accounted for.
pub fn parse_itemsearch_dump(dump: &str) -> ItemIngestParse {
    let (specs, failures, id_collisions_resolved) = parse_dump(dump);
    let total_records = specs.len() + failures.len();
    ItemIngestParse {
        total_records,
        specs,
        failures,
        id_collisions_resolved,
    }
}

/// Ingest a captured itemsearch dump end-to-end: parse -> emit editor-authored
/// content records -> persist + lower through the shared valve -> read every row
/// back. Returns the count-parity readback (the FSV evidence). Fails LOUD if the
/// persisted row count or the independent readback count does not match the
/// parsed spec count (a store-side loss would otherwise be silent).
pub fn ingest_itemsearch_dump(
    store: &EngineCalyxContentStore,
    dump: &str,
) -> Result<ItemIngestReadback, EngineError> {
    let parse = parse_itemsearch_dump(dump);

    // Emit one editor-authored content record per parsed spec.
    let mut records = Vec::with_capacity(parse.specs.len());
    for spec in &parse.specs {
        records.extend(item_spec_content_records(spec)?);
    }

    // Persist + lower the whole tranche through the ONE shared valve.
    let lowering = persist_and_lower_rpg_authored_content(store, &records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "item_ingest.persist_and_lower".to_string(),
            reason: error.to_string(),
        }
    })?;

    // Independent readback: fetch every row back out of the store.
    let mut readback_rows = 0usize;
    for record in &records {
        let row = store
            .get_content_for_editor(record.kind, &record.id)
            .map_err(|error| EngineError::RpgStatsInvalidConfig {
                field: "item_ingest.readback".to_string(),
                reason: format!("row {} did not read back: {error}", record.id),
            })?;
        if row.id != record.id {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: "item_ingest.readback".to_string(),
                reason: format!("readback id {} != persisted id {}", row.id, record.id),
            });
        }
        readback_rows += 1;
    }

    let parsed = parse.specs.len();
    let persisted_rows = lowering.row_count;
    // Loud count-parity guard: a mismatch is a hard fail, not a warning.
    if persisted_rows != parsed || readback_rows != parsed {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "item_ingest.count_parity".to_string(),
            reason: format!(
                "count parity broken: parsed={parsed} persisted={persisted_rows} \
                 readback={readback_rows}"
            ),
        });
    }

    Ok(ItemIngestReadback {
        schema: RPG_ITEM_INGEST_READBACK_SCHEMA.to_string(),
        total_records: parse.total_records,
        parsed,
        failed: parse.failures.len(),
        persisted_rows,
        readback_rows,
        id_collisions_resolved: parse.id_collisions_resolved,
        package_fingerprint: lowering.package_fingerprint,
        failures: parse.failures,
    })
}
