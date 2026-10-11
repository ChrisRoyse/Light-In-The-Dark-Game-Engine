//! One-way Calyx lowering valve for deterministic sim data.
//!
//! The valve consumes verified Calyx content readbacks outside hot paths and
//! emits frozen, fingerprinted data that sim systems can use without querying
//! Calyx live.

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    calyx_content::{
        ENGINE_CALYX_CONTENT_READBACK_SCHEMA, ENGINE_LOWERED_CONTENT_SCHEMA,
        EngineContentConstellationReadback, EngineContentKind,
    },
    determinism_boundary::{HotPathPhase, current_hot_path_phase},
    math::Fixed,
    sim_order::SimOrderSource,
};

pub const ENGINE_CALYX_LOWERING_VALVE_SCHEMA: &str = "litd.engine.calyx_lowering_valve.v1";
pub const ENGINE_LOWERED_SIM_PACKAGE_SCHEMA: &str = "litd.engine.lowered_sim_package.v1";
pub const ENGINE_LOWERING_HOT_PATH_POLICY: &str = "frozen_fingerprinted_data_only";
pub const ENGINE_LOWERING_SIM_ACCESS_MODE: &str = "frozen_lowered_data_only";
pub const ENGINE_LOWERING_SOURCE: &str = "calyx_content_constellation_readback";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineLoweredSimFieldValue {
    Bool(bool),
    I64(i64),
    U64(u64),
    String(String),
    EmptyArray,
    EmptyObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineLoweredSimField {
    pub path: String,
    pub value: EngineLoweredSimFieldValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineLoweredSimPackage {
    pub schema: String,
    pub source: String,
    pub source_cx_id: String,
    pub source_kind: EngineContentKind,
    pub source_id: String,
    pub display_name: String,
    pub source_fingerprint: String,
    pub package_fingerprint: String,
    pub hot_path_policy: String,
    pub sim_access_mode: String,
    pub fixed_point_fractional_bits: u32,
    pub lowered_agent_id: u32,
    pub calyx_live_query_allowed_in_hot_path: bool,
    pub fields: Vec<EngineLoweredSimField>,
}

impl EngineLoweredSimPackage {
    pub fn sim_order_source(&self) -> SimOrderSource {
        SimOrderSource::lowered_agent(self.lowered_agent_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineLoweredSimReadback {
    pub schema: String,
    pub source_cx_id: String,
    pub source_kind: EngineContentKind,
    pub source_id: String,
    pub source_fingerprint: String,
    pub package_fingerprint: String,
    pub field_count: usize,
    pub hot_path_policy: String,
    pub sim_access_mode: String,
    pub calyx_live_query_allowed_in_hot_path: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineLoweringError {
    #[error("engine Calyx lowering field {field} is invalid: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("engine Calyx lowering cannot run inside hot path {phase:?}")]
    HotPath { phase: HotPathPhase },
    #[error("engine Calyx lowering fingerprint mismatch: expected {expected}, actual {actual}")]
    FingerprintMismatch { expected: String, actual: String },
    #[error("engine Calyx lowering codec failed: {0}")]
    Codec(String),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct EngineLoweringValve;

impl EngineLoweringValve {
    pub fn lower_content_for_sim(
        readback: &EngineContentConstellationReadback,
    ) -> Result<EngineLoweredSimPackage, EngineLoweringError> {
        if let Some(phase) = current_hot_path_phase() {
            return Err(EngineLoweringError::HotPath { phase });
        }
        validate_content_readback(readback)?;

        let mut fields = Vec::new();
        flatten_payload(&readback.payload, "", &mut fields)?;
        // Canonicalize to the lexical order that validate_package requires. Sorted-key object
        // traversal is already lexical, but numeric array indices are emitted in insertion order
        // (".../9" before ".../10"), which is NOT lexical once an array has 10+ elements. Sorting
        // here is a no-op for every already-lexical payload and fixes 10+ element arrays (e.g. a
        // race class allowlist of 11+ classes) that would otherwise fail the sorted-unique check.
        fields.sort_by(|a, b| a.path.cmp(&b.path));
        if fields.is_empty() {
            return invalid("payload", "payload must lower into at least one sim field");
        }

        let lowered_agent_id = lowered_agent_id(&readback.input_sha256)?;
        let mut package = EngineLoweredSimPackage {
            schema: ENGINE_LOWERED_SIM_PACKAGE_SCHEMA.to_string(),
            source: ENGINE_LOWERING_SOURCE.to_string(),
            source_cx_id: readback.cx_id.clone(),
            source_kind: readback.kind,
            source_id: readback.id.clone(),
            display_name: readback.display_name.clone(),
            source_fingerprint: readback.input_sha256.clone(),
            package_fingerprint: String::new(),
            hot_path_policy: ENGINE_LOWERING_HOT_PATH_POLICY.to_string(),
            sim_access_mode: ENGINE_LOWERING_SIM_ACCESS_MODE.to_string(),
            fixed_point_fractional_bits: Fixed::FRACTIONAL_BITS,
            lowered_agent_id,
            calyx_live_query_allowed_in_hot_path: false,
            fields,
        };
        package.package_fingerprint = package_fingerprint(&package)?;
        Ok(package)
    }

    pub fn verify_lowered_package(
        package: &EngineLoweredSimPackage,
    ) -> Result<EngineLoweredSimReadback, EngineLoweringError> {
        validate_package(package)?;
        let expected = package_fingerprint(package)?;
        if expected != package.package_fingerprint {
            return Err(EngineLoweringError::FingerprintMismatch {
                expected,
                actual: package.package_fingerprint.clone(),
            });
        }
        Ok(EngineLoweredSimReadback {
            schema: ENGINE_CALYX_LOWERING_VALVE_SCHEMA.to_string(),
            source_cx_id: package.source_cx_id.clone(),
            source_kind: package.source_kind,
            source_id: package.source_id.clone(),
            source_fingerprint: package.source_fingerprint.clone(),
            package_fingerprint: package.package_fingerprint.clone(),
            field_count: package.fields.len(),
            hot_path_policy: package.hot_path_policy.clone(),
            sim_access_mode: package.sim_access_mode.clone(),
            calyx_live_query_allowed_in_hot_path: package.calyx_live_query_allowed_in_hot_path,
        })
    }
}

fn validate_content_readback(
    readback: &EngineContentConstellationReadback,
) -> Result<(), EngineLoweringError> {
    if readback.schema != ENGINE_CALYX_CONTENT_READBACK_SCHEMA {
        return invalid(
            "schema",
            format!("expected {ENGINE_CALYX_CONTENT_READBACK_SCHEMA}"),
        );
    }
    if readback.cx_id.trim().is_empty() {
        return invalid("cx_id", "source constellation id must be non-empty");
    }
    if readback.input_sha256.trim().is_empty() {
        return invalid("input_sha256", "source fingerprint must be non-empty");
    }
    if readback.lowering.schema != ENGINE_LOWERED_CONTENT_SCHEMA {
        return invalid(
            "lowering.schema",
            format!("expected {ENGINE_LOWERED_CONTENT_SCHEMA}"),
        );
    }
    if readback.lowering.hot_path_policy != ENGINE_LOWERING_HOT_PATH_POLICY {
        return invalid(
            "lowering.hot_path_policy",
            format!("expected {ENGINE_LOWERING_HOT_PATH_POLICY}"),
        );
    }
    if readback.lowering.source_cx_id != readback.cx_id {
        return invalid("lowering.source_cx_id", "must match readback cx_id");
    }
    if readback.lowering.source_kind != readback.kind {
        return invalid("lowering.source_kind", "must match readback kind");
    }
    if readback.lowering.source_id != readback.id {
        return invalid("lowering.source_id", "must match readback id");
    }
    if readback.lowering.fingerprint != readback.input_sha256 {
        return invalid("lowering.fingerprint", "must match readback input_sha256");
    }
    if readback.payload.is_null() {
        return invalid("payload", "payload must be non-null JSON");
    }
    Ok(())
}

fn validate_package(package: &EngineLoweredSimPackage) -> Result<(), EngineLoweringError> {
    if package.schema != ENGINE_LOWERED_SIM_PACKAGE_SCHEMA {
        return invalid(
            "schema",
            format!("expected {ENGINE_LOWERED_SIM_PACKAGE_SCHEMA}"),
        );
    }
    if package.source != ENGINE_LOWERING_SOURCE {
        return invalid("source", format!("expected {ENGINE_LOWERING_SOURCE}"));
    }
    if package.hot_path_policy != ENGINE_LOWERING_HOT_PATH_POLICY {
        return invalid(
            "hot_path_policy",
            format!("expected {ENGINE_LOWERING_HOT_PATH_POLICY}"),
        );
    }
    if package.sim_access_mode != ENGINE_LOWERING_SIM_ACCESS_MODE {
        return invalid(
            "sim_access_mode",
            format!("expected {ENGINE_LOWERING_SIM_ACCESS_MODE}"),
        );
    }
    if package.fixed_point_fractional_bits != Fixed::FRACTIONAL_BITS {
        return invalid(
            "fixed_point_fractional_bits",
            format!("expected {}", Fixed::FRACTIONAL_BITS),
        );
    }
    if package.calyx_live_query_allowed_in_hot_path {
        return invalid(
            "calyx_live_query_allowed_in_hot_path",
            "live Calyx queries are forbidden in the deterministic hot path",
        );
    }
    if package.fields.is_empty() {
        return invalid("fields", "lowered sim package must contain fields");
    }
    let mut previous = "";
    for field in &package.fields {
        if field.path.trim().is_empty() {
            return invalid("fields.path", "field path must be non-empty");
        }
        if field.path.as_str() <= previous {
            return invalid("fields", "field paths must be strictly sorted and unique");
        }
        previous = field.path.as_str();
    }
    Ok(())
}

fn flatten_payload(
    value: &Value,
    path: &str,
    fields: &mut Vec<EngineLoweredSimField>,
) -> Result<(), EngineLoweringError> {
    match value {
        Value::Null => invalid_field(path, "null cannot be lowered into deterministic sim data"),
        Value::Bool(value) => push_field(fields, path, EngineLoweredSimFieldValue::Bool(*value)),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                push_field(fields, path, EngineLoweredSimFieldValue::I64(value))
            } else if let Some(value) = value.as_u64() {
                push_field(fields, path, EngineLoweredSimFieldValue::U64(value))
            } else {
                invalid_field(
                    path,
                    "numbers must be integers; floating point values are forbidden",
                )
            }
        }
        Value::String(value) => push_field(
            fields,
            path,
            EngineLoweredSimFieldValue::String(value.clone()),
        ),
        Value::Array(values) => {
            if values.is_empty() {
                return push_field(fields, path, EngineLoweredSimFieldValue::EmptyArray);
            }
            for (index, value) in values.iter().enumerate() {
                flatten_payload(value, &format!("{path}/{index}"), fields)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.is_empty() {
                return push_field(fields, path, EngineLoweredSimFieldValue::EmptyObject);
            }
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                if key.trim().is_empty() {
                    return invalid_field(path, "object keys must be non-empty");
                }
                let child_path = format!("{path}/{}", escape_json_pointer(key));
                flatten_payload(&values[key], &child_path, fields)?;
            }
            Ok(())
        }
    }
}

fn push_field(
    fields: &mut Vec<EngineLoweredSimField>,
    path: &str,
    value: EngineLoweredSimFieldValue,
) -> Result<(), EngineLoweringError> {
    fields.push(EngineLoweredSimField {
        path: display_path(path),
        value,
    });
    Ok(())
}

fn display_path(path: &str) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

fn escape_json_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn package_fingerprint(package: &EngineLoweredSimPackage) -> Result<String, EngineLoweringError> {
    let mut input = String::new();
    input.push_str(ENGINE_LOWERED_SIM_PACKAGE_SCHEMA);
    input.push('\n');
    input.push_str(&package.source);
    input.push('\n');
    input.push_str(&package.source_cx_id);
    input.push('\n');
    input.push_str(package.source_kind.as_str());
    input.push('\n');
    input.push_str(&package.source_id);
    input.push('\n');
    input.push_str(&package.display_name);
    input.push('\n');
    input.push_str(&package.source_fingerprint);
    input.push('\n');
    input.push_str(&package.hot_path_policy);
    input.push('\n');
    input.push_str(&package.sim_access_mode);
    input.push('\n');
    input.push_str(&package.fixed_point_fractional_bits.to_string());
    input.push('\n');
    input.push_str(&package.lowered_agent_id.to_string());
    input.push('\n');
    input.push_str(if package.calyx_live_query_allowed_in_hot_path {
        "live_calyx_allowed"
    } else {
        "live_calyx_forbidden"
    });
    input.push('\n');
    for field in &package.fields {
        input.push_str(&field.path);
        input.push('=');
        input.push_str(&serde_json::to_string(&field.value).map_err(codec_failed)?);
        input.push('\n');
    }
    Ok(sha256_hex(input.as_bytes()))
}

fn lowered_agent_id(fingerprint: &str) -> Result<u32, EngineLoweringError> {
    let hex = fingerprint
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(8)
        .collect::<String>();
    if hex.len() != 8 {
        return invalid(
            "input_sha256",
            "source fingerprint must contain at least eight hex digits",
        );
    }
    let parsed =
        u32::from_str_radix(&hex, 16).map_err(|error| EngineLoweringError::InvalidInput {
            field: "input_sha256".to_string(),
            reason: error.to_string(),
        })?;
    Ok(parsed.max(1))
}

fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineLoweringError> {
    Err(EngineLoweringError::InvalidInput {
        field: field.into(),
        reason: reason.into(),
    })
}

fn invalid_field<T>(path: &str, reason: impl Into<String>) -> Result<T, EngineLoweringError> {
    invalid(display_path(path), reason)
}

fn codec_failed(error: impl std::fmt::Display) -> EngineLoweringError {
    EngineLoweringError::Codec(error.to_string())
}
