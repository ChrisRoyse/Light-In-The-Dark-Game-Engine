use std::path::Path;

use calyx_core::{SlotId, SlotVector};
use calyx_registry::{
    CapabilitySignalKind, LensRuntime, load_vault_panel_state, signal_kind_from_spec,
};
use litd_engine::calyx_content::{EngineCalyxContentError, EngineContentConstellationReadback};
use serde_json::{Value, json};

pub(super) fn content_readback_summary(
    readback: &EngineContentConstellationReadback,
    roster: &[Value],
) -> Value {
    json!({
        "row_id": readback.database.id,
        "row_key": readback.database.row_key,
        "row_sha256": readback.database.sha256,
        "graph_rows": readback.database.graph_rows,
        "cx_id": readback.cx_id,
        "modality": format!("{:?}", readback.constellation.modality),
        "ungrounded": readback.constellation.flags.ungrounded,
        "degraded": readback.constellation.flags.degraded,
        "persisted_anchor_count": readback.constellation.anchors.len(),
        "measurement": readback.measurement,
        "measured_real_roster": roster,
    })
}

pub(super) fn measured_real_roster(
    panel_vault: &Path,
    readback: &EngineContentConstellationReadback,
) -> Result<Vec<Value>, EngineCalyxContentError> {
    let state = load_vault_panel_state(panel_vault).map_err(|error| {
        EngineCalyxContentError::Measurement {
            phase: "fsv.load_panel".to_string(),
            reason: format!("{}: {}", error.code, error.message),
        }
    })?;
    let mut roster = Vec::new();
    for slot_id in &readback.measurement.measured_slot_ids {
        let slot_id = SlotId::new(*slot_id);
        let slot = state
            .panel
            .slots
            .iter()
            .find(|slot| slot.slot_id == slot_id)
            .ok_or_else(|| EngineCalyxContentError::Measurement {
                phase: "fsv.roster".to_string(),
                reason: format!("missing measured slot {}", slot_id.get()),
            })?;
        let spec = state.registry.lens_spec(slot.lens_id).ok_or_else(|| {
            EngineCalyxContentError::Measurement {
                phase: "fsv.roster".to_string(),
                reason: format!("missing LensSpec for slot {}", slot_id.get()),
            }
        })?;
        roster.push(json!({
            "slot_id": slot_id.get(),
            "slot_key": slot.slot_key.key(),
            "lens_id": slot.lens_id.to_string(),
            "lens_name": spec.name,
            "signal_kind": format!("{:?}", signal_kind_from_spec(spec)),
            "is_learned_encoder": signal_kind_from_spec(spec) == CapabilitySignalKind::LearnedEncoder,
            "runtime": runtime_label(&spec.runtime),
            "shape": format!("{:?}", slot.shape),
            "vector": readback.constellation.slots.get(&slot_id).map(slot_vector_summary).unwrap_or_else(|| json!({"kind": "missing"})),
            "non_dense_full_vector": non_dense_full_vector_metadata(&readback.constellation.metadata, slot_id)
        }));
    }
    Ok(roster)
}

fn non_dense_full_vector_metadata(
    metadata: &std::collections::BTreeMap<String, String>,
    slot_id: SlotId,
) -> Value {
    let prefix = format!("non_dense_slot_{}", slot_id.get());
    let mut out = serde_json::Map::new();
    for key in [
        "kind",
        "full_sha256",
        "full_bytes",
        "dim",
        "full_entries",
        "stored_preview_entries",
        "token_dim",
        "full_tokens",
        "full_floats",
        "stored_preview_tokens",
        "assay_schema",
        "assay_projection",
        "assay_source_dim",
        "assay_source_count",
        "assay_dim",
        "assay_vector_sha256",
    ] {
        let metadata_key = format!("{prefix}_{key}");
        if let Some(value) = metadata.get(&metadata_key) {
            out.insert(key.to_string(), json!(value));
        }
    }
    Value::Object(out)
}

fn slot_vector_summary(vector: &SlotVector) -> Value {
    match vector {
        SlotVector::Dense { dim, data } => json!({
            "kind": "dense",
            "dim": dim,
            "len": data.len(),
            "finite": data.iter().all(|value| value.is_finite()),
            "l2": data.iter().map(|value| value * value).sum::<f32>().sqrt()
        }),
        SlotVector::Sparse { entries, .. } => json!({
            "kind": "sparse",
            "entries": entries.len()
        }),
        SlotVector::Multi { token_dim, tokens } => json!({
            "kind": "multi",
            "token_dim": token_dim,
            "tokens": tokens.len()
        }),
        SlotVector::Absent { reason } => json!({
            "kind": "absent",
            "reason": format!("{:?}", reason)
        }),
    }
}

fn runtime_label(runtime: &LensRuntime) -> String {
    format!("{runtime:?}")
}
