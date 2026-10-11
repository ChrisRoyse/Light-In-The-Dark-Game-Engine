//! #833 / #835: lower persisted trigger state to a fingerprinted `litd/data`
//! artifact the engine consumes. The lowered document carries the typed variable
//! table (with array shape + initial values) and the per-trigger organization
//! flags (folder/category, enable/disable, initially-on, comment). Lowering
//! re-validates every variable so a type mismatch cannot survive to runtime, and
//! `verify_lowered_trigger_document` re-checks types + fingerprint on read-back so
//! a sabotaged artifact (e.g. a variable's type flipped) fails loud.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::mcp_trigger_tools::mcp_trigger_tools_store::McpTriggerRuntimeState;
use crate::mcp_trigger_tools::{
    TriggerValueType, TriggerVariable, TriggerVariableScope, validate_trigger_variable,
};

pub const TRIGGER_LOWERING_SCHEMA_VERSION: u32 = 1;

/// A typed variable in the lowered artifact. `initial_values` holds 0..=1 entries
/// for a scalar and 0 or `array_size` entries for an array (0 == default-init).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredTriggerVariable {
    pub name: String,
    pub value_type: TriggerValueType,
    pub scope: TriggerVariableScope,
    pub is_array: bool,
    pub array_size: Option<u32>,
    pub initial_values: Vec<String>,
}

/// Per-trigger organization row in the lowered artifact (#835).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredTriggerOrg {
    pub trigger_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub initially_on: bool,
    pub folder_path: Option<String>,
    pub comment: Option<String>,
}

/// The full fingerprinted lowering the engine loads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredTriggerDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub variables: Vec<LoweredTriggerVariable>,
    pub triggers: Vec<LoweredTriggerOrg>,
}

fn lower_variable(variable: &TriggerVariable) -> LoweredTriggerVariable {
    let initial_values = if variable.is_array {
        variable.array_initial_values.clone()
    } else {
        variable.initial_value.clone().into_iter().collect()
    };
    LoweredTriggerVariable {
        name: variable.name.clone(),
        value_type: variable.value_type,
        scope: variable.scope,
        is_array: variable.is_array,
        array_size: variable.array_size,
        initial_values,
    }
}

/// Reconstruct the authoring-shaped variable so the shared validator can type-check it.
fn rehydrate_variable(lowered: &LoweredTriggerVariable) -> TriggerVariable {
    if lowered.is_array {
        TriggerVariable {
            name: lowered.name.clone(),
            value_type: lowered.value_type,
            scope: lowered.scope,
            is_array: true,
            initial_value: None,
            array_size: lowered.array_size,
            array_initial_values: lowered.initial_values.clone(),
        }
    } else {
        TriggerVariable {
            name: lowered.name.clone(),
            value_type: lowered.value_type,
            scope: lowered.scope,
            is_array: false,
            initial_value: lowered.initial_values.first().cloned(),
            array_size: None,
            array_initial_values: Vec::new(),
        }
    }
}

/// Lower the persisted trigger state into a fingerprinted document. Fails loud if
/// any variable does not type-check (never lowers a mismatch).
pub fn lower_trigger_state(
    state: &McpTriggerRuntimeState,
) -> Result<LoweredTriggerDocument, EngineError> {
    for variable in &state.variables {
        validate_trigger_variable(variable)?;
    }
    let variables = state.variables.iter().map(lower_variable).collect();
    let triggers = state
        .graphs
        .iter()
        .map(|graph| LoweredTriggerOrg {
            trigger_id: graph.trigger_id.clone(),
            display_name: graph.display_name.clone(),
            enabled: graph.enabled,
            initially_on: graph.initially_on,
            folder_path: graph.folder_path.clone(),
            comment: graph.comment.clone(),
        })
        .collect();
    let mut document = LoweredTriggerDocument {
        schema_version: TRIGGER_LOWERING_SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        variables,
        triggers,
    };
    document.lowering_fingerprint = document_fingerprint(&document)?;
    Ok(document)
}

/// Serialize a lowered document to canonical pretty JSON (the on-disk artifact).
pub fn encode_lowered_trigger_document(
    document: &LoweredTriggerDocument,
) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(document).map_err(|error| EngineError::SerializationFailed {
        format: "trigger-lowering-json",
        reason: error.to_string(),
    })
}

/// Parse + fully re-validate a lowered artifact: schema, fingerprint, and every
/// variable's type. A sabotaged type or a tampered value is rejected here.
pub fn verify_lowered_trigger_document(
    bytes: &[u8],
    expected_fingerprint: &str,
) -> Result<LoweredTriggerDocument, EngineError> {
    let document: LoweredTriggerDocument =
        serde_json::from_slice(bytes).map_err(|error| EngineError::SerializationFailed {
            format: "trigger-lowering-json",
            reason: error.to_string(),
        })?;
    if document.schema_version != TRIGGER_LOWERING_SCHEMA_VERSION {
        return Err(lowering_error(
            "schema_version",
            format!(
                "expected schema {TRIGGER_LOWERING_SCHEMA_VERSION}, got {}",
                document.schema_version
            ),
        ));
    }
    for lowered in &document.variables {
        validate_trigger_variable(&rehydrate_variable(lowered))?;
    }
    let actual = document_fingerprint(&document)?;
    if actual != expected_fingerprint || document.lowering_fingerprint != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(document)
}

fn document_fingerprint(document: &LoweredTriggerDocument) -> Result<String, EngineError> {
    let mut preimage = document.clone();
    preimage.lowering_fingerprint.clear();
    let bytes =
        serde_json::to_vec(&preimage).map_err(|error| EngineError::SerializationFailed {
            format: "trigger-lowering-fingerprint",
            reason: error.to_string(),
        })?;
    Ok(litd_hash::fnv1a64_hex(&bytes))
}

fn lowering_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.trigger_organization_lowering.{field}"),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "trigger_organization_lowering_tests.rs"]
mod tests;
