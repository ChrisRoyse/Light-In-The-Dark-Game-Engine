use std::{env, fs, path::Path};

use calyx_aster::{
    cf::{ColumnFamily, base_key},
    vault::{AsterVault, VaultOptions, encode},
};
use calyx_core::{CxId, VaultId, VaultStore};
use litd_editor::history::stable_fingerprint;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(super) fn first_base_readback(
    vault_path: &Path,
    vault_id: &str,
    vault_name: &str,
    cx_id: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let vault = open_vault(vault_path, vault_id, vault_name)?;
    let cx_id = cx_id.parse::<CxId>()?;
    let key = base_key(cx_id);
    let bytes = vault
        .read_cf_at(vault.snapshot(), ColumnFamily::Base, &key)?
        .ok_or("first Base CF row missing")?;
    let decoded = encode::decode_constellation_base(&bytes)?;
    let loaded = vault.get(cx_id, vault.snapshot())?;
    Ok(json!({
        "cx_id": cx_id.to_string(),
        "key_hex": hex(&key),
        "bytes_len": bytes.len(),
        "bytes_sha256": sha256_hex(&bytes),
        "decoded_modality": decoded.modality,
        "decoded_anchor_count": decoded.anchors.len(),
        "decoded_slot_count": decoded.slots.len(),
        "loaded_matches_decoded": loaded == decoded,
    }))
}

pub(super) fn vault_state_for_happy(
    calyx_home: &Path,
    happy: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let vault_id = happy["vault_id"].as_str().ok_or("happy vault_id")?;
    let vault_name = happy["vault_name"].as_str().ok_or("happy vault_name")?;
    vault_state(
        &calyx_home.join("vaults").join(vault_id),
        vault_id,
        vault_name,
    )
}

pub(super) fn vault_state(
    vault_path: &Path,
    vault_id: &str,
    vault_name: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let vault = open_vault(vault_path, vault_id, vault_name)?;
    Ok(json!({
        "base": cf_summary(&vault, ColumnFamily::Base)?,
        "assay": cf_summary(&vault, ColumnFamily::Assay)?,
        "kernel": cf_summary(&vault, ColumnFamily::Kernel)?,
        "guard": cf_summary(&vault, ColumnFamily::Guard)?,
        "ledger": cf_summary(&vault, ColumnFamily::Ledger)?,
        "latest_seq": vault.latest_seq(),
    }))
}

fn open_vault(
    vault_path: &Path,
    vault_id: &str,
    vault_name: &str,
) -> Result<AsterVault, Box<dyn std::error::Error>> {
    let vault_id = vault_id.parse::<VaultId>()?;
    Ok(AsterVault::open(
        vault_path,
        vault_id,
        format!("calyx-cli-vault:{vault_id}:{vault_name}").into_bytes(),
        VaultOptions::default(),
    )?)
}

fn cf_summary(vault: &AsterVault, cf: ColumnFamily) -> Result<Value, Box<dyn std::error::Error>> {
    let rows = vault.scan_cf_at(vault.snapshot(), cf)?;
    let keys = rows.iter().map(|(key, _)| hex(key)).collect::<Vec<_>>();
    let value_hashes = rows
        .iter()
        .map(|(_, value)| sha256_hex(value))
        .collect::<Vec<_>>();
    Ok(json!({
        "count": rows.len(),
        "keys": keys,
        "value_sha256": value_hashes,
    }))
}

pub(super) fn summarize_value(value: &Value) -> Value {
    json!({
        "keys": value.as_object().map(|map| map.keys().cloned().collect::<Vec<_>>()),
        "fingerprint": stable_fingerprint(value).ok(),
    })
}

pub(super) fn write_json<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

pub(super) fn read_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn sha256_hex(bytes: &[u8]) -> String {
    litd_hash::sha256_hex(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) struct CalyxHomeScope {
    old: Option<std::ffi::OsString>,
}

impl CalyxHomeScope {
    pub(super) fn set(home: &Path) -> Self {
        let old = env::var_os("CALYX_HOME");
        unsafe {
            env::set_var("CALYX_HOME", home);
        }
        Self { old }
    }
}

impl Drop for CalyxHomeScope {
    fn drop(&mut self) {
        unsafe {
            match &self.old {
                Some(value) => env::set_var("CALYX_HOME", value),
                None => env::remove_var("CALYX_HOME"),
            }
        }
    }
}
