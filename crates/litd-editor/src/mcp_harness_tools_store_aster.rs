use super::*;

pub fn read_mcp_harness_aster_ledger(
    aster_dir: impl AsRef<Path>,
    ledger_id: &str,
) -> Result<Option<HarnessAsterReadback>, EngineError> {
    validate_token("ledger_id", ledger_id)?;
    let aster_dir = aster_dir.as_ref();
    if !aster_dir.exists() {
        return Ok(None);
    }
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, HARNESS_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = ledger_row(ledger_id);
    let Some(bytes) = column
        .get(seq, &row, HARNESS_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
    else {
        return Ok(None);
    };
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(HarnessAsterReadback {
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        sha256: sha256_hex(&bytes),
        bytes: bytes.len() as u64,
    }))
}

pub(super) fn write_aster_ledger(
    dir: &Path,
    ledger_id: &str,
    bytes: &[u8],
) -> Result<HarnessAsterReadback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, HARNESS_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let row = ledger_row(ledger_id);
    let commit = column
        .put(&row, HARNESS_LEDGER_COLUMN, bytes)
        .map_err(|error| store_error("aster.put", error))?;
    let value = column
        .get(commit.seq, &row, HARNESS_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
        .ok_or_else(|| store_error("aster.get", "ledger missing immediately after put"))?;
    if value != bytes {
        return Err(store_error(
            "aster.readback",
            "ledger bytes differed after put",
        ));
    }
    let raw = vault
        .scan_cf_at(commit.seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    let cell_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.cell_key && value.as_slice() == bytes);
    let index_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.index_key && value.as_slice() == bytes);
    if !cell_ok || !index_ok {
        return Err(store_error(
            "aster.graph_cf",
            "Aster Graph CF did not contain both harness ledger rows",
        ));
    }
    Ok(HarnessAsterReadback {
        seq: commit.seq,
        graph_rows: raw.len(),
        cell_key: hex_preview(&commit.cell_key),
        index_key: hex_preview(&commit.index_key),
        sha256: sha256_hex(bytes),
        bytes: bytes.len() as u64,
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = HARNESS_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        HARNESS_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn ledger_row(ledger_id: &str) -> Vec<u8> {
    format!("{HARNESS_ROW_PREFIX}{ledger_id}").into_bytes()
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(18)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}
