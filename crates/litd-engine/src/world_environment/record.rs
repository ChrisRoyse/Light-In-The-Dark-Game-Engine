//! Persist + lowering spine shared by every world-environment surface.
//!
//! An [`EnvironmentPayload`] is the deterministic, integer-only JSON body a
//! concern produces. [`persist_and_lower_environment`] routes it through the REAL
//! content path: `EngineCalyxContentStore::put_content` (which applies the #1791
//! `pending_measurement` semantics when no ORT/addon is present) and then the
//! `EngineLoweringValve` to a frozen fingerprinted sim package. No side stores.

use serde_json::{Value, json};

use crate::{
    calyx_content::{
        EngineCalyxContentError, EngineCalyxContentStore, EngineContentConstellationReadback,
    },
    calyx_lowering::{EngineLoweredSimPackage, EngineLoweringError, EngineLoweringValve},
};

use super::{
    WORLD_ENVIRONMENT_SCHEMA, WorldEnvironmentError, build_content_record, invalid,
    require_non_empty,
};

/// A validated, deterministic environment payload ready to persist + lower.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentPayload {
    /// Which authoring surface produced this (`surface_kind` in the JSON body).
    pub surface_kind: String,
    /// Stable authoring id (also the content-record id).
    pub id: String,
    /// The full JSON body (includes schema + surface_kind + integer fields).
    pub body: Value,
}

impl EnvironmentPayload {
    /// Build a payload, stamping the shared schema + surface kind + id and
    /// verifying every field lowers deterministically (no floats, no nulls).
    pub fn new(surface_kind: &str, id: &str, fields: Value) -> Result<Self, WorldEnvironmentError> {
        require_non_empty("payload.surface_kind", surface_kind)?;
        require_non_empty("payload.id", id)?;
        let object = fields
            .as_object()
            .ok_or_else(|| WorldEnvironmentError::Invalid {
                field: "payload.fields".to_string(),
                reason: "fields must be a JSON object".to_string(),
            })?;
        let mut body = serde_json::Map::new();
        body.insert("schema".to_string(), json!(WORLD_ENVIRONMENT_SCHEMA));
        body.insert("surface_kind".to_string(), json!(surface_kind));
        body.insert("id".to_string(), json!(id));
        for (key, value) in object {
            reject_non_lowerable(key, value)?;
            body.insert(key.clone(), value.clone());
        }
        Ok(Self {
            surface_kind: surface_kind.to_string(),
            id: id.to_string(),
            body: Value::Object(body),
        })
    }
}

/// Recursively reject any value the lowering valve cannot accept (float / null),
/// failing loud at authoring time instead of deep inside the valve.
fn reject_non_lowerable(path: &str, value: &Value) -> Result<(), WorldEnvironmentError> {
    match value {
        Value::Null => invalid(
            format!("payload.{path}"),
            "null cannot be lowered into deterministic sim data",
        ),
        Value::Number(number) => {
            if number.is_i64() || number.is_u64() {
                Ok(())
            } else {
                invalid(
                    format!("payload.{path}"),
                    "floats are forbidden; author as fixed-point milli-units",
                )
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                reject_non_lowerable(&format!("{path}/{index}"), child)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (key, child) in map {
                reject_non_lowerable(&format!("{path}/{key}"), child)?;
            }
            Ok(())
        }
        Value::Bool(_) | Value::String(_) => Ok(()),
    }
}

/// The full readback of one authored environment surface: the persisted
/// constellation row (source of truth) and the frozen lowered sim package the
/// renderer/runtime consumes.
#[derive(Debug, Clone)]
pub struct EnvironmentPersistReadback {
    pub surface_kind: String,
    pub row: EngineContentConstellationReadback,
    pub lowered: EngineLoweredSimPackage,
}

impl EnvironmentPersistReadback {
    /// A compact JSON evidence view for FSV reports.
    pub fn evidence(&self) -> Value {
        json!({
            "surface_kind": self.surface_kind,
            "content_id": self.row.id,
            "cx_id": self.row.cx_id,
            "measurement_state": self.row.measurement.measurement_state,
            "declared_learned_lenses": self.row.measurement.declared_learned_lenses,
            "measured_learned_lenses": self.row.measurement.measured_learned_lenses,
            "row_seq": self.row.database.seq,
            "row_sha256": self.row.database.sha256,
            "lowered_field_count": self.lowered.fields.len(),
            "lowered_package_fingerprint": self.lowered.package_fingerprint,
            "lowered_source_fingerprint": self.lowered.source_fingerprint,
            "calyx_live_query_allowed_in_hot_path": self.lowered.calyx_live_query_allowed_in_hot_path,
        })
    }
}

/// Error raised by the persist/lower spine.
#[derive(Debug, thiserror::Error)]
pub enum EnvironmentPersistError {
    #[error(transparent)]
    Authoring(#[from] WorldEnvironmentError),
    #[error(transparent)]
    Content(#[from] EngineCalyxContentError),
    #[error(transparent)]
    Lowering(#[from] EngineLoweringError),
}

/// Persist an authored environment payload through the real content path and
/// lower it to a frozen sim package. Works with NO ORT: the learned-lens panel
/// degrades to `pending_measurement` (#1791) while the explicit payload persists
/// and lowers unconditionally.
pub fn persist_and_lower_environment(
    store: &EngineCalyxContentStore,
    author: &str,
    editor_path: &str,
    display_name: &str,
    payload: &EnvironmentPayload,
) -> Result<EnvironmentPersistReadback, EnvironmentPersistError> {
    let record = build_content_record(
        &payload.id,
        display_name,
        author,
        editor_path,
        payload.body.clone(),
    )?;
    let row = store.put_content(&record)?;
    let package = EngineLoweringValve::lower_content_for_sim(&row)?;
    // Verify the frozen package before handing it to the renderer/runtime.
    EngineLoweringValve::verify_lowered_package(&package)?;
    Ok(EnvironmentPersistReadback {
        surface_kind: payload.surface_kind.clone(),
        row,
        lowered: package,
    })
}

/// Look up a single lowered field's value by its JSON-pointer path (e.g.
/// `"/water_deep_depth_mm"`). Fails loud if absent -- the renderer must not guess.
pub fn lowered_field<'a>(
    package: &'a EngineLoweredSimPackage,
    path: &str,
) -> Result<&'a crate::calyx_lowering::EngineLoweredSimFieldValue, WorldEnvironmentError> {
    package
        .fields
        .iter()
        .find(|field| field.path == path)
        .map(|field| &field.value)
        .ok_or_else(|| WorldEnvironmentError::Invalid {
            field: format!("lowered{path}"),
            reason: "lowered field not found in frozen sim package".to_string(),
        })
}

/// Read a lowered field as an unsigned integer, failing loud on type mismatch.
pub fn lowered_u64(
    package: &EngineLoweredSimPackage,
    path: &str,
) -> Result<u64, WorldEnvironmentError> {
    use crate::calyx_lowering::EngineLoweredSimFieldValue as V;
    match lowered_field(package, path)? {
        V::U64(value) => Ok(*value),
        V::I64(value) if *value >= 0 => Ok(*value as u64),
        other => invalid(
            format!("lowered{path}"),
            format!("expected an unsigned integer, found {other:?}"),
        ),
    }
}
