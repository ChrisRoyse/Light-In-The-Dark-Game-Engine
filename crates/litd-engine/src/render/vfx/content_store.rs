use std::path::{Path, PathBuf};

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
    render::vfx::{
        VFX_EFFECT_SCHEMA_VERSION, VfxEffectAsset, VfxValidationReport, effect_to_json_bytes,
        invalid_vfx, invalid_vfx_error, validate_effect,
    },
};
use litd_hash::sha256_hex;

pub const VFX_EFFECT_MEDIA_TYPE: &str = "application/vnd.litd.vfx.effect.v1+json";
pub const VFX_EFFECT_STORE_PREFIX: &str = "particles";
pub const VFX_EFFECT_DIGEST_ALGORITHM: &str = "sha256";
const VFX_CONTENT_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCD";
const VFX_CONTENT_VAULT_PURPOSE: &[u8] = b"litd-engine-vfx-content-issue1363";
const VFX_CONTENT_COLLECTION: &str = "litd_engine_vfx_content";
const VFX_CONTENT_COLUMN: &[u8] = b"effect_json";
const VFX_CONTENT_COUNT_LIMIT: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfxEffectDescriptor {
    pub media_type: String,
    pub effect_id: String,
    pub schema_version: u32,
    pub digest: String,
    pub size: u64,
    pub store_relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxEffectStoreReadback {
    pub descriptor: VfxEffectDescriptor,
    pub object_path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub effect: VfxEffectAsset,
    pub validation: VfxValidationReport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxEffectStoreWriteback {
    pub descriptor: VfxEffectDescriptor,
    pub object_path: PathBuf,
    pub deduplicated: bool,
    pub readback: VfxEffectStoreReadback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfxEffectStoreCount {
    pub vault_dir: PathBuf,
    pub seq: u64,
    pub graph_rows: usize,
    pub effect_count: usize,
    pub row_keys: Vec<String>,
}

impl VfxEffectDescriptor {
    pub fn new(
        effect_id: impl Into<String>,
        schema_version: u32,
        digest: impl Into<String>,
        size: u64,
    ) -> Result<Self, EngineError> {
        let digest = digest.into();
        let sha256 = digest_sha256(&digest)?;
        let mut descriptor = Self {
            media_type: VFX_EFFECT_MEDIA_TYPE.to_string(),
            effect_id: effect_id.into(),
            schema_version,
            digest,
            size,
            store_relative_path: effect_store_relative_path(&sha256)?,
        };
        let verified = validate_effect_descriptor(&descriptor)?;
        descriptor.store_relative_path = effect_store_relative_path(&verified)?;
        Ok(descriptor)
    }

    pub fn from_effect_bytes(
        effect: &VfxEffectAsset,
        sha256: &str,
        size: u64,
    ) -> Result<Self, EngineError> {
        Self::new(
            effect.effect_id.clone(),
            effect.schema_version,
            format!("{VFX_EFFECT_DIGEST_ALGORITHM}:{sha256}"),
            size,
        )
    }

    pub fn sha256(&self) -> Result<String, EngineError> {
        validate_effect_descriptor(self)
    }
}

pub fn write_effect_to_content_store(
    root: impl AsRef<Path>,
    effect: &VfxEffectAsset,
) -> Result<VfxEffectStoreWriteback, EngineError> {
    let root = root.as_ref();
    validate_store_root(root)?;
    let bytes = effect_to_json_bytes(effect)?;
    let sha256 = sha256_hex(&bytes);
    let descriptor = VfxEffectDescriptor::from_effect_bytes(effect, &sha256, bytes.len() as u64)?;
    let object_path = effect_object_path(root, &sha256)?;

    let deduplicated = if has_existing_object(root, &sha256)? {
        verify_existing_object(root, &bytes, &sha256)?;
        true
    } else {
        write_new_object(root, &bytes, &sha256)?;
        false
    };

    let readback = read_effect_from_content_store(root, &descriptor)?;
    Ok(VfxEffectStoreWriteback {
        descriptor,
        object_path,
        deduplicated,
        readback,
    })
}

pub fn read_effect_from_content_store(
    root: impl AsRef<Path>,
    descriptor: &VfxEffectDescriptor,
) -> Result<VfxEffectStoreReadback, EngineError> {
    let root = root.as_ref();
    validate_store_root(root)?;
    let expected_sha256 = validate_effect_descriptor(descriptor)?;
    let object_path = effect_object_path(root, &expected_sha256)?;
    let row = effect_object_row(&expected_sha256)?;
    let readback = read_calyx_json_bytes(&effect_object_spec(root, &row))
        .map_err(|error| invalid_vfx_error("content_store.read", error))?
        .ok_or_else(|| {
            invalid_vfx_error(
                "content_store.read",
                format!(
                    "missing Calyx Aster VFX content row {} at {}",
                    String::from_utf8_lossy(&row),
                    root.display()
                ),
            )
        })?;
    let bytes = readback.bytes;
    let actual_size = bytes.len() as u64;
    if actual_size != descriptor.size {
        return invalid_vfx(
            "content_store.size",
            format!(
                "{} descriptor_size={} actual_size={actual_size}",
                object_path.display(),
                descriptor.size
            ),
        );
    }
    let actual_sha256 = sha256_hex(&bytes);
    if actual_sha256 != expected_sha256 {
        return invalid_vfx(
            "content_store.sha256",
            format!(
                "{} descriptor_digest={} actual_sha256={actual_sha256}",
                object_path.display(),
                descriptor.digest
            ),
        );
    }
    let effect: VfxEffectAsset =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "vfx-effect-json",
            reason: format!(
                "parse content store object {}: {error}",
                object_path.display()
            ),
        })?;
    if effect.effect_id != descriptor.effect_id {
        return invalid_vfx(
            "content_store.effect_id",
            format!(
                "{} descriptor_effect_id={} object_effect_id={}",
                object_path.display(),
                descriptor.effect_id,
                effect.effect_id
            ),
        );
    }
    if effect.schema_version != descriptor.schema_version {
        return invalid_vfx(
            "content_store.schema_version",
            format!(
                "{} descriptor_schema_version={} object_schema_version={}",
                object_path.display(),
                descriptor.schema_version,
                effect.schema_version
            ),
        );
    }
    let validation = validate_effect(&effect)?;
    Ok(VfxEffectStoreReadback {
        descriptor: descriptor.clone(),
        object_path,
        bytes: actual_size,
        sha256: actual_sha256,
        effect,
        validation,
    })
}

pub fn count_effects_in_content_store(
    root: impl AsRef<Path>,
) -> Result<VfxEffectStoreCount, EngineError> {
    let root = root.as_ref();
    validate_store_root(root)?;
    if !root.exists() {
        return Ok(VfxEffectStoreCount {
            vault_dir: root.to_path_buf(),
            seq: 0,
            graph_rows: 0,
            effect_count: 0,
            row_keys: Vec::new(),
        });
    }
    let vault = open_content_vault(root)?;
    let seq = vault.latest_seq();
    let column = PlainColumn::new(&vault, VFX_CONTENT_COLLECTION)
        .map_err(|error| invalid_vfx_error("content_store.count.column", error.to_string()))?;
    let rows = column
        .scan_column(seq, VFX_CONTENT_COLUMN, VFX_CONTENT_COUNT_LIMIT)
        .map_err(|error| invalid_vfx_error("content_store.count.column_scan", error.to_string()))?;
    let graph_rows = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| invalid_vfx_error("content_store.count.graph_scan", error.to_string()))?
        .len();
    Ok(VfxEffectStoreCount {
        vault_dir: root.to_path_buf(),
        seq,
        graph_rows,
        effect_count: rows.len(),
        row_keys: rows
            .into_iter()
            .map(|cell| String::from_utf8_lossy(&cell.row).to_string())
            .collect(),
    })
}

pub fn validate_effect_descriptor(descriptor: &VfxEffectDescriptor) -> Result<String, EngineError> {
    if descriptor.media_type != VFX_EFFECT_MEDIA_TYPE {
        return invalid_vfx(
            "content_store.media_type",
            format!(
                "expected {VFX_EFFECT_MEDIA_TYPE}, got {}",
                descriptor.media_type
            ),
        );
    }
    if descriptor.effect_id.trim().is_empty() {
        return invalid_vfx("content_store.effect_id", "effect_id must be non-empty");
    }
    if descriptor.schema_version != VFX_EFFECT_SCHEMA_VERSION {
        return invalid_vfx(
            "content_store.schema_version",
            format!(
                "expected {}, got {}",
                VFX_EFFECT_SCHEMA_VERSION, descriptor.schema_version
            ),
        );
    }
    if descriptor.size == 0 {
        return invalid_vfx("content_store.size", "descriptor size must be non-zero");
    }
    let sha256 = digest_sha256(&descriptor.digest)?;
    let expected_path = effect_store_relative_path(&sha256)?;
    if descriptor.store_relative_path != expected_path {
        return invalid_vfx(
            "content_store.relative_path",
            format!(
                "descriptor path {} must equal digest path {expected_path}",
                descriptor.store_relative_path
            ),
        );
    }
    Ok(sha256)
}

pub fn effect_store_relative_path(sha256: &str) -> Result<String, EngineError> {
    validate_sha256_hex(sha256)?;
    Ok(format!(
        "{VFX_EFFECT_STORE_PREFIX}/{VFX_EFFECT_DIGEST_ALGORITHM}/{}/{}",
        &sha256[..2],
        sha256
    ))
}

fn effect_object_path(root: &Path, sha256: &str) -> Result<PathBuf, EngineError> {
    validate_sha256_hex(sha256)?;
    Ok(root
        .join(VFX_EFFECT_STORE_PREFIX)
        .join(VFX_EFFECT_DIGEST_ALGORITHM)
        .join(&sha256[..2])
        .join(sha256))
}

fn write_new_object(root: &Path, bytes: &[u8], sha256: &str) -> Result<(), EngineError> {
    let row = effect_object_row(sha256)?;
    let stored = write_calyx_json_bytes(&effect_object_spec(root, &row), bytes)
        .map_err(|error| invalid_vfx_error("content_store.write", error))?;
    if stored.bytes != bytes {
        return invalid_vfx(
            "content_store.readback",
            format!(
                "Aster VFX content row changed expected_bytes={} actual_bytes={}",
                bytes.len(),
                stored.bytes.len()
            ),
        );
    }
    Ok(())
}

fn verify_existing_object(root: &Path, expected: &[u8], sha256: &str) -> Result<(), EngineError> {
    let row = effect_object_row(sha256)?;
    let bytes = read_calyx_json_bytes(&effect_object_spec(root, &row))
        .map_err(|error| invalid_vfx_error("content_store.existing.read", error))?
        .ok_or_else(|| {
            invalid_vfx_error(
                "content_store.existing.read",
                format!(
                    "missing existing Calyx Aster VFX content row {}",
                    String::from_utf8_lossy(&row)
                ),
            )
        })?
        .bytes;
    if bytes != expected {
        return invalid_vfx(
            "content_store.existing.bytes",
            format!(
                "Calyx Aster VFX content row exists for sha256={sha256} but bytes do not match canonical effect JSON"
            ),
        );
    }
    Ok(())
}

fn validate_store_root(root: &Path) -> Result<(), EngineError> {
    if root.as_os_str().is_empty() {
        return invalid_vfx("content_store.root", "store root must be non-empty");
    }
    if root.is_file() {
        return invalid_vfx(
            "content_store.root",
            format!(
                "{} is a file; expected a Calyx Aster content vault directory",
                root.display()
            ),
        );
    }
    Ok(())
}

fn has_existing_object(root: &Path, sha256: &str) -> Result<bool, EngineError> {
    let row = effect_object_row(sha256)?;
    read_calyx_json_bytes(&effect_object_spec(root, &row))
        .map(|readback| readback.is_some())
        .map_err(|error| invalid_vfx_error("content_store.exists", error))
}

fn open_content_vault(root: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = VFX_CONTENT_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| invalid_vfx_error("content_store.vault_id", error.to_string()))?;
    AsterVault::open(
        root,
        vault_id,
        VFX_CONTENT_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| invalid_vfx_error("content_store.open", error.to_string()))
}

fn digest_sha256(digest: &str) -> Result<String, EngineError> {
    let Some(encoded) = digest.strip_prefix("sha256:") else {
        return invalid_vfx(
            "content_store.digest",
            format!("digest must use sha256:<64 lowercase hex>, got {digest}"),
        );
    };
    validate_sha256_hex(encoded)?;
    Ok(encoded.to_string())
}

fn validate_sha256_hex(value: &str) -> Result<(), EngineError> {
    if !litd_hash::is_sha256_hex(value) {
        return invalid_vfx("content_store.sha256", "SHA-256 must be 64 lowercase hex");
    }
    Ok(())
}

fn effect_object_row(sha256: &str) -> Result<Vec<u8>, EngineError> {
    validate_sha256_hex(sha256)?;
    Ok(format!("{VFX_EFFECT_STORE_PREFIX}/{VFX_EFFECT_DIGEST_ALGORITHM}/{sha256}").into_bytes())
}

fn effect_object_spec<'a>(root: &'a Path, row: &'a [u8]) -> CalyxJsonStoreSpec<'a> {
    CalyxJsonStoreSpec {
        vault_dir: root,
        vault_id: VFX_CONTENT_VAULT_ID,
        purpose: VFX_CONTENT_VAULT_PURPOSE,
        collection: VFX_CONTENT_COLLECTION,
        row_key: row,
        column: VFX_CONTENT_COLUMN,
    }
}
