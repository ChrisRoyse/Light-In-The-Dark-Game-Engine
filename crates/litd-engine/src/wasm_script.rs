//! Deterministic, fuel-metered WebAssembly escape hatch for trigger scripts.

use std::borrow::Cow;

use litd_hash::{Fnv1a64, is_sha256_hex, sha256_hex};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wasm_encoder::{CustomSection, Section};

use crate::math::Fixed;

pub use crate::wasm_script_runtime::WasmScriptRuntime;
pub use crate::wasm_script_validation::WasmValidationReport;

pub const WASM_SCRIPT_ABI_VERSION: u32 = 1;
pub const WASMTIME_PINNED_VERSION: &str = "46.0.1";
pub const WASM_FIXED_SLOT_COUNT: usize = 64;
pub const WASM_MAX_MEMORY_BYTES: usize = 64 * 1024;
pub const WASM_MAX_MODULE_BYTES: usize = 256 * 1024;
pub const WASM_MIN_FUEL: u64 = 1;
pub const WASM_MAX_FUEL: u64 = 10_000_000;

#[derive(Debug, Error)]
pub enum WasmScriptError {
    #[error("WASM script config field {field} is invalid: {reason}")]
    InvalidConfig { field: &'static str, reason: String },
    #[error("GATE_FAILED: step=script_authoring module={module_id} detail={reason}")]
    AuthoringFailed { module_id: String, reason: String },
    #[error("GATE_FAILED: step=script_validation module={module_id} detail={reason}")]
    ValidationFailed { module_id: String, reason: String },
    #[error(
        "GATE_FAILED: step=script_import module={module_id} denied={namespace}.{name} detail=only litd fixed-point imports are allowed"
    )]
    DeniedImport {
        module_id: String,
        namespace: String,
        name: String,
    },
    #[error("WASM artifact for module {module_id} failed integrity: {reason}")]
    ArtifactMismatch { module_id: String, reason: String },
    #[error("WASM module {module_id} could not be prepared: {reason}")]
    PreparationFailed { module_id: String, reason: String },
    #[error(
        "GATE_FAILED: step=script_fuel module={module_id} entry={entry_point} budget={budget} detail=fuel exhausted"
    )]
    FuelExhausted {
        module_id: String,
        entry_point: String,
        budget: u64,
    },
    #[error("WASM module {module_id} entry {entry_point} trapped: {reason}")]
    ExecutionTrapped {
        module_id: String,
        entry_point: String,
        reason: String,
    },
    #[error("WASM fixed-point state slot {slot} is out of range 0..{slot_count}")]
    StateSlotOutOfRange { slot: i32, slot_count: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmScriptArtifact {
    pub module_id: String,
    pub abi_version: u32,
    pub wasmtime_version: String,
    pub wasm_sha256: String,
    pub wasm_bytes: Vec<u8>,
}

impl WasmScriptArtifact {
    pub fn from_wat(
        module_id: impl Into<String>,
        wat_source: &str,
    ) -> Result<Self, WasmScriptError> {
        let module_id = module_id.into();
        let bytes =
            wat::parse_str(wat_source).map_err(|error| WasmScriptError::AuthoringFailed {
                module_id: module_id.clone(),
                reason: format!("parse WAT: {error}"),
            })?;
        Self::from_wasm_bytes(module_id, bytes)
    }

    pub fn from_wat_with_custom_section(
        module_id: impl Into<String>,
        wat_source: &str,
        section_name: &str,
        section_data: &[u8],
    ) -> Result<Self, WasmScriptError> {
        let module_id = module_id.into();
        validate_custom_section(section_name, section_data)?;
        let mut bytes =
            wat::parse_str(wat_source).map_err(|error| WasmScriptError::AuthoringFailed {
                module_id: module_id.clone(),
                reason: format!("parse WAT: {error}"),
            })?;
        if crate::wasm_script_validation::custom_section(&bytes, section_name)?.is_some() {
            return Err(WasmScriptError::ArtifactMismatch {
                module_id,
                reason: format!("custom section {section_name} already exists"),
            });
        }
        CustomSection {
            name: Cow::Borrowed(section_name),
            data: Cow::Borrowed(section_data),
        }
        .append_to(&mut bytes);
        Self::from_wasm_bytes(module_id, bytes)
    }

    pub fn from_wasm_bytes(
        module_id: impl Into<String>,
        wasm_bytes: Vec<u8>,
    ) -> Result<Self, WasmScriptError> {
        let module_id = module_id.into();
        validate_identifier("module_id", &module_id)?;
        crate::wasm_script_validation::validate_module(&module_id, &wasm_bytes)?;
        Ok(Self {
            module_id,
            abi_version: WASM_SCRIPT_ABI_VERSION,
            wasmtime_version: WASMTIME_PINNED_VERSION.to_string(),
            wasm_sha256: sha256_hex(&wasm_bytes),
            wasm_bytes,
        })
    }

    pub fn validate(&self) -> Result<WasmValidationReport, WasmScriptError> {
        validate_identifier("artifact.module_id", &self.module_id)?;
        if self.abi_version != WASM_SCRIPT_ABI_VERSION {
            return Err(WasmScriptError::ArtifactMismatch {
                module_id: self.module_id.clone(),
                reason: format!(
                    "ABI version {} does not match required {}",
                    self.abi_version, WASM_SCRIPT_ABI_VERSION
                ),
            });
        }
        if self.wasmtime_version != WASMTIME_PINNED_VERSION {
            return Err(WasmScriptError::ArtifactMismatch {
                module_id: self.module_id.clone(),
                reason: format!(
                    "Wasmtime version {} does not match replay pin {}",
                    self.wasmtime_version, WASMTIME_PINNED_VERSION
                ),
            });
        }
        if !is_sha256_hex(&self.wasm_sha256) || sha256_hex(&self.wasm_bytes) != self.wasm_sha256 {
            return Err(WasmScriptError::ArtifactMismatch {
                module_id: self.module_id.clone(),
                reason: "module SHA-256 does not match physical wasm bytes".to_string(),
            });
        }
        crate::wasm_script_validation::validate_module(&self.module_id, &self.wasm_bytes)
    }

    pub fn custom_section(&self, name: &str) -> Result<Option<&[u8]>, WasmScriptError> {
        crate::wasm_script_validation::custom_section(&self.wasm_bytes, name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmScriptBinding {
    pub artifact: WasmScriptArtifact,
    pub entry_point: String,
    pub fuel_budget: u64,
}

impl WasmScriptBinding {
    pub fn validate(&self) -> Result<WasmValidationReport, WasmScriptError> {
        validate_identifier("binding.entry_point", &self.entry_point)?;
        if !(WASM_MIN_FUEL..=WASM_MAX_FUEL).contains(&self.fuel_budget) {
            return Err(WasmScriptError::InvalidConfig {
                field: "binding.fuel_budget",
                reason: format!("must be in {WASM_MIN_FUEL}..={WASM_MAX_FUEL}"),
            });
        }
        self.artifact.validate()
    }

    pub fn replay_identity(&self) -> WasmReplayIdentity {
        WasmReplayIdentity {
            module_id: self.artifact.module_id.clone(),
            module_sha256: self.artifact.wasm_sha256.clone(),
            abi_version: self.artifact.abi_version,
            wasmtime_version: self.artifact.wasmtime_version.clone(),
            entry_point: self.entry_point.clone(),
            fuel_budget: self.fuel_budget,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmReplayIdentity {
    pub module_id: String,
    pub module_sha256: String,
    pub abi_version: u32,
    pub wasmtime_version: String,
    pub entry_point: String,
    pub fuel_budget: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasmFixedState {
    slots: [Fixed; WASM_FIXED_SLOT_COUNT],
}

impl Default for WasmFixedState {
    fn default() -> Self {
        Self {
            slots: [Fixed::ZERO; WASM_FIXED_SLOT_COUNT],
        }
    }
}

impl WasmFixedState {
    pub fn get(&self, slot: usize) -> Result<Fixed, WasmScriptError> {
        self.slots
            .get(slot)
            .copied()
            .ok_or(WasmScriptError::StateSlotOutOfRange {
                slot: i32::try_from(slot).unwrap_or(i32::MAX),
                slot_count: WASM_FIXED_SLOT_COUNT,
            })
    }

    pub fn set(&mut self, slot: usize, value: Fixed) -> Result<(), WasmScriptError> {
        let target = self
            .slots
            .get_mut(slot)
            .ok_or(WasmScriptError::StateSlotOutOfRange {
                slot: i32::try_from(slot).unwrap_or(i32::MAX),
                slot_count: WASM_FIXED_SLOT_COUNT,
            })?;
        *target = value;
        Ok(())
    }

    pub fn state_hash(&self) -> u64 {
        let mut hash = Fnv1a64::new();
        hash.update(b"litd.wasm.fixed-state.v1");
        for value in self.slots {
            hash.update_i64_le(value.raw());
        }
        hash.finish()
    }

    pub(crate) fn get_raw(&self, slot: i32) -> Result<i64, WasmScriptError> {
        slot_index(slot).map(|index| self.slots[index].raw())
    }

    pub(crate) fn set_raw(&mut self, slot: i32, raw: i64) -> Result<(), WasmScriptError> {
        let index = slot_index(slot)?;
        self.slots[index] = Fixed::from_raw(raw);
        Ok(())
    }

    pub(crate) fn add_raw(&mut self, slot: i32, raw: i64) -> Result<i64, WasmScriptError> {
        let index = slot_index(slot)?;
        let updated = self.slots[index].raw().checked_add(raw).ok_or_else(|| {
            WasmScriptError::ExecutionTrapped {
                module_id: "host".to_string(),
                entry_point: "litd.add_fixed".to_string(),
                reason: format!("fixed-point overflow in slot {slot}"),
            }
        })?;
        self.slots[index] = Fixed::from_raw(updated);
        Ok(updated)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmInvocationStats {
    pub fuel_budget: u64,
    pub fuel_consumed: u64,
    pub fixed_state_hash: u64,
    pub replay_state_hash: u64,
}

pub(crate) fn validate_identifier(field: &'static str, value: &str) -> Result<(), WasmScriptError> {
    let mut chars = value.chars();
    let valid = value.len() <= 96
        && chars
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if valid {
        Ok(())
    } else {
        Err(WasmScriptError::InvalidConfig {
            field,
            reason: "must be a non-empty ASCII identifier of at most 96 bytes".to_string(),
        })
    }
}

fn validate_custom_section(name: &str, data: &[u8]) -> Result<(), WasmScriptError> {
    if !name.starts_with("litd.") || name.len() > 96 || !name.is_ascii() {
        return Err(WasmScriptError::InvalidConfig {
            field: "custom_section.name",
            reason: "must be a bounded ASCII litd.* name".to_string(),
        });
    }
    if data.is_empty() || data.len() > 64 * 1024 {
        return Err(WasmScriptError::InvalidConfig {
            field: "custom_section.data",
            reason: "must contain 1..=65536 bytes".to_string(),
        });
    }
    Ok(())
}

fn slot_index(slot: i32) -> Result<usize, WasmScriptError> {
    usize::try_from(slot)
        .ok()
        .filter(|index| *index < WASM_FIXED_SLOT_COUNT)
        .ok_or(WasmScriptError::StateSlotOutOfRange {
            slot,
            slot_count: WASM_FIXED_SLOT_COUNT,
        })
}
