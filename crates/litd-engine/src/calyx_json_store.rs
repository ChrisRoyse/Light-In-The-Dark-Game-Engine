use std::{
    fs,
    path::{Path, PathBuf},
};

use calyx_aster::{
    cf::ColumnFamily,
    plain_column::PlainColumn,
    vault::{AsterVault, encode::decode_write_batch},
    wal::replay_dir,
};
use calyx_core::{SystemClock, VaultId};
use litd_hash::sha256_hex;

use crate::determinism_boundary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalyxJsonStoreSpec<'a> {
    pub vault_dir: &'a Path,
    pub vault_id: &'a str,
    pub purpose: &'a [u8],
    pub collection: &'a str,
    pub row_key: &'a [u8],
    pub column: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalyxJsonStoreReadback {
    pub vault_dir: PathBuf,
    pub collection: String,
    pub row_key: String,
    pub column: String,
    pub bytes: Vec<u8>,
    pub byte_len: usize,
    pub sha256: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalyxJsonWalReadback {
    pub vault_dir: PathBuf,
    pub wal_dir: PathBuf,
    pub seq: u64,
    pub bytes: Vec<u8>,
    pub byte_len: usize,
    pub sha256: String,
    pub wal_row_count: usize,
    pub graph_row_count: usize,
    pub time_index_row_count: usize,
    pub graph_keys: Vec<String>,
    pub graph_key_sha256: Vec<String>,
    pub segment_path: PathBuf,
    pub segment_sha256: String,
    pub frame_sha256: String,
    pub start_offset: u64,
    pub end_offset: u64,
}

pub fn write_calyx_json_bytes(
    spec: &CalyxJsonStoreSpec<'_>,
    bytes: &[u8],
) -> Result<CalyxJsonStoreReadback, String> {
    determinism_boundary::record_calyx_access("calyx_json_store.write")
        .map_err(|error| error.to_string())?;
    validate_spec(spec)?;
    if bytes.is_empty() {
        return Err("refusing to write an empty Calyx JSON payload".to_string());
    }
    let vault = open_vault(spec)?;
    let column = PlainColumn::new(&vault, spec.collection)
        .map_err(|error| format!("open Aster plain column {}: {error}", spec.collection))?;
    let commit = column
        .put(spec.row_key, spec.column, bytes)
        .map_err(|error| format!("put Aster row: {error}"))?;
    let stored = column
        .get(commit.seq, spec.row_key, spec.column)
        .map_err(|error| format!("read Aster row after put: {error}"))?
        .ok_or_else(|| "Aster row missing immediately after put".to_string())?;
    if stored != bytes {
        return Err("Aster row bytes did not match immediately after put".to_string());
    }
    let graph = vault
        .scan_cf_at(commit.seq, ColumnFamily::Graph)
        .map_err(|error| format!("scan Aster Graph CF after put: {error}"))?;
    for expected_key in [&commit.cell_key, &commit.index_key] {
        if !graph
            .iter()
            .any(|(key, value)| key == expected_key && value == bytes)
        {
            return Err(
                "Aster Graph CF did not contain both row-major and column-major rows".to_string(),
            );
        }
    }
    Ok(readback(
        spec,
        stored,
        commit.seq,
        graph.len(),
        hex_preview(&commit.cell_key),
        hex_preview(&commit.index_key),
    ))
}

pub fn read_calyx_json_bytes(
    spec: &CalyxJsonStoreSpec<'_>,
) -> Result<Option<CalyxJsonStoreReadback>, String> {
    determinism_boundary::record_calyx_access("calyx_json_store.read")
        .map_err(|error| error.to_string())?;
    validate_spec(spec)?;
    if !spec.vault_dir.exists() {
        return Ok(None);
    }
    let vault = open_vault(spec)?;
    let column = PlainColumn::new(&vault, spec.collection)
        .map_err(|error| format!("open Aster plain column {}: {error}", spec.collection))?;
    let seq = vault.latest_seq();
    let Some(bytes) = column
        .get(seq, spec.row_key, spec.column)
        .map_err(|error| format!("read Aster row: {error}"))?
    else {
        return Ok(None);
    };
    let graph = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| format!("scan Aster Graph CF: {error}"))?;
    Ok(Some(readback(
        spec,
        bytes,
        seq,
        graph.len(),
        "loaded".to_string(),
        "loaded".to_string(),
    )))
}

/// Reads one JSON plain-column write from its physical WAL frame, without using
/// the Aster row-read API that produced `CalyxJsonStoreReadback`.
pub fn read_calyx_json_wal_write(
    vault_dir: &Path,
    seq: u64,
) -> Result<CalyxJsonWalReadback, String> {
    determinism_boundary::record_calyx_access("calyx_json_store.wal_read")
        .map_err(|error| error.to_string())?;
    if !vault_dir.is_dir() {
        return Err(format!(
            "{} is not a Calyx Aster vault directory",
            vault_dir.display()
        ));
    }
    let wal_dir = vault_dir.join("wal");
    if !wal_dir.is_dir() {
        return Err(format!(
            "Aster WAL directory is missing: {}",
            wal_dir.display()
        ));
    }
    let replay = replay_dir(&wal_dir)
        .map_err(|error| format!("replay physical Aster WAL {}: {error}", wal_dir.display()))?;
    if let Some(torn) = replay.torn_tail {
        return Err(format!(
            "physical Aster WAL has a torn tail at {}:{}: {}",
            torn.segment_path.display(),
            torn.offset,
            torn.message
        ));
    }
    let matching = replay
        .records
        .iter()
        .filter(|record| record.seq == seq)
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(format!(
            "expected exactly one physical Aster WAL record at seq {seq}, found {}",
            matching.len()
        ));
    }
    let record = matching[0];
    let rows = decode_write_batch(&record.payload)
        .map_err(|error| format!("decode physical Aster WAL write batch at seq {seq}: {error}"))?;
    let graph_rows = rows
        .iter()
        .filter(|row| row.cf == ColumnFamily::Graph)
        .collect::<Vec<_>>();
    let time_index_row_count = rows
        .iter()
        .filter(|row| row.cf == ColumnFamily::TimeIndex)
        .count();
    if rows.len() != 3 || graph_rows.len() != 2 || time_index_row_count != 1 {
        return Err(format!(
            "expected plain-column WAL write at seq {seq} to contain two Graph rows and one TimeIndex row, found total={} graph={} time_index={time_index_row_count}",
            rows.len(),
            graph_rows.len()
        ));
    }
    if graph_rows[0].key == graph_rows[1].key {
        return Err(format!(
            "plain-column WAL write at seq {seq} repeated one physical Graph key"
        ));
    }
    if graph_rows[0].value != graph_rows[1].value {
        return Err(format!(
            "plain-column WAL write at seq {seq} stored different row-major and column-major values"
        ));
    }
    let segment_bytes = fs::read(&record.segment_path).map_err(|error| {
        format!(
            "read physical Aster WAL segment {}: {error}",
            record.segment_path.display()
        )
    })?;
    let start = usize::try_from(record.start_offset).map_err(|_| {
        format!(
            "WAL frame start offset does not fit usize: {}",
            record.start_offset
        )
    })?;
    let end = usize::try_from(record.end_offset).map_err(|_| {
        format!(
            "WAL frame end offset does not fit usize: {}",
            record.end_offset
        )
    })?;
    let frame = segment_bytes.get(start..end).ok_or_else(|| {
        format!(
            "WAL frame offsets {start}..{end} exceed segment bytes {}",
            segment_bytes.len()
        )
    })?;
    if frame.is_empty() {
        return Err(format!("physical Aster WAL frame at seq {seq} is empty"));
    }
    let bytes = graph_rows[0].value.clone();
    Ok(CalyxJsonWalReadback {
        vault_dir: vault_dir.to_path_buf(),
        wal_dir,
        seq,
        byte_len: bytes.len(),
        sha256: sha256_hex(&bytes),
        bytes,
        wal_row_count: rows.len(),
        graph_row_count: graph_rows.len(),
        time_index_row_count,
        graph_keys: graph_rows.iter().map(|row| hex_preview(&row.key)).collect(),
        graph_key_sha256: graph_rows.iter().map(|row| sha256_hex(&row.key)).collect(),
        segment_path: record.segment_path.clone(),
        segment_sha256: sha256_hex(&segment_bytes),
        frame_sha256: sha256_hex(frame),
        start_offset: record.start_offset,
        end_offset: record.end_offset,
    })
}

fn validate_spec(spec: &CalyxJsonStoreSpec<'_>) -> Result<(), String> {
    if spec.vault_dir.as_os_str().is_empty() {
        return Err("Aster vault directory must be non-empty".to_string());
    }
    if spec.vault_dir.is_file() {
        return Err(format!(
            "{} is a file; expected a Calyx Aster vault directory",
            spec.vault_dir.display()
        ));
    }
    if spec.vault_id.trim().is_empty()
        || spec.collection.trim().is_empty()
        || spec.row_key.is_empty()
        || spec.column.is_empty()
        || spec.purpose.is_empty()
    {
        return Err(
            "Aster vault id, purpose, collection, row key, and column must be non-empty"
                .to_string(),
        );
    }
    Ok(())
}

fn open_vault(spec: &CalyxJsonStoreSpec<'_>) -> Result<AsterVault<SystemClock>, String> {
    let vault_id = spec
        .vault_id
        .parse::<VaultId>()
        .map_err(|error| format!("parse Aster vault id {}: {error}", spec.vault_id))?;
    AsterVault::open(
        spec.vault_dir,
        vault_id,
        spec.purpose.to_vec(),
        Default::default(),
    )
    .map_err(|error| format!("open Aster vault {}: {error}", spec.vault_dir.display()))
}

fn readback(
    spec: &CalyxJsonStoreSpec<'_>,
    bytes: Vec<u8>,
    seq: u64,
    graph_rows: usize,
    cell_key: String,
    index_key: String,
) -> CalyxJsonStoreReadback {
    CalyxJsonStoreReadback {
        vault_dir: spec.vault_dir.to_path_buf(),
        collection: spec.collection.to_string(),
        row_key: String::from_utf8_lossy(spec.row_key).to_string(),
        column: String::from_utf8_lossy(spec.column).to_string(),
        byte_len: bytes.len(),
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows,
        cell_key,
        index_key,
    }
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(24)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}
