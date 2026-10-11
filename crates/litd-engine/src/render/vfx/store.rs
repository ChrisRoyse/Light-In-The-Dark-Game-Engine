use litd_hash::sha256_hex;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
    render::vfx::{VfxEffectAsset, VfxValidationReport, invalid_vfx_error, validate_effect},
};

const VFX_EFFECT_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC4";
const VFX_EFFECT_VAULT_PURPOSE: &[u8] = b"litd-engine-vfx-effect-issue1360";
const VFX_EFFECT_COLLECTION: &str = "litd_engine_vfx_effect";
const VFX_EFFECT_ROW: &[u8] = b"vfx/effect/latest";
const VFX_EFFECT_COLUMN: &[u8] = b"effect_json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfxEffectFileReadback {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub validation: VfxValidationReport,
}

pub fn effect_to_json_bytes(effect: &VfxEffectAsset) -> Result<Vec<u8>, EngineError> {
    validate_effect(effect)?;
    serde_json::to_vec_pretty(effect).map_err(|error| EngineError::SerializationFailed {
        format: "vfx-effect-json",
        reason: error.to_string(),
    })
}

pub fn write_effect_json(
    path: impl AsRef<Path>,
    effect: &VfxEffectAsset,
) -> Result<VfxEffectFileReadback, EngineError> {
    let path = path.as_ref();
    let bytes = effect_to_json_bytes(effect)?;
    let stored = write_calyx_json_bytes(&effect_spec(path), &bytes)
        .map_err(|error| invalid_vfx_error("aster.write", error))?;
    if stored.bytes != bytes {
        return Err(invalid_vfx_error(
            "aster.readback",
            "Aster VFX effect bytes did not match serialized bytes",
        ));
    }
    let readback = read_effect_json(path)?;
    let expected_sha = sha256_hex(&bytes);
    if readback.sha256 != expected_sha {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_sha,
            actual: readback.sha256,
        });
    }
    Ok(readback)
}

pub fn read_effect_json(path: impl AsRef<Path>) -> Result<VfxEffectFileReadback, EngineError> {
    let path = path.as_ref();
    let readback = read_calyx_json_bytes(&effect_spec(path))
        .map_err(|error| invalid_vfx_error("aster.read", error))?
        .ok_or_else(|| {
            invalid_vfx_error(
                "aster.read",
                format!("missing Calyx Aster VFX effect row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let effect: VfxEffectAsset =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "vfx-effect-json",
            reason: format!("parse {}: {error}", path.display()),
        })?;
    let validation = validate_effect(&effect)?;
    Ok(VfxEffectFileReadback {
        path: path.to_path_buf(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        validation,
    })
}

fn effect_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: VFX_EFFECT_VAULT_ID,
        purpose: VFX_EFFECT_VAULT_PURPOSE,
        collection: VFX_EFFECT_COLLECTION,
        row_key: VFX_EFFECT_ROW,
        column: VFX_EFFECT_COLUMN,
    }
}
