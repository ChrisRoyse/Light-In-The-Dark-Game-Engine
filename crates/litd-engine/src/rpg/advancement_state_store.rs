//! Canonical Calyx persistence for character advancement readbacks (#1750).
//!
//! The deterministic advancement engine remains side-effect free. This module is the explicit
//! persistence boundary: callers write the complete readback after applying a validated plan, and
//! later load the same row independently. No in-memory state is treated as durable truth.

use std::path::{Path, PathBuf};

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};

use super::{AdvancementReadback, RPG_ADVANCEMENT_SCHEMA};

const VAULT_ID: &str = "01ARZ3NDEKTSV4RRFF69RPGADV";
const VAULT_PURPOSE: &[u8] = b"litd-rpg-advancement-state-issue1750";
const COLLECTION: &str = "litd_rpg_advancement_state";
const COLUMN: &[u8] = b"advancement_readback_json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancementStateStoreReadback {
    pub vault_dir: PathBuf,
    pub character_id: String,
    pub bytes: usize,
    pub sha256: String,
    pub readback: AdvancementReadback,
}

pub fn persist_advancement_readback(
    vault_dir: impl AsRef<Path>,
    readback: &AdvancementReadback,
) -> Result<AdvancementStateStoreReadback, EngineError> {
    validate_readback(readback)?;
    let vault_dir = vault_dir.as_ref();
    let bytes = serde_json::to_vec(readback).map_err(|error| EngineError::SerializationFailed {
        format: "rpg-advancement-readback-json",
        reason: format!("serialize advancement readback: {error}"),
    })?;
    write_calyx_json_bytes(&spec(vault_dir, &readback.character_id), &bytes)
        .map_err(|error| store_error("write", error))?;
    read_persisted_advancement_readback(vault_dir, &readback.character_id)
}

pub fn read_persisted_advancement_readback(
    vault_dir: impl AsRef<Path>,
    character_id: &str,
) -> Result<AdvancementStateStoreReadback, EngineError> {
    if character_id.trim().is_empty() {
        return Err(store_error("character_id", "must be non-empty"));
    }
    let vault_dir = vault_dir.as_ref();
    let stored = read_calyx_json_bytes(&spec(vault_dir, character_id))
        .map_err(|error| store_error("read", error))?
        .ok_or_else(|| {
            store_error(
                "read",
                format!("missing advancement readback for character {character_id}"),
            )
        })?;
    let readback: AdvancementReadback = serde_json::from_slice(&stored.bytes).map_err(|error| {
        store_error("decode", format!("decode persisted advancement readback: {error}"))
    })?;
    validate_readback(&readback)?;
    if readback.character_id != character_id {
        return Err(store_error(
            "read.character_id",
            format!(
                "requested {character_id}, persisted {}",
                readback.character_id
            ),
        ));
    }
    Ok(AdvancementStateStoreReadback {
        vault_dir: vault_dir.to_path_buf(),
        character_id: character_id.to_string(),
        bytes: stored.bytes.len(),
        sha256: sha256_hex(&stored.bytes),
        readback,
    })
}

fn validate_readback(readback: &AdvancementReadback) -> Result<(), EngineError> {
    if readback.schema != RPG_ADVANCEMENT_SCHEMA {
        return Err(store_error(
            "schema",
            format!("expected {RPG_ADVANCEMENT_SCHEMA}"),
        ));
    }
    if readback.character_id.trim().is_empty() {
        return Err(store_error("character_id", "must be non-empty"));
    }
    Ok(())
}

fn spec<'a>(vault_dir: &'a Path, character_id: &'a str) -> CalyxJsonStoreSpec<'a> {
    CalyxJsonStoreSpec {
        vault_dir,
        vault_id: VAULT_ID,
        purpose: VAULT_PURPOSE,
        collection: COLLECTION,
        row_key: character_id.as_bytes(),
        column: COLUMN,
    }
}

fn store_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::RpgAdvancementInvalidConfig {
        field: format!("advancement_state_store.{field}"),
        reason: reason.into(),
    }
}
